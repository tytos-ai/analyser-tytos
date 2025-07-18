use async_trait::async_trait;
use chrono::{DateTime, Utc};
use config_manager::{PriceFetchingConfig, BirdEyeConfig};
use pnl_core::{PriceFetcher, Result as PnLResult, PnLError};
use reqwest::Client;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, error, info, warn};

use crate::{BirdEyeClient, BirdEyeError};

#[derive(Error, Debug)]
pub enum PriceFetchingError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),
    #[error("BirdEye API error: {0}")]
    BirdEyeError(#[from] BirdEyeError),
    #[error("JSON parsing error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Configuration error: {0}")]
    ConfigError(String),
    #[error("All price sources failed: {0}")]
    AllSourcesFailed(String),
    #[error("Invalid response from Jupiter API: {0}")]
    InvalidJupiterResponse(String),
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
}

pub type Result<T> = std::result::Result<T, PriceFetchingError>;

/// Jupiter API V3 response structure for price data
/// Direct mapping to token mint -> price data (no nested 'data' field)
pub type JupiterPriceResponse = HashMap<String, JupiterPriceData>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JupiterPriceData {
    #[serde(rename = "usdPrice")]
    pub usd_price: f64,
    #[serde(rename = "blockId")]
    pub block_id: u64,
    pub decimals: u8,
    #[serde(rename = "priceChange24h")]
    pub price_change_24h: f64,
}

/// Jupiter API response structure for historical price data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JupiterHistoricalPriceResponse {
    pub data: Vec<JupiterHistoricalPriceData>,
    #[serde(rename = "timeTaken")]
    pub time_taken: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JupiterHistoricalPriceData {
    pub mint: String,
    pub timestamp: i64,
    pub price: f64,
    pub vs_token: String,
}

