use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use dex_client::{BirdEyeClient, BirdEyeConfig, BirdEyeError};
use persistence_layer::{RedisClient, PersistenceError};
use pnl_core::{PriceFetcher, Result as PnLResult, PnLError};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, info, warn, error};

#[derive(Error, Debug)]
pub enum BirdEyePriceError {
    #[error("BirdEye API error: {0}")]
    BirdEye(#[from] BirdEyeError),
    #[error("Persistence error: {0}")]
    Persistence(#[from] PersistenceError),
    #[error("Invalid price data: {0}")]
    InvalidPriceData(String),
    #[error("No price data found for token: {0}")]
    NoPriceData(String),
    #[error("Rate limit exceeded")]
    RateLimit,
}

/// BirdEye-based price fetcher with Redis caching
#[derive(Clone)]
pub struct BirdEyePriceFetcher {
    client: BirdEyeClient,
    redis_client: Arc<Mutex<Option<RedisClient>>>,
    cache_ttl_seconds: u64,
}

impl BirdEyePriceFetcher {
    pub fn new(
        config: BirdEyeConfig,
        redis_client: Option<RedisClient>,
        cache_ttl_seconds: Option<u64>,
    ) -> Result<Self> {
        let client = BirdEyeClient::new(config)?;
        
        Ok(Self {
            client,
            redis_client: Arc::new(Mutex::new(redis_client)),
            cache_ttl_seconds: cache_ttl_seconds.unwrap_or(300), // 5 minutes default
        })
    }

    /// Get cache key for current price
    fn current_price_cache_key(&self, token_mint: &str) -> String {
        format!("birdeye:price:current:{}", token_mint)
    }

    /// Get cache key for historical price
    fn historical_price_cache_key(&self, token_mint: &str, timestamp: i64) -> String {
        format!("birdeye:price:historical:{}:{}", token_mint, timestamp)
    }

    /// Cache current price in Redis
    async fn cache_current_price(&self, token_mint: &str, price: Decimal) -> Result<(), BirdEyePriceError> {
        let redis = self.redis_client.lock().await;
        if let Some(ref redis_client) = *redis {
            let cache_key = self.current_price_cache_key(token_mint);
            let price_str = price.to_string();
            
            match redis_client.set_with_expiry(&cache_key, &price_str, self.cache_ttl_seconds).await {
                Ok(_) => {
                    debug!("Cached current price for {}: ${}", token_mint, price);
                }
                Err(e) => {
                    warn!("Failed to cache current price for {}: {}", token_mint, e);
                }
            }
        }
        Ok(())
    }

    /// Get cached current price from Redis
    async fn get_cached_current_price(&self, token_mint: &str) -> Result<Option<Decimal>, BirdEyePriceError> {
        let redis = self.redis_client.lock().await;
        if let Some(ref redis_client) = *redis {
            let cache_key = self.current_price_cache_key(token_mint);
            
            match redis_client.get_cached_data(&cache_key).await {
                Ok(Some(price_str)) => {
                    match price_str.parse::<Decimal>() {
                        Ok(price) => {
                            debug!("Cache hit for current price {}: ${}", token_mint, price);
                            return Ok(Some(price));
                        }
                        Err(e) => {
                            warn!("Failed to parse cached price for {}: {}", token_mint, e);
                        }
                    }
                }
                Ok(None) => {
                    debug!("No cached current price for {}", token_mint);
                }
                Err(e) => {
                    warn!("Failed to get cached current price for {}: {}", token_mint, e);
                }
            }
        }
        Ok(None)
    }

    /// Cache historical price in Redis
    async fn cache_historical_price(&self, token_mint: &str, timestamp: i64, price: Decimal) -> Result<(), BirdEyePriceError> {
        let redis = self.redis_client.lock().await;
        if let Some(ref redis_client) = *redis {
            let cache_key = self.historical_price_cache_key(token_mint, timestamp);
            let price_str = price.to_string();
            
            // Historical prices can be cached for a longer time since they don't change
            let historical_cache_ttl = self.cache_ttl_seconds * 24; // 24x longer for historical
            
            match redis_client.set_with_expiry(&cache_key, &price_str, historical_cache_ttl).await {
                Ok(_) => {
                    debug!("Cached historical price for {} at {}: ${}", token_mint, timestamp, price);
                }
                Err(e) => {
                    warn!("Failed to cache historical price for {} at {}: {}", token_mint, timestamp, e);
                }
            }
        }
        Ok(())
    }

    /// Get cached historical price from Redis
    async fn get_cached_historical_price(&self, token_mint: &str, timestamp: i64) -> Result<Option<Decimal>, BirdEyePriceError> {
        let redis = self.redis_client.lock().await;
        if let Some(ref redis_client) = *redis {
            let cache_key = self.historical_price_cache_key(token_mint, timestamp);
            
            match redis_client.get_cached_data(&cache_key).await {
                Ok(Some(price_str)) => {
                    match price_str.parse::<Decimal>() {
                        Ok(price) => {
                            debug!("Cache hit for historical price {} at {}: ${}", token_mint, timestamp, price);
                            return Ok(Some(price));
                        }
                        Err(e) => {
                            warn!("Failed to parse cached historical price for {} at {}: {}", token_mint, timestamp, e);
                        }
                    }
                }
                Ok(None) => {
                    debug!("No cached historical price for {} at {}", token_mint, timestamp);
                }
                Err(e) => {
                    warn!("Failed to get cached historical price for {} at {}: {}", token_mint, timestamp, e);
                }
            }
        }
        Ok(None)
    }

    /// Fetch current price from BirdEye API with caching
    async fn fetch_current_price_cached(&self, token_mint: &str) -> Result<Decimal, BirdEyePriceError> {
        // Try cache first
        if let Some(cached_price) = self.get_cached_current_price(token_mint).await? {
            return Ok(cached_price);
        }

        // Fetch from API
        match self.client.get_current_price(token_mint).await {
            Ok(price_f64) => {
                let price = Decimal::from_f64_retain(price_f64)
                    .ok_or_else(|| BirdEyePriceError::InvalidPriceData(
                        format!("Invalid price value: {}", price_f64)
                    ))?;
                
                // Cache the result
                let _ = self.cache_current_price(token_mint, price).await;
                
                info!("Fetched current price from BirdEye for {}: ${}", token_mint, price);
                Ok(price)
            }
            Err(BirdEyeError::RateLimit) => {
                warn!("BirdEye rate limit hit for current price: {}", token_mint);
                Err(BirdEyePriceError::RateLimit)
            }
            Err(e) => {
                error!("Failed to fetch current price from BirdEye for {}: {}", token_mint, e);
                Err(BirdEyePriceError::BirdEye(e))
            }
        }
    }

    /// Fetch historical price from BirdEye API with caching
    async fn fetch_historical_price_cached(&self, token_mint: &str, timestamp: i64) -> Result<Decimal, BirdEyePriceError> {
        // Try cache first
        if let Some(cached_price) = self.get_cached_historical_price(token_mint, timestamp).await? {
            return Ok(cached_price);
        }

        // Fetch from API
        match self.client.get_historical_price(token_mint, timestamp).await {
            Ok(price_f64) => {
                let price = Decimal::from_f64_retain(price_f64)
                    .ok_or_else(|| BirdEyePriceError::InvalidPriceData(
                        format!("Invalid historical price value: {}", price_f64)
                    ))?;
                
                // Cache the result
                let _ = self.cache_historical_price(token_mint, timestamp, price).await;
                
                info!("Fetched historical price from BirdEye for {} at {}: ${}", token_mint, timestamp, price);
                Ok(price)
            }
            Err(BirdEyeError::RateLimit) => {
                warn!("BirdEye rate limit hit for historical price: {} at {}", token_mint, timestamp);
                Err(BirdEyePriceError::RateLimit)
            }
            Err(e) => {
                error!("Failed to fetch historical price from BirdEye for {} at {}: {}", token_mint, timestamp, e);
                Err(BirdEyePriceError::BirdEye(e))
            }
        }
    }
}

#[async_trait]
impl PriceFetcher for BirdEyePriceFetcher {
    /// Fetch current prices for multiple tokens
    async fn fetch_prices(
        &self,
        token_mints: &[String],
        _vs_token: Option<&str>, // BirdEye prices are typically in USD
    ) -> PnLResult<HashMap<String, Decimal>> {
        debug!("Fetching current prices for {} tokens via BirdEye", token_mints.len());
        
        let mut prices = HashMap::new();
        let mut uncached_tokens = Vec::new();
        
        // Check cache for all tokens first
        for token_mint in token_mints {
            match self.get_cached_current_price(token_mint).await {
                Ok(Some(cached_price)) => {
                    prices.insert(token_mint.clone(), cached_price);
                }
                Ok(None) => {
                    uncached_tokens.push(token_mint.clone());
                }
                Err(e) => {
                    warn!("Cache error for token {}: {}", token_mint, e);
                    uncached_tokens.push(token_mint.clone());
                }
            }
        }

        // Fetch uncached tokens from API
        if !uncached_tokens.is_empty() {
            debug!("Fetching {} uncached tokens from BirdEye API", uncached_tokens.len());
            
            // Use batch API if available, otherwise fetch individually
            if uncached_tokens.len() > 1 {
                match self.client.get_current_prices(&uncached_tokens).await {
                    Ok(api_prices) => {
                        for (token_mint, price_f64) in api_prices {
                            if let Ok(price) = Decimal::from_f64_retain(price_f64)
                                .ok_or_else(|| BirdEyePriceError::InvalidPriceData(
                                    format!("Invalid price value: {}", price_f64)
                                )) {
                                prices.insert(token_mint.clone(), price);
                                
                                // Cache the result
                                let _ = self.cache_current_price(&token_mint, price).await;
                            }
                        }
                    }
                    Err(BirdEyeError::RateLimit) => {
                        warn!("BirdEye rate limit hit for batch prices");
                        return Err(PnLError::PriceFetch("Rate limit exceeded".to_string()));
                    }
                    Err(e) => {
                        warn!("Batch price fetch failed, falling back to individual requests: {}", e);
                        
                        // Fallback to individual requests
                        for token_mint in &uncached_tokens {
                            match self.fetch_current_price_cached(token_mint).await {
                                Ok(price) => {
                                    prices.insert(token_mint.clone(), price);
                                }
                                Err(BirdEyePriceError::RateLimit) => {
                                    warn!("Rate limit hit for token: {}", token_mint);
                                    // Continue with other tokens
                                }
                                Err(e) => {
                                    warn!("Failed to fetch price for token {}: {}", token_mint, e);
                                    // Continue with other tokens
                                }
                            }
                            
                            // Add delay to respect rate limits
                            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        }
                    }
                }
            } else if uncached_tokens.len() == 1 {
                let token_mint = &uncached_tokens[0];
                match self.fetch_current_price_cached(token_mint).await {
                    Ok(price) => {
                        prices.insert(token_mint.clone(), price);
                    }
                    Err(BirdEyePriceError::RateLimit) => {
                        return Err(PnLError::PriceFetch("Rate limit exceeded".to_string()));
                    }
                    Err(e) => {
                        warn!("Failed to fetch price for token {}: {}", token_mint, e);
                    }
                }
            }
        }

        info!("Successfully fetched prices for {}/{} tokens", prices.len(), token_mints.len());
        Ok(prices)
    }

    /// Fetch historical price for a token at a specific time
    async fn fetch_historical_price(
        &self,
        token_mint: &str,
        timestamp: DateTime<Utc>,
        _vs_token: Option<&str>, // BirdEye prices are typically in USD
    ) -> PnLResult<Option<Decimal>> {
        debug!("Fetching historical price for {} at {}", token_mint, timestamp);
        
        let unix_timestamp = timestamp.timestamp();
        
        match self.fetch_historical_price_cached(token_mint, unix_timestamp).await {
            Ok(price) => {
                debug!("Successfully fetched historical price for {} at {}: ${}", 
                       token_mint, timestamp, price);
                Ok(Some(price))
            }
            Err(BirdEyePriceError::RateLimit) => {
                warn!("Rate limit hit for historical price: {} at {}", token_mint, timestamp);
                Err(PnLError::PriceFetch("Rate limit exceeded".to_string()))
            }
            Err(BirdEyePriceError::NoPriceData(_)) => {
                debug!("No historical price data available for {} at {}", token_mint, timestamp);
                Ok(None)
            }
            Err(e) => {
                warn!("Failed to fetch historical price for {} at {}: {}", token_mint, timestamp, e);
                // Return None instead of error to allow fallback to current price
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_generation() {
        let fetcher = BirdEyePriceFetcher {
            client: BirdEyeClient::new(BirdEyeConfig::default()).unwrap(),
            redis_client: Arc::new(Mutex::new(None)),
            cache_ttl_seconds: 300,
        };

        let token = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
        let current_key = fetcher.current_price_cache_key(token);
        let historical_key = fetcher.historical_price_cache_key(token, 1640995200);

        assert_eq!(current_key, format!("birdeye:price:current:{}", token));
        assert_eq!(historical_key, format!("birdeye:price:historical:{}:1640995200", token));
    }
}