use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use config_manager::BirdEyeConfig;
use pnl_core::{PriceFetcher, Result as PnLResult};
use reqwest::Client;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, info, error, warn};

#[derive(Error, Debug)]
pub enum BirdEyeError {
    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error: {0}")]
    Api(String),
    #[error("Rate limit exceeded")]
    RateLimit,
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    #[error("Authentication error")]
    Auth,
}

// BirdEye API client configuration moved to config_manager crate

/// Trending token response from BirdEye
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendingTokenResponse {
    pub success: bool,
    pub data: TrendingTokenData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendingTokenData {
    pub tokens: Vec<TrendingToken>,
    pub total: Option<u32>,
    #[serde(rename = "updateUnixTime")]
    pub update_unix_time: Option<i64>,
    #[serde(rename = "updateHumanTime")]
    pub update_human_time: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendingToken {
    pub address: String,
    pub symbol: String,
    pub name: String,
    pub decimals: Option<u8>,
    pub price: f64,
    #[serde(rename = "price24hChangePercent")]
    pub price_change_24h: Option<f64>,
    #[serde(rename = "volume24hUSD")]
    pub volume_24h: Option<f64>,
    #[serde(rename = "volume24hChangePercent")]
    pub volume_change_24h: Option<f64>,
    pub liquidity: Option<f64>,
    pub fdv: Option<f64>,
    pub marketcap: Option<f64>,
    pub rank: Option<u32>,
    #[serde(rename = "logoURI")]
    pub logo_uri: Option<String>,
    #[serde(rename = "txns24h")]
    pub txns_24h: Option<u32>,
    #[serde(rename = "lastTradeUnixTime")]
    pub last_trade_unix_time: Option<i64>,
}

/// Top traders response from BirdEye
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopTradersResponse {
    pub success: bool,
    pub data: TopTradersData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopTradersData {
    pub items: Vec<TopTrader>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopTrader {
    #[serde(rename = "tokenAddress")]
    pub token_address: String,
    pub owner: String, // This is the wallet address
    pub tags: Vec<String>,
    #[serde(rename = "type")]
    pub trader_type: String, // "24h"
    pub volume: f64,
    pub trade: u32,
    #[serde(rename = "tradeBuy")]
    pub trade_buy: u32,
    #[serde(rename = "tradeSell")]
    pub trade_sell: u32,
    #[serde(rename = "volumeBuy")]
    pub volume_buy: f64,
    #[serde(rename = "volumeSell")]
    pub volume_sell: f64,
}

/// Trader transactions response from BirdEye
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraderTxsResponse {
    pub success: bool,
    pub data: TraderTxsData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraderTxsData {
    pub items: Vec<TraderTransaction>,
    #[serde(rename = "hasNext")]
    pub has_next: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraderTransaction {
    #[serde(rename = "txHash")]
    pub tx_hash: String,
    #[serde(rename = "blockUnixTime")]
    pub block_unix_time: i64,
    #[serde(rename = "blockHumanTime")]
    pub block_human_time: String,
    pub side: String, // "buy" or "sell"
    #[serde(rename = "tokenAddress")]
    pub token_address: String,
    #[serde(rename = "tokenSymbol")]
    pub token_symbol: Option<String>,
    #[serde(rename = "tokenAmount")]
    pub token_amount: f64,
    #[serde(rename = "tokenPrice")]
    pub token_price: f64,
    #[serde(rename = "volumeUsd")]
    pub volume_usd: f64,
    pub source: Option<String>,
    #[serde(rename = "poolAddress")]
    pub pool_address: Option<String>,
}

/// General trader transactions response from BirdEye (/trader/txs/seek_by_time)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralTraderTransactionsResponse {
    pub success: bool,
    pub data: GeneralTraderTransactionsData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralTraderTransactionsData {
    pub items: Vec<GeneralTraderTransaction>,
    pub has_next: Option<bool>, // Make optional since it may not always be present
}

/// Single transaction from general BirdEye trader API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralTraderTransaction {
    pub quote: TokenTransactionSide,
    pub base: TokenTransactionSide,
    #[serde(rename = "base_price")]
    pub base_price: Option<f64>,
    #[serde(rename = "quote_price")]
    #[serde(deserialize_with = "deserialize_nullable_f64")]
    pub quote_price: f64,
    #[serde(rename = "tx_hash")]
    pub tx_hash: String,
    pub source: String,
    #[serde(rename = "block_unix_time")]
    pub block_unix_time: i64,
    #[serde(rename = "tx_type")]
    #[serde(default = "default_tx_type")]
    pub tx_type: String, // "swap"
    #[serde(default)]
    pub address: String, // Program address
    #[serde(default)]
    pub owner: String,   // Wallet address
}

/// Token side of a transaction (quote or base)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTransactionSide {
    #[serde(default = "default_symbol")]
    pub symbol: String, // Make resilient to missing symbol
    #[serde(default)]
    pub decimals: u32,
    #[serde(deserialize_with = "deserialize_nullable_string")]
    pub address: String, // Make resilient to null values
    #[serde(deserialize_with = "deserialize_amount")]
    pub amount: u128,
    #[serde(rename = "type")]
    pub transfer_type: Option<String>, // "transfer", "transferChecked", "split", "burn", "mintTo", etc.
    #[serde(rename = "type_swap")]
    #[serde(deserialize_with = "deserialize_nullable_string")]
    pub type_swap: String, // "from", "to" - Make resilient to null values
    #[serde(rename = "ui_amount")]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_optional_nullable_f64")]
    pub ui_amount: f64, // Make resilient to missing/null values
    pub price: Option<f64>,
    #[serde(rename = "nearest_price")]
    pub nearest_price: Option<f64>,
    #[serde(rename = "change_amount")]
    #[serde(deserialize_with = "deserialize_signed_amount")]
    pub change_amount: i128,
    #[serde(rename = "ui_change_amount")]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_optional_nullable_f64")]
    pub ui_change_amount: f64, // Make resilient to missing/null values
    #[serde(rename = "fee_info")]
    pub fee_info: Option<serde_json::Value>,
}

/// Historical price response from BirdEye
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalPriceResponse {
    pub success: bool,
    pub data: HistoricalPriceData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalPriceData {
    pub value: f64,
    #[serde(rename = "updateUnixTime")]
    pub update_unix_time: i64,
    #[serde(rename = "updateHumanTime")]
    pub update_human_time: String,
}

/// Current price response from BirdEye
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceResponse {
    pub success: bool,
    pub data: PriceData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceData {
    pub value: f64,
    #[serde(rename = "updateUnixTime")]
    pub update_unix_time: i64,
    #[serde(rename = "updateHumanTime")]
    pub update_human_time: String,
}

/// Multi-price response from BirdEye
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiPriceResponse {
    pub success: bool,
    pub data: HashMap<String, TokenPriceData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPriceData {
    pub value: f64,
    #[serde(rename = "updateUnixTime")]
    pub update_unix_time: i64,
    #[serde(rename = "updateHumanTime")]
    pub update_human_time: String,
    #[serde(rename = "priceChange24h")]
    pub price_change_24h: Option<f64>,
    #[serde(rename = "priceInNative")]
    pub price_in_native: Option<f64>,
    pub liquidity: Option<f64>,
}

/// BirdEye API client
#[derive(Clone)]
pub struct BirdEyeClient {
    config: BirdEyeConfig,
    http_client: Client,
}

impl BirdEyeClient {
    pub fn new(config: BirdEyeConfig) -> Result<Self> {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(config.request_timeout_seconds))
            .build()?;

        Ok(Self {
            config,
            http_client,
        })
    }

    /// Get trending tokens from BirdEye
    pub async fn get_trending_tokens(&self, chain: &str) -> Result<Vec<TrendingToken>, BirdEyeError> {
        let url = format!("{}/defi/token_trending", self.config.api_base_url);
        
        debug!("Fetching trending tokens from BirdEye for chain: {}", chain);
        
        let response = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .query(&[("chain", chain)])
            .send()
            .await?;

        if response.status() == 429 {
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            return Err(BirdEyeError::Api(format!("HTTP {}", response.status())));
        }

        let trending_response: TrendingTokenResponse = response.json().await?;
        
        if !trending_response.success {
            return Err(BirdEyeError::Api("API returned success=false".to_string()));
        }

        info!("Retrieved {} trending tokens from BirdEye", trending_response.data.tokens.len());
        Ok(trending_response.data.tokens)
    }

    /// Get top traders for a specific token
    pub async fn get_top_traders(&self, token_address: &str, limit: Option<u32>) -> Result<Vec<TopTrader>, BirdEyeError> {
        let url = format!("{}/defi/v2/tokens/top_traders", self.config.api_base_url);
        
        debug!("Fetching top traders from BirdEye for token: {}", token_address);
        
        let mut query_params = vec![
            ("address", token_address),
            ("time_frame", "24h"),
            ("sort_type", "desc"),
            ("sort_by", "volume"),
            ("offset", "0"),
        ];
        
        let limit_string = limit.unwrap_or(10).to_string();
        query_params.push(("limit", &limit_string));

        let response = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .header("x-chain", "solana")
            .header("accept", "application/json")
            .query(&query_params)
            .send()
            .await?;

        if response.status() == 429 {
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_else(|_| "Failed to read error body".to_string());
            error!("BirdEye API error for top traders {}: HTTP {} - Body: {}", token_address, status, error_body);
            return Err(BirdEyeError::Api(format!("HTTP {} - {}", status, error_body)));
        }

        let top_traders_response: TopTradersResponse = response.json().await?;
        
        if !top_traders_response.success {
            return Err(BirdEyeError::Api("API returned success=false".to_string()));
        }

        info!("Retrieved {} top traders from BirdEye for token {}", 
              top_traders_response.data.items.len(), token_address);
        Ok(top_traders_response.data.items)
    }

    /// Get transaction history for a specific trader and token
    pub async fn get_trader_transactions(
        &self,
        wallet_address: &str,
        token_address: &str,
        from_time: Option<i64>,
        to_time: Option<i64>,
        limit: Option<u32>,
    ) -> Result<Vec<TraderTransaction>, BirdEyeError> {
        let url = format!("{}/trader/txs/seek_by_time", self.config.api_base_url);
        
        debug!("Fetching trader transactions from BirdEye for wallet: {}", wallet_address);
        
        let mut query_params = vec![
            ("address", wallet_address),
        ];
        
        let from_string;
        let to_string;
        let limit_string;
        
        if let Some(from) = from_time {
            from_string = from.to_string();
            query_params.push(("after_time", &from_string));
        }
        if let Some(to) = to_time {
            to_string = to.to_string();
            query_params.push(("before_time", &to_string));
        }
        if let Some(limit_val) = limit {
            limit_string = limit_val.to_string();
            query_params.push(("limit", &limit_string));
        }

        let response = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .header("x-chain", "solana")
            .query(&query_params)
            .send()
            .await?;

        if response.status() == 429 {
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            return Err(BirdEyeError::Api(format!("HTTP {}", response.status())));
        }

        let trader_txs_response: TraderTxsResponse = response.json().await?;
        
        if !trader_txs_response.success {
            return Err(BirdEyeError::Api("API returned success=false".to_string()));
        }

        debug!("Retrieved {} transactions from BirdEye for wallet {} token {}", 
               trader_txs_response.data.items.len(), wallet_address, token_address);
        Ok(trader_txs_response.data.items)
    }

    /// Get all trader transactions for a wallet (general endpoint without token filter)
    pub async fn get_all_trader_transactions(
        &self,
        wallet_address: &str,
        from_time: Option<i64>,
        to_time: Option<i64>,
        limit: Option<u32>,
    ) -> Result<Vec<GeneralTraderTransaction>, BirdEyeError> {
        let url = format!("{}/trader/txs/seek_by_time", self.config.api_base_url);
        
        debug!("Fetching all trader transactions from BirdEye for wallet: {}", wallet_address);
        
        let mut query_params = vec![
            ("address", wallet_address),
        ];
        
        let from_string;
        let to_string;
        let limit_string;
        
        if let Some(from) = from_time {
            from_string = from.to_string();
            query_params.push(("after_time", &from_string));
        }
        if let Some(to) = to_time {
            to_string = to.to_string();
            query_params.push(("before_time", &to_string));
        }
        if let Some(limit_val) = limit {
            limit_string = limit_val.to_string();
            query_params.push(("limit", &limit_string));
        }

        let request = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .header("x-chain", "solana")
            .query(&query_params);
            
        debug!("Making BirdEye API request to: {} with params: {:?}", url, query_params);
        
        let response = request.send().await?;

        debug!("BirdEye API response status: {} for wallet: {}", response.status(), wallet_address);

        if response.status() == 429 {
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_else(|_| "Failed to read error body".to_string());
            error!("BirdEye API error for wallet {}: HTTP {} - Body: {}", wallet_address, status, error_body);
            return Err(BirdEyeError::Api(format!("HTTP {} - {}", status, error_body)));
        }

        let response_text = response.text().await?;
        debug!("Raw BirdEye API response for wallet {}: {}", wallet_address, response_text);
        
        let general_txs_response: GeneralTraderTransactionsResponse = serde_json::from_str(&response_text)
            .map_err(|e| BirdEyeError::Api(format!("JSON parsing error: {}", e)))?;
        
        if !general_txs_response.success {
            return Err(BirdEyeError::Api("API returned success=false".to_string()));
        }

        debug!("Retrieved {} general transactions from BirdEye for wallet {}", 
               general_txs_response.data.items.len(), wallet_address);
        Ok(general_txs_response.data.items)
    }

    /// Get all trader transactions for a wallet with pagination (fetches all available transactions)
    pub async fn get_all_trader_transactions_paginated(
        &self,
        wallet_address: &str,
        from_time: Option<i64>,
        to_time: Option<i64>,
        max_total_transactions: u32,
    ) -> Result<Vec<GeneralTraderTransaction>, BirdEyeError> {
        let mut all_transactions = Vec::new();
        let mut offset = 0u32;
        let limit = 100u32; // Maximum allowed by BirdEye API
        let max_offset_limit = 10000u32; // BirdEye API constraint: offset + limit <= 10000
        
        debug!("Starting paginated fetch for wallet {} with max_total_transactions: {}", 
               wallet_address, max_total_transactions);
        
        loop {
            // Check BirdEye API constraint: offset + limit <= 10000
            if offset + limit > max_offset_limit {
                warn!("Reached BirdEye API constraint (offset + limit > 10000) for wallet {} at offset {}", 
                     wallet_address, offset);
                break;
            }
            
            // Check if we've fetched enough transactions
            if all_transactions.len() >= max_total_transactions as usize {
                debug!("Reached max_total_transactions limit ({}) for wallet {}", 
                       max_total_transactions, wallet_address);
                break;
            }
            
            // Calculate how many transactions we still need
            let remaining_needed = max_total_transactions.saturating_sub(all_transactions.len() as u32);
            let current_limit = std::cmp::min(limit, remaining_needed);
            
            // Ensure we don't violate the API constraint with the current limit
            let adjusted_limit = if offset + current_limit > max_offset_limit {
                max_offset_limit - offset
            } else {
                current_limit
            };
            
            if adjusted_limit == 0 {
                debug!("Cannot make request with limit 0, stopping pagination for wallet {}", wallet_address);
                break;
            }
            
            debug!("Fetching batch for wallet {} (offset: {}, limit: {})", 
                   wallet_address, offset, adjusted_limit);
            
            // Make the API call with offset and retry logic
            let (batch_transactions, has_next) = match self.get_all_trader_transactions_with_offset_retry(
                wallet_address,
                from_time,
                to_time,
                Some(adjusted_limit),
                Some(offset),
            ).await {
                Ok(result) => result,
                Err(e) => {
                    warn!("Failed to fetch batch for wallet {} at offset {}: {}. Continuing with next batch.", 
                          wallet_address, offset, e);
                    // Continue pagination despite this batch failure
                    (Vec::new(), true)
                }
            };
            
            let batch_size = batch_transactions.len();
            debug!("Retrieved {} transactions in batch for wallet {} (has_next: {})", 
                   batch_size, wallet_address, has_next);
            
            // If no transactions returned, we've reached the end
            if batch_size == 0 {
                debug!("No more transactions available for wallet {}", wallet_address);
                break;
            }
            
            // Add to our collection
            all_transactions.extend(batch_transactions);
            
            // Check has_next field from API response - this is the reliable indicator
            if !has_next {
                debug!("API indicates no more transactions available (has_next=false) for wallet {}", wallet_address);
                break;
            }
            
            // Move to next page by incrementing offset by the standard limit size (100)
            // This follows standard offset pagination: offset=0,100,200,300...
            // Always use the standard limit, not adjusted_limit, to maintain consistent pagination
            offset += limit;
            
            // Respect BirdEye rate limit: 10 req/sec = 100ms between requests (conservative)
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
        
        info!("Completed paginated fetch for wallet {}: {} total transactions", 
              wallet_address, all_transactions.len());
        
        Ok(all_transactions)
    }

    /// Private helper function that includes offset parameter
    /// Returns tuple of (transactions, has_next)
    async fn get_all_trader_transactions_with_offset(
        &self,
        wallet_address: &str,
        from_time: Option<i64>,
        to_time: Option<i64>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<(Vec<GeneralTraderTransaction>, bool), BirdEyeError> {
        let url = format!("{}/trader/txs/seek_by_time", self.config.api_base_url);
        
        let mut query_params = vec![
            ("address", wallet_address),
        ];
        
        let from_string;
        let to_string;
        let limit_string;
        let offset_string;
        
        if let Some(from) = from_time {
            from_string = from.to_string();
            query_params.push(("after_time", &from_string));
        }
        if let Some(to) = to_time {
            to_string = to.to_string();
            query_params.push(("before_time", &to_string));
        }
        if let Some(limit_val) = limit {
            limit_string = limit_val.to_string();
            query_params.push(("limit", &limit_string));
        }
        if let Some(offset_val) = offset {
            offset_string = offset_val.to_string();
            query_params.push(("offset", &offset_string));
        }

        let request = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .header("x-chain", "solana")
            .query(&query_params);
            
        info!("🔄 Making BirdEye API request to: {} with params: {:?}", url, query_params);
        
        let response = request.send().await?;

        info!("📡 BirdEye API response status: {} for wallet: {}", response.status(), wallet_address);

        if response.status() == 429 {
            error!("🚫 Rate limit hit (429) for wallet {}", wallet_address);
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_else(|_| "Failed to read error body".to_string());
            error!("❌ BirdEye API error for wallet {}: HTTP {} - Body: {}", wallet_address, status, error_body);
            return Err(BirdEyeError::Api(format!("HTTP {} - {}", status, error_body)));
        }

        let response_text = response.text().await?;
        
        info!("📄 BirdEye API response size: {} bytes for wallet: {}", response_text.len(), wallet_address);
        
        // Log first and last 100 chars to debug JSON corruption
        let preview_start = if response_text.len() > 100 {
            &response_text[..100]
        } else {
            &response_text
        };
        let preview_end = if response_text.len() > 200 {
            &response_text[response_text.len()-100..]
        } else {
            ""
        };
        
        info!("📄 Response preview - Start: {}...", preview_start.replace('\n', " "));
        if !preview_end.is_empty() {
            info!("📄 Response preview - End: ...{}", preview_end.replace('\n', " "));
        }
        
        let general_txs_response: GeneralTraderTransactionsResponse = serde_json::from_str(&response_text)
            .map_err(|e| {
                error!("💥 JSON parsing failed for wallet {}: {}", wallet_address, e);
                error!("💥 Response length: {} bytes", response_text.len());
                
                // Log the problematic section for debugging
                if let Some(column) = extract_column_from_error(&e.to_string()) {
                    let start = column.saturating_sub(100);
                    let end = std::cmp::min(column + 100, response_text.len());
                    if start < response_text.len() {
                        let snippet = &response_text[start..end];
                        error!("💥 Error context (column {}): ...{}...", column, snippet.replace('\n', " "));
                    }
                }
                
                BirdEyeError::Api(format!("JSON parsing error: {}", e))
            })?;
        
        if !general_txs_response.success {
            error!("❌ BirdEye API returned success=false for wallet {}", wallet_address);
            return Err(BirdEyeError::Api("API returned success=false".to_string()));
        }

        let has_next = general_txs_response.data.has_next.unwrap_or(false);
        let items_count = general_txs_response.data.items.len();
        
        info!("✅ Successfully parsed {} transactions, has_next={} for wallet {}", 
              items_count, has_next, wallet_address);
        
        Ok((general_txs_response.data.items, has_next))
    }

    /// Retry wrapper with exponential backoff for rate limit handling
    async fn get_all_trader_transactions_with_offset_retry(
        &self,
        wallet_address: &str,
        from_time: Option<i64>,
        to_time: Option<i64>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<(Vec<GeneralTraderTransaction>, bool), BirdEyeError> {
        let max_retries = 3;
        let mut attempt: u32 = 0;
        
        loop {
            attempt += 1;
            
            match self.get_all_trader_transactions_with_offset(
                wallet_address, from_time, to_time, limit, offset
            ).await {
                Ok(result) => return Ok(result),
                Err(BirdEyeError::RateLimit) if attempt <= max_retries => {
                    let delay_ms = 1000 * attempt.pow(2); // Exponential backoff: 1s, 4s, 9s
                    warn!("Rate limit hit for wallet {} (attempt {}), retrying in {}ms", 
                          wallet_address, attempt, delay_ms);
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms as u64)).await;
                    continue;
                }
                Err(BirdEyeError::Api(ref msg)) if msg.contains("JSON parsing error") && attempt <= max_retries => {
                    let delay_ms = 500 * attempt; // Linear backoff for JSON errors: 500ms, 1s, 1.5s
                    warn!("JSON parsing error for wallet {} (attempt {}): {}. Retrying in {}ms", 
                          wallet_address, attempt, msg, delay_ms);
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms as u64)).await;
                    continue;
                }
                Err(e) => {
                    if attempt > max_retries {
                        error!("Max retries exceeded for wallet {} at offset {:?}: {}", 
                               wallet_address, offset, e);
                    }
                    return Err(e);
                }
            }
        }
    }

    /// Get historical price for a token at a specific timestamp
    pub async fn get_historical_price(
        &self,
        token_address: &str,
        unix_timestamp: i64,
    ) -> Result<f64, BirdEyeError> {
        let url = format!("{}/defi/historical_price_unix", self.config.api_base_url);
        
        debug!("Fetching historical price from BirdEye for token: {} at timestamp: {}", 
               token_address, unix_timestamp);
        
        let response = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .query(&[
                ("address", token_address),
                ("timestamp", &unix_timestamp.to_string()),
            ])
            .send()
            .await?;

        if response.status() == 429 {
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            return Err(BirdEyeError::Api(format!("HTTP {}", response.status())));
        }

        let price_response: HistoricalPriceResponse = response.json().await?;
        
        if !price_response.success {
            return Err(BirdEyeError::Api("API returned success=false".to_string()));
        }

        debug!("Retrieved historical price from BirdEye for token {}: ${}", 
               token_address, price_response.data.value);
        Ok(price_response.data.value)
    }

    /// Get current price for a token
    pub async fn get_current_price(&self, token_address: &str) -> Result<f64, BirdEyeError> {
        let url = format!("{}/defi/price", self.config.api_base_url);
        
        debug!("Fetching current price from BirdEye for token: {}", token_address);
        
        let response = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .query(&[("address", token_address)])
            .send()
            .await?;

        if response.status() == 429 {
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            return Err(BirdEyeError::Api(format!("HTTP {}", response.status())));
        }

        let price_response: PriceResponse = response.json().await?;
        
        if !price_response.success {
            return Err(BirdEyeError::Api("API returned success=false".to_string()));
        }

        debug!("Retrieved current price from BirdEye for token {}: ${}", 
               token_address, price_response.data.value);
        Ok(price_response.data.value)
    }

    /// Get current prices for multiple tokens
    pub async fn get_current_prices(&self, token_addresses: &[String]) -> Result<HashMap<String, f64>, BirdEyeError> {
        let url = format!("{}/defi/multi_price", self.config.api_base_url);
        
        debug!("Fetching current prices from BirdEye for {} tokens", token_addresses.len());
        
        let address_list = token_addresses.join(",");
        
        let response = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .query(&[("list_address", &address_list)])
            .send()
            .await?;

        if response.status() == 429 {
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            return Err(BirdEyeError::Api(format!("HTTP {}", response.status())));
        }

        // BirdEye multi_price returns a different format
        let response_text = response.text().await?;
        let response_data: serde_json::Value = serde_json::from_str(&response_text)
            .map_err(|e| BirdEyeError::InvalidResponse(format!("JSON parse error: {}", e)))?;
        
        if !response_data.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
            return Err(BirdEyeError::Api("API returned success=false".to_string()));
        }

        let mut prices = HashMap::new();
        
        if let Some(data) = response_data.get("data") {
            for token_address in token_addresses {
                if let Some(price_value) = data.get(token_address)
                    .and_then(|v| v.get("value"))
                    .and_then(|v| v.as_f64()) 
                {
                    prices.insert(token_address.clone(), price_value);
                }
            }
        }

        debug!("Retrieved current prices from BirdEye for {}/{} tokens", 
               prices.len(), token_addresses.len());
        Ok(prices)
    }

    /// Filter trending tokens based on quality criteria
    pub fn filter_trending_tokens(
        &self,
        tokens: Vec<TrendingToken>,
        min_volume_usd: Option<f64>,
        min_price_change_24h: Option<f64>,
        min_liquidity: Option<f64>,
        min_market_cap: Option<f64>,
        max_rank: Option<u32>,
    ) -> Vec<TrendingToken> {
        tokens
            .into_iter()
            .filter(|token| {
                // Volume filter
                if let Some(min_vol) = min_volume_usd {
                    if token.volume_24h.unwrap_or(0.0) < min_vol {
                        return false;
                    }
                }
                
                // Price change filter
                if let Some(min_change) = min_price_change_24h {
                    if token.price_change_24h.unwrap_or(0.0) < min_change {
                        return false;
                    }
                }
                
                // Liquidity filter
                if let Some(min_liq) = min_liquidity {
                    if token.liquidity.unwrap_or(0.0) < min_liq {
                        return false;
                    }
                }
                
                // Market cap filter
                if let Some(min_cap) = min_market_cap {
                    if token.marketcap.unwrap_or(0.0) < min_cap {
                        return false;
                    }
                }
                
                // Rank filter (lower rank numbers are better)
                if let Some(max_r) = max_rank {
                    if token.rank.unwrap_or(u32::MAX) > max_r {
                        return false;
                    }
                }
                
                true
            })
            .collect()
    }

    /// Filter top traders based on quality criteria
    pub fn filter_top_traders(
        &self,
        traders: Vec<TopTrader>,
        min_volume_usd: f64,
        min_trades: u32,
        _min_win_rate: Option<f64>, // Not available in BirdEye response
        _max_last_trade_hours: Option<u32>, // Not available in BirdEye response
    ) -> Vec<TopTrader> {
        traders
            .into_iter()
            .filter(|trader| {
                // Volume filter
                if trader.volume < min_volume_usd {
                    return false;
                }
                
                // Trades filter
                if trader.trade < min_trades {
                    return false;
                }
                
                // Note: Win rate and last trade time filters are not available
                // in the BirdEye top traders API response structure
                
                true
            })
            .collect()
    }
}

/// Quality criteria for filtering trending tokens
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendingTokenFilter {
    /// Minimum 24h volume in USD
    pub min_volume_usd: Option<f64>,
    /// Minimum 24h price change percentage
    pub min_price_change_24h: Option<f64>,
    /// Minimum liquidity in USD
    pub min_liquidity: Option<f64>,
    /// Minimum market cap in USD
    pub min_market_cap: Option<f64>,
    /// Maximum rank (lower is better)
    pub max_rank: Option<u32>,
    /// Maximum number of tokens to return
    pub max_tokens: Option<usize>,
}

impl Default for TrendingTokenFilter {
    fn default() -> Self {
        Self {
            min_volume_usd: Some(10000.0),     // $10k minimum volume
            min_price_change_24h: Some(5.0),   // 5% minimum price change
            min_liquidity: Some(50000.0),      // $50k minimum liquidity
            min_market_cap: Some(100000.0),    // $100k minimum market cap
            max_rank: Some(1000),              // Top 1000 ranked tokens
            max_tokens: Some(50),              // Top 50 tokens max
        }
    }
}

/// Quality criteria for filtering top traders
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopTraderFilter {
    /// Minimum volume in USD
    pub min_volume_usd: f64,
    /// Minimum number of trades
    pub min_trades: u32,
    /// Minimum win rate percentage (0-100)
    pub min_win_rate: Option<f64>,
    /// Maximum hours since last trade
    pub max_last_trade_hours: Option<u32>,
    /// Maximum number of traders to return
    pub max_traders: Option<usize>,
}

impl Default for TopTraderFilter {
    fn default() -> Self {
        Self {
            min_volume_usd: 1000.0,    // $1k minimum volume
            min_trades: 5,             // At least 5 trades
            min_win_rate: Some(60.0),  // 60% win rate
            max_last_trade_hours: Some(48), // Last trade within 48 hours
            max_traders: Some(50),     // Top 50 traders max
        }
    }
}

/// Custom deserializer for amount fields that can be either string or number
fn deserialize_amount<'de, D>(deserializer: D) -> Result<u128, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{Error, Unexpected, Visitor};
    use std::fmt;

    struct AmountVisitor;

    impl<'de> Visitor<'de> for AmountVisitor {
        type Value = u128;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string or number representing an amount")
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(value as u128)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            if value >= 0 {
                Ok(value as u128)
            } else {
                Err(Error::invalid_value(Unexpected::Signed(value), &self))
            }
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            // Handle large floating point numbers more gracefully
            if value >= 0.0 && value.is_finite() {
                // For very large numbers, truncate the fractional part
                let truncated = value.floor();
                if truncated <= (u128::MAX as f64) {
                    Ok(truncated as u128)
                } else {
                    // If the number is too large for u128, use u128::MAX
                    debug!("Large amount {} truncated to u128::MAX", value);
                    Ok(u128::MAX)
                }
            } else {
                Err(Error::invalid_value(Unexpected::Float(value), &self))
            }
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: Error,
        {
            value.parse::<u128>().map_err(|_| {
                Error::invalid_value(Unexpected::Str(value), &self)
            })
        }
    }

    deserializer.deserialize_any(AmountVisitor)
}

/// Custom deserializer for signed amount fields that can be either string or number
fn deserialize_signed_amount<'de, D>(deserializer: D) -> Result<i128, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{Error, Unexpected, Visitor};
    use std::fmt;

    struct SignedAmountVisitor;

    impl<'de> Visitor<'de> for SignedAmountVisitor {
        type Value = i128;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string or number representing a signed amount")
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(value as i128)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(value as i128)
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            if value.is_finite() {
                let truncated = if value >= 0.0 { value.floor() } else { value.ceil() };
                if truncated >= (i128::MIN as f64) && truncated <= (i128::MAX as f64) {
                    Ok(truncated as i128)
                } else {
                    // If the number is too large for i128, use appropriate limit
                    if value > 0.0 {
                        debug!("Large positive amount {} truncated to i128::MAX", value);
                        Ok(i128::MAX)
                    } else {
                        debug!("Large negative amount {} truncated to i128::MIN", value);
                        Ok(i128::MIN)
                    }
                }
            } else {
                Err(Error::invalid_value(Unexpected::Float(value), &self))
            }
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: Error,
        {
            value.parse::<i128>().map_err(|_| {
                Error::invalid_value(Unexpected::Str(value), &self)
            })
        }
    }

    deserializer.deserialize_any(SignedAmountVisitor)
}

/// Default value for missing symbol field
fn default_symbol() -> String {
    "UNKNOWN".to_string()
}

/// Default value for missing tx_type field
fn default_tx_type() -> String {
    "unknown".to_string()
}

/// Custom deserializer for nullable f64 fields
fn deserialize_nullable_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{Error, Unexpected, Visitor};
    use std::fmt;

    struct NullableF64Visitor;

    impl<'de> Visitor<'de> for NullableF64Visitor {
        type Value = f64;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a number or null")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: Error,
        {
            // Return 0.0 for null values
            Ok(0.0)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: Error,
        {
            // Return 0.0 for null values
            Ok(0.0)
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(value)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(value as f64)
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(value as f64)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: Error,
        {
            value.parse::<f64>().map_err(|_| {
                Error::invalid_value(Unexpected::Str(value), &self)
            })
        }
    }

    deserializer.deserialize_any(NullableF64Visitor)
}

/// Deserialize a string that might be null
fn deserialize_nullable_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{Error, Visitor};
    use std::fmt;

    struct NullableStringVisitor;

    impl<'de> Visitor<'de> for NullableStringVisitor {
        type Value = String;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string or null")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: Error,
        {
            // Return empty string for null values
            Ok(String::new())
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: Error,
        {
            // Return empty string for null values
            Ok(String::new())
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(value.to_string())
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(value)
        }
    }

    deserializer.deserialize_any(NullableStringVisitor)
}

/// Deserialize an optional f64 that might be missing or null
fn deserialize_optional_nullable_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{Error, Visitor};
    use std::fmt;

    struct OptionalNullableF64Visitor;

    impl<'de> Visitor<'de> for OptionalNullableF64Visitor {
        type Value = f64;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a number, null, or missing field")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: Error,
        {
            // Return 0.0 for null values
            Ok(0.0)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: Error,
        {
            // Return 0.0 for null values
            Ok(0.0)
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(value)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(value as f64)
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(value as f64)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: Error,
        {
            match value.parse::<f64>() {
                Ok(parsed) => Ok(parsed),
                Err(_) => {
                    warn!("Could not parse '{}' as f64, using 0.0", value);
                    Ok(0.0)
                }
            }
        }
    }

    deserializer.deserialize_any(OptionalNullableF64Visitor)
}

/// Extract column number from JSON error message for better debugging
fn extract_column_from_error(error_msg: &str) -> Option<usize> {
    use regex::Regex;
    
    // Look for patterns like "at line 1 column 123" or "column 123"
    let re = Regex::new(r"column\s+(\d+)").unwrap();
    if let Some(captures) = re.captures(error_msg) {
        if let Some(column_match) = captures.get(1) {
            return column_match.as_str().parse::<usize>().ok();
        }
    }
    None
}

/// Implementation of PriceFetcher trait for BirdEye
#[async_trait]
impl PriceFetcher for BirdEyeClient {
    async fn fetch_prices(
        &self,
        token_mints: &[String],
        _vs_token: Option<&str>,
    ) -> PnLResult<HashMap<String, Decimal>> {
        match self.get_current_prices(token_mints).await {
            Ok(prices) => {
                let mut result = HashMap::new();
                for (mint, price) in prices {
                    result.insert(mint, Decimal::from_f64_retain(price).unwrap_or(Decimal::ZERO));
                }
                Ok(result)
            }
            Err(e) => {
                warn!("Failed to fetch prices from BirdEye: {}", e);
                Err(pnl_core::PnLError::PriceFetch(format!("BirdEye error: {}", e)))
            }
        }
    }

    async fn fetch_historical_price(
        &self,
        token_mint: &str,
        _timestamp: DateTime<Utc>,
        _vs_token: Option<&str>,
    ) -> PnLResult<Option<Decimal>> {
        // Historical prices should use embedded data from transactions
        // This method should not be called for embedded price systems
        debug!("Historical price requested for {} - should use embedded transaction prices instead", token_mint);
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_creation() {
        let config = BirdEyeConfig::default();
        assert_eq!(config.api_base_url, "https://public-api.birdeye.so");
        assert_eq!(config.request_timeout_seconds, 30);
    }

    #[test]
    fn test_filter_creation() {
        let filter = TopTraderFilter::default();
        assert_eq!(filter.min_volume_usd, 1000.0);
        assert_eq!(filter.min_trades, 5);
        assert_eq!(filter.min_win_rate, Some(60.0));
    }
}