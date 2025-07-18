use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, error, info, warn};

#[derive(Error, Debug)]
pub enum TokenMetadataError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),
    #[error("JSON parsing failed: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("API error: {0}")]
    ApiError(String),
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    #[error("Invalid token address: {0}")]
    InvalidTokenAddress(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetadata {
    pub address: String,
    pub symbol: String,
    pub name: String,
    pub decimals: u8,
    pub logo_uri: Option<String>,
    pub extensions: Option<TokenExtensions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenExtensions {
    pub coingecko_id: Option<String>,
    pub website: Option<String>,
    pub twitter: Option<String>,
    pub discord: Option<String>,
    pub medium: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BirdEyeTokenResponse {
    pub data: TokenMetadata,
    pub success: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct BirdEyeMultipleTokenResponse {
    pub data: HashMap<String, TokenMetadata>,
    pub success: bool,
}

/// Service for fetching token metadata from BirdEye API
#[derive(Debug)]
pub struct TokenMetadataService {
    client: Client,
    api_key: String,
    base_url: String,
    // In-memory cache for token metadata
    cache: std::sync::Arc<std::sync::RwLock<HashMap<String, TokenMetadata>>>,
}

impl TokenMetadataService {
    pub fn new(api_key: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            api_key,
            base_url: "https://public-api.birdeye.so".to_string(),
            cache: std::sync::Arc::new(std::sync::RwLock::new(HashMap::new())),
        }
    }

    /// Fetch metadata for a single token
    pub async fn get_metadata_single(&self, token_address: &str) -> Result<TokenMetadata, TokenMetadataError> {
        // Check cache first
        if let Ok(cache) = self.cache.read() {
            if let Some(metadata) = cache.get(token_address) {
                debug!("Token metadata cache hit for: {}", token_address);
                return Ok(metadata.clone());
            }
        }

        debug!("Fetching single token metadata for: {}", token_address);

        let url = format!("{}/defi/v3/token/meta-data/single", self.base_url);
        let response = self.client
            .get(&url)
            .header("X-API-KEY", &self.api_key)
            .header("x-chain", "solana")
            .query(&[("address", token_address)])
            .send()
            .await?;

        if response.status().as_u16() == 429 {
            return Err(TokenMetadataError::RateLimitExceeded);
        }

        let response_text = response.text().await?;
        let api_response: BirdEyeTokenResponse = serde_json::from_str(&response_text)
            .map_err(|e| {
                error!("Failed to parse BirdEye token response: {}", response_text);
                TokenMetadataError::JsonError(e)
            })?;

        if !api_response.success {
            return Err(TokenMetadataError::ApiError(format!(
                "BirdEye API returned success=false for token: {}",
                token_address
            )));
        }

        // Cache the result
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(token_address.to_string(), api_response.data.clone());
        }

        Ok(api_response.data)
    }

    /// Fetch metadata for multiple tokens (up to 50 at once)
    pub async fn get_metadata_batch(&self, token_addresses: &[String]) -> Result<HashMap<String, TokenMetadata>, TokenMetadataError> {
        if token_addresses.is_empty() {
            return Ok(HashMap::new());
        }

        if token_addresses.len() > 50 {
            return Err(TokenMetadataError::ApiError(
                "BirdEye API supports maximum 50 addresses per batch request".to_string()
            ));
        }

        // Check cache for all tokens first
        let mut cached_results = HashMap::new();
        let mut uncached_addresses = Vec::new();

        if let Ok(cache) = self.cache.read() {
            for address in token_addresses {
                if let Some(metadata) = cache.get(address) {
                    cached_results.insert(address.clone(), metadata.clone());
                } else {
                    uncached_addresses.push(address.clone());
                }
            }
        }

        debug!("Token metadata: {} cached, {} need fetching", cached_results.len(), uncached_addresses.len());

        if uncached_addresses.is_empty() {
            return Ok(cached_results);
        }

        // Fetch uncached tokens
        let address_list = uncached_addresses.join(",");
        debug!("Fetching batch token metadata for {} tokens", uncached_addresses.len());

        let url = format!("{}/defi/v3/token/meta-data/multiple", self.base_url);
        let response = self.client
            .get(&url)
            .header("X-API-KEY", &self.api_key)
            .header("x-chain", "solana")
            .query(&[("list_address", &address_list)])
            .send()
            .await?;

        if response.status().as_u16() == 429 {
            return Err(TokenMetadataError::RateLimitExceeded);
        }

        let response_text = response.text().await?;
        let api_response: BirdEyeMultipleTokenResponse = serde_json::from_str(&response_text)
            .map_err(|e| {
                error!("Failed to parse BirdEye multiple token response: {}", response_text);
                TokenMetadataError::JsonError(e)
            })?;

        if !api_response.success {
            return Err(TokenMetadataError::ApiError(
                "BirdEye API returned success=false for batch token request".to_string()
            ));
        }

        // Cache the new results
        if let Ok(mut cache) = self.cache.write() {
            for (address, metadata) in &api_response.data {
                cache.insert(address.clone(), metadata.clone());
            }
        }

        // Merge cached and fetched results
        let mut all_results = cached_results;
        all_results.extend(api_response.data);

        info!("Successfully fetched metadata for {} tokens", all_results.len());

        Ok(all_results)
    }

    /// Get token metadata with fallback to mint address
    pub async fn get_metadata_with_fallback(&self, token_address: &str) -> TokenMetadata {
        match self.get_metadata_single(token_address).await {
            Ok(metadata) => metadata,
            Err(e) => {
                warn!("Failed to fetch metadata for token {}: {}, using fallback", token_address, e);
                // Create fallback metadata using mint address
                TokenMetadata {
                    address: token_address.to_string(),
                    symbol: Self::extract_symbol_from_address(token_address),
                    name: format!("Unknown Token ({})", Self::shorten_address(token_address)),
                    decimals: 9, // Default to 9 decimals for most Solana tokens
                    logo_uri: None,
                    extensions: None,
                }
            }
        }
    }

    /// Batch fetch with fallback for failed tokens
    pub async fn get_metadata_batch_with_fallback(&self, token_addresses: &[String]) -> HashMap<String, TokenMetadata> {
        let mut all_results = HashMap::new();

        // Try batch fetch first
        match self.get_metadata_batch(token_addresses).await {
            Ok(metadata_map) => {
                all_results.extend(metadata_map);
            }
            Err(e) => {
                warn!("Batch token metadata fetch failed: {}, falling back to individual requests", e);
                // Fallback to individual requests
                for address in token_addresses {
                    let metadata = self.get_metadata_with_fallback(address).await;
                    all_results.insert(address.clone(), metadata);
                }
                return all_results;
            }
        }

        // Add fallback metadata for any missing tokens
        for address in token_addresses {
            if !all_results.contains_key(address) {
                let fallback_metadata = TokenMetadata {
                    address: address.clone(),
                    symbol: Self::extract_symbol_from_address(address),
                    name: format!("Unknown Token ({})", Self::shorten_address(address)),
                    decimals: 9,
                    logo_uri: None,
                    extensions: None,
                };
                all_results.insert(address.clone(), fallback_metadata);
            }
        }

        all_results
    }

    /// Extract a short symbol from token address (for fallback)
    fn extract_symbol_from_address(address: &str) -> String {
        if address.len() >= 8 {
            format!("{}..{}", &address[..4], &address[address.len()-4..])
        } else {
            address.to_string()
        }
    }

    /// Shorten address for display
    fn shorten_address(address: &str) -> String {
        if address.len() >= 12 {
            format!("{}..{}", &address[..6], &address[address.len()-6..])
        } else {
            address.to_string()
        }
    }

    /// Clear the in-memory cache
    pub fn clear_cache(&self) {
        if let Ok(mut cache) = self.cache.write() {
            cache.clear();
            debug!("Token metadata cache cleared");
        }
    }

    /// Get cache size
    pub fn get_cache_size(&self) -> usize {
        self.cache.read().map(|cache| cache.len()).unwrap_or(0)
    }
}

impl Clone for TokenMetadataService {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            api_key: self.api_key.clone(),
            base_url: self.base_url.clone(),
            cache: self.cache.clone(),
        }
    }
}