/// Birdeye Historical Price API response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BirdeyeHistoricalPriceResponse {
    pub success: bool,
    pub data: BirdeyeHistoricalPriceData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BirdeyeHistoricalPriceData {
    pub items: Vec<BirdeyeHistoricalPriceItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BirdeyeHistoricalPriceItem {
    pub unixtime: i64,
    pub value: f64,
    pub address: String,
}

/// Unified price fetching service that can use multiple sources with fallback
#[derive(Debug, Clone)]
pub struct PriceFetchingService {
    config: PriceFetchingConfig,
    birdeye_client: Option<BirdEyeClient>,
    http_client: Client,
}

impl PriceFetchingService {
    /// Create a new price fetching service with the given configuration
    pub fn new(config: PriceFetchingConfig, birdeye_config: Option<BirdEyeConfig>) -> Result<Self> {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(config.request_timeout_seconds))
            .user_agent("wallet-analyzer-price-fetcher/1.0")
            .build()
            .map_err(|e| PriceFetchingError::ConfigError(format!("Failed to create HTTP client: {}", e)))?;

        let birdeye_client = if let Some(birdeye_cfg) = birdeye_config {
            Some(BirdEyeClient::new(birdeye_cfg).map_err(|e| {
                PriceFetchingError::ConfigError(format!("Failed to create BirdEye client: {}", e))
            })?)
        } else {
            None
        };

        Ok(Self {
            config,
            birdeye_client,
            http_client,
        })
    }

    /// Fetch current prices from Jupiter API
    pub async fn fetch_jupiter_prices(&self, token_mints: &[String]) -> Result<HashMap<String, f64>> {
        if token_mints.is_empty() {
            return Ok(HashMap::new());
        }

        let token_list = token_mints.join(",");
        let url = format!("{}/price/v3?ids={}", self.config.jupiter_api_url, token_list);
        
        debug!("Fetching Jupiter V3 prices for {} tokens", token_mints.len());
        
        let response = self.http_client
            .get(&url)
            .send()
            .await?;

        if response.status() == 429 {
            return Err(PriceFetchingError::RateLimitExceeded);
        }

        if !response.status().is_success() {
            return Err(PriceFetchingError::InvalidJupiterResponse(
                format!("HTTP {}", response.status())
            ));
        }

        let jupiter_response: JupiterPriceResponse = response.json().await?;
        
        let mut prices = HashMap::new();
        for (mint, price_data) in jupiter_response {
            prices.insert(mint, price_data.usd_price);
        }

        debug!("Retrieved Jupiter V3 prices for {}/{} tokens", prices.len(), token_mints.len());
        Ok(prices)
    }

    /// Fetch historical price from Jupiter API
    pub async fn fetch_jupiter_historical_price(
        &self,
        token_mint: &str,
        timestamp: DateTime<Utc>,
        vs_token: Option<&str>,
    ) -> Result<Option<f64>> {
        let vs_token = vs_token.unwrap_or("USDC");
        let timestamp_unix = timestamp.timestamp();
        
        let url = format!(
            "{}/price/v2/history?mint={}&vs_token={}&timestamp={}", 
            self.config.jupiter_api_url, token_mint, vs_token, timestamp_unix
        );
        
        debug!("Fetching Jupiter historical price for {} at {}", token_mint, timestamp);
        
        let response = self.http_client
            .get(&url)
            .send()
            .await?;

        if response.status() == 429 {
            return Err(PriceFetchingError::RateLimitExceeded);
        }

        if !response.status().is_success() {
            return Err(PriceFetchingError::InvalidJupiterResponse(
                format!("HTTP {}", response.status())
            ));
        }

        let jupiter_response: JupiterHistoricalPriceResponse = response.json().await?;
        
        // Find the price closest to the requested timestamp
        let price = jupiter_response.data
            .into_iter()
            .min_by_key(|item| (item.timestamp - timestamp_unix).abs())
            .map(|item| item.price);

        debug!("Retrieved Jupiter historical price for {}: {:?}", token_mint, price);
        Ok(price)
    }

    /// Fetch historical price from Birdeye API
    pub async fn fetch_birdeye_historical_price(
        &self,
        token_mint: &str,
        timestamp: DateTime<Utc>,
        _vs_token: Option<&str>,
    ) -> Result<Option<f64>> {
        let birdeye_client = self.birdeye_client.as_ref()
            .ok_or_else(|| PriceFetchingError::ConfigError("BirdEye client not configured".to_string()))?;

        let timestamp_unix = timestamp.timestamp();
        
        let url = format!("{}/defi/history_price", birdeye_client.config().api_base_url);
        
        debug!("Fetching Birdeye historical price for {} at {}", token_mint, timestamp);
        
        let response = self.http_client
            .get(&url)
            .header("X-API-KEY", &birdeye_client.config().api_key)
            .query(&[
                ("address", token_mint),
                ("address_type", "token"),
                ("type", "1D"),
                ("time_from", &timestamp_unix.to_string()),
                ("time_to", &(timestamp_unix + 86400).to_string()), // +1 day
            ])
            .send()
            .await?;

        if response.status() == 429 {
            return Err(PriceFetchingError::RateLimitExceeded);
        }

        if !response.status().is_success() {
            return Err(PriceFetchingError::BirdEyeError(
                BirdEyeError::Api(format!("HTTP {}", response.status()))
            ));
        }

        let birdeye_response: BirdeyeHistoricalPriceResponse = response.json().await?;
        
        if !birdeye_response.success {
            return Err(PriceFetchingError::BirdEyeError(
                BirdEyeError::Api("API returned success=false".to_string())
            ));
        }

        // Find the price closest to the requested timestamp
        let price = birdeye_response.data.items
            .into_iter()
            .min_by_key(|item| (item.unixtime - timestamp_unix).abs())
            .map(|item| item.value);

        debug!("Retrieved Birdeye historical price for {}: {:?}", token_mint, price);
        Ok(price)
    }

    /// Fetch current prices with fallback logic
    async fn fetch_current_prices_with_fallback(&self, token_mints: &[String]) -> Result<HashMap<String, f64>> {
        let primary_source = &self.config.primary_source;
        let fallback_source = &self.config.fallback_source;

        debug!("Fetching current prices with primary: {}, fallback: {}", primary_source, fallback_source);

        // Try primary source first
        let primary_result = match primary_source.as_str() {
            "jupiter" => self.fetch_jupiter_prices(token_mints).await,
            "birdeye" => {
                if let Some(ref birdeye_client) = self.birdeye_client {
                    birdeye_client.get_current_prices(token_mints).await
                        .map_err(PriceFetchingError::BirdEyeError)
                } else {
                    Err(PriceFetchingError::ConfigError("BirdEye client not configured".to_string()))
                }
            }
            "both" => {
                // For "both", try Jupiter first, then merge with Birdeye
                let mut jupiter_prices = self.fetch_jupiter_prices(token_mints).await.unwrap_or_default();
                
                if let Some(ref birdeye_client) = self.birdeye_client {
                    if let Ok(birdeye_prices) = birdeye_client.get_current_prices(token_mints).await {
                        // Merge prices, Jupiter takes precedence
                        for (mint, price) in birdeye_prices {
                            jupiter_prices.entry(mint).or_insert(price);
                        }
                    }
                }
                
                Ok(jupiter_prices)
            }
            _ => Err(PriceFetchingError::ConfigError(
                format!("Invalid primary source: {}", primary_source)
            )),
        };

        match primary_result {
            Ok(prices) if !prices.is_empty() => {
                info!("Successfully fetched {} prices from primary source: {}", prices.len(), primary_source);
                Ok(prices)
            }
            Ok(_) | Err(_) => {
                // Primary source failed or returned no prices, try fallback
                warn!("Primary source {} failed or returned no prices, trying fallback: {}", primary_source, fallback_source);
                
                let fallback_result = match fallback_source.as_str() {
                    "jupiter" => self.fetch_jupiter_prices(token_mints).await,
                    "birdeye" => {
                        if let Some(ref birdeye_client) = self.birdeye_client {
                            birdeye_client.get_current_prices(token_mints).await
                                .map_err(PriceFetchingError::BirdEyeError)
                        } else {
                            Err(PriceFetchingError::ConfigError("BirdEye client not configured".to_string()))
                        }
                    }
                    _ => Err(PriceFetchingError::ConfigError(
                        format!("Invalid fallback source: {}", fallback_source)
                    )),
                };

                match fallback_result {
                    Ok(prices) => {
                        info!("Successfully fetched {} prices from fallback source: {}", prices.len(), fallback_source);
                        Ok(prices)
                    }
                    Err(e) => {
                        error!("Both primary and fallback sources failed");
                        Err(PriceFetchingError::AllSourcesFailed(
                            format!("Primary ({}) and fallback ({}) both failed: {}", primary_source, fallback_source, e)
                        ))
                    }
                }
            }
        }
    }

    /// Fetch historical price with fallback logic
    async fn fetch_historical_price_with_fallback(
        &self,
        token_mint: &str,
        timestamp: DateTime<Utc>,
        vs_token: Option<&str>,
    ) -> Result<Option<f64>> {
        let primary_source = &self.config.primary_source;
        let fallback_source = &self.config.fallback_source;

        debug!("Fetching historical price for {} with primary: {}, fallback: {}", token_mint, primary_source, fallback_source);

        // For historical prices, prioritize BirdEye since Jupiter V3 doesn't support historical data
        let primary_result = match primary_source.as_str() {
            "jupiter" => {
                // Jupiter V3 doesn't support historical prices, use BirdEye instead
                debug!("Jupiter V3 doesn't support historical prices, using BirdEye");
                self.fetch_birdeye_historical_price(token_mint, timestamp, vs_token).await
            },
            "birdeye" => self.fetch_birdeye_historical_price(token_mint, timestamp, vs_token).await,
            "both" => {
                // Use BirdEye for historical prices since Jupiter V3 doesn't support it
                self.fetch_birdeye_historical_price(token_mint, timestamp, vs_token).await
            }
            _ => Err(PriceFetchingError::ConfigError(
                format!("Invalid primary source: {}", primary_source)
            )),
        };

        match primary_result {
            Ok(Some(price)) => {
                debug!("Successfully fetched historical price from primary source: {} = {}", token_mint, price);
                Ok(Some(price))
            }
            Ok(None) | Err(_) => {
                // Primary source failed or returned no price, try fallback
                warn!("Primary source {} failed or returned no price for {}, trying fallback: {}", primary_source, token_mint, fallback_source);
                
                let fallback_result = match fallback_source.as_str() {
                    "jupiter" => {
                        // Jupiter V3 doesn't support historical prices, use BirdEye instead
                        debug!("Jupiter V3 doesn't support historical prices, using BirdEye for fallback");
                        self.fetch_birdeye_historical_price(token_mint, timestamp, vs_token).await
                    },
                    "birdeye" => self.fetch_birdeye_historical_price(token_mint, timestamp, vs_token).await,
                    _ => Err(PriceFetchingError::ConfigError(
                        format!("Invalid fallback source: {}", fallback_source)
                    )),
                };

                match fallback_result {
                    Ok(Some(price)) => {
                        debug!("Successfully fetched historical price from fallback source: {} = {}", token_mint, price);
                        Ok(Some(price))
                    }
                    Ok(None) => {
                        debug!("No historical price found for {} from any source", token_mint);
                        Ok(None)
                    }
                    Err(e) => {
                        error!("Both primary and fallback sources failed for historical price of {}: {}", token_mint, e);
                        Ok(None) // Return None instead of error to avoid breaking P&L calculation
                    }
                }
            }
        }
    }
}

