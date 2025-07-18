use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, error, info};

#[derive(Error, Debug)]
pub enum DexScreenerError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),
    #[error("JSON parsing error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("API error: {status} - {message}")]
    ApiError { status: u16, message: String },
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    #[error("No data available")]
    NoDataAvailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexScreenerBoostedToken {
    #[serde(rename = "chainId")]
    pub chain_id: String,
    #[serde(rename = "tokenAddress")]
    pub token_address: String,
    // Only keeping essential fields for token identification
    pub description: Option<String>,
}

// Simplified response - just array of tokens
pub type DexScreenerBoostedResponse = Vec<DexScreenerBoostedToken>;

/// Configuration for DexScreener client
#[derive(Debug, Clone)]
pub struct DexScreenerConfig {
    pub api_base_url: String,
    pub request_timeout_seconds: u64,
    pub rate_limit_delay_ms: u64, // 60 requests per minute = 1000ms delay
    pub max_retries: u32,
    pub enabled: bool,
}

impl Default for DexScreenerConfig {
    fn default() -> Self {
        Self {
            api_base_url: "https://api.dexscreener.com".to_string(),
            request_timeout_seconds: 30,
            rate_limit_delay_ms: 1000, // 1 request per second for 60/min limit
            max_retries: 3,
            enabled: true,
        }
    }
}

/// DexScreener API client for fetching boosted tokens
pub struct DexScreenerClient {
    client: Client,
    config: DexScreenerConfig,
}

impl DexScreenerClient {
    /// Create a new DexScreener client
    pub fn new(config: DexScreenerConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.request_timeout_seconds))
            .build()?;

        Ok(Self { client, config })
    }

    /// Check if DexScreener client is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the latest boosted tokens
    pub async fn get_latest_boosted_tokens(&self) -> Result<Vec<DexScreenerBoostedToken>, DexScreenerError> {
        if !self.config.enabled {
            return Ok(vec![]);
        }

        let url = format!("{}/token-boosts/latest/v1", self.config.api_base_url);
        debug!("🔍 Fetching latest boosted tokens from: {}", url);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(DexScreenerError::ApiError { status, message });
        }

        // DexScreener API returns an array of boosted tokens
        let boosted_tokens: Vec<DexScreenerBoostedToken> = response.json().await?;
        
        // Filter for Solana tokens only
        let solana_tokens: Vec<DexScreenerBoostedToken> = boosted_tokens
            .into_iter()
            .filter(|token| token.chain_id.to_lowercase() == "solana")
            .collect();

        info!("📊 Retrieved {} latest boosted tokens from DexScreener (Solana only)", solana_tokens.len());
        
        if self.config.rate_limit_delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.config.rate_limit_delay_ms)).await;
        }

        Ok(solana_tokens)
    }

    /// Get the top boosted tokens (most active boosts)
    pub async fn get_top_boosted_tokens(&self) -> Result<Vec<DexScreenerBoostedToken>, DexScreenerError> {
        if !self.config.enabled {
            return Ok(vec![]);
        }

        let url = format!("{}/token-boosts/top/v1", self.config.api_base_url);
        debug!("🔍 Fetching top boosted tokens from: {}", url);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(DexScreenerError::ApiError { status, message });
        }

        // DexScreener API returns an array of boosted tokens
        let boosted_tokens: Vec<DexScreenerBoostedToken> = response.json().await?;
        
        // Filter for Solana tokens only
        let solana_tokens: Vec<DexScreenerBoostedToken> = boosted_tokens
            .into_iter()
            .filter(|token| token.chain_id.to_lowercase() == "solana")
            .collect();

        info!("📊 Retrieved {} top boosted tokens from DexScreener (Solana only)", solana_tokens.len());
        
        if self.config.rate_limit_delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.config.rate_limit_delay_ms)).await;
        }

        Ok(solana_tokens)
    }

    /// Get both latest and top boosted tokens in a single call
    pub async fn get_all_boosted_tokens(&self) -> Result<(Vec<DexScreenerBoostedToken>, Vec<DexScreenerBoostedToken>), DexScreenerError> {
        if !self.config.enabled {
            return Ok((vec![], vec![]));
        }

        debug!("🔍 Fetching all boosted tokens from DexScreener");

        let latest_tokens = self.get_latest_boosted_tokens().await?;
        let top_tokens = self.get_top_boosted_tokens().await?;

        debug!("✅ Retrieved {} latest + {} top boosted tokens", latest_tokens.len(), top_tokens.len());

        Ok((latest_tokens, top_tokens))
    }

    /// Extract unique token addresses from boosted tokens
    pub fn extract_token_addresses(&self, boosted_tokens: &[DexScreenerBoostedToken]) -> Vec<String> {
        let mut addresses: Vec<String> = boosted_tokens
            .iter()
            .map(|token| token.token_address.clone())
            .collect();

        // Remove duplicates and sort
        addresses.sort();
        addresses.dedup();

        debug!("📋 Extracted {} unique token addresses from boosted tokens", addresses.len());
        addresses
    }

    /// Get only the token addresses from boosted tokens (convenience method)
    pub fn get_token_addresses(&self, boosted_tokens: &[DexScreenerBoostedToken]) -> Vec<String> {
        boosted_tokens.iter().map(|token| token.token_address.clone()).collect()
    }

    /// Get configuration
    pub fn get_config(&self) -> &DexScreenerConfig {
        &self.config
    }
}