#[async_trait]
impl PriceFetcher for PriceFetchingService {
    async fn fetch_prices(
        &self,
        token_mints: &[String],
        _vs_token: Option<&str>,
    ) -> PnLResult<HashMap<String, Decimal>> {
        match self.fetch_current_prices_with_fallback(token_mints).await {
            Ok(prices) => {
                let mut result = HashMap::new();
                for (mint, price) in prices {
                    result.insert(mint, Decimal::from_f64_retain(price).unwrap_or(Decimal::ZERO));
                }
                Ok(result)
            }
            Err(e) => {
                error!("Failed to fetch prices: {}", e);
                Err(PnLError::PriceFetch(format!("Price fetching failed: {}", e)))
            }
        }
    }

    async fn fetch_historical_price(
        &self,
        token_mint: &str,
        timestamp: DateTime<Utc>,
        vs_token: Option<&str>,
    ) -> PnLResult<Option<Decimal>> {
        match self.fetch_historical_price_with_fallback(token_mint, timestamp, vs_token).await {
            Ok(Some(price)) => Ok(Some(Decimal::from_f64_retain(price).unwrap_or(Decimal::ZERO))),
            Ok(None) => Ok(None),
            Err(e) => {
                warn!("Failed to fetch historical price for {}: {}", token_mint, e);
                Ok(None) // Return None instead of error to avoid breaking P&L calculation
            }
        }
    }
}

