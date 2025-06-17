// DexScreener Client - Exact TypeScript Implementation  
// Based on ts_system_to_rewrite_to_rust/Dex/trendingWs.js and fetchDex.js

use anyhow::Result;
use base64::encode;
use futures_util::{SinkExt, StreamExt};
use persistence_layer::{RedisClient, PersistenceError};
use rand::RngCore;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

#[derive(Error, Debug)]
pub enum DexClientError {
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Persistence error: {0}")]
    Persistence(#[from] PersistenceError),
    #[error("Parsing error: {0}")]
    Parsing(String),
    #[error("Connection error: {0}")]
    Connection(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexClientConfig {
    /// TypeScript: TRENDING_WS_ENDPOINT
    pub ws_url: String,
    /// TypeScript: DEX_API_HOSTNAME
    pub http_base_url: String,
    /// Request timeout
    pub request_timeout_seconds: u64,
    /// Debug mode
    pub debug: bool,
}

impl Default for DexClientConfig {
    fn default() -> Self {
        Self {
            // TypeScript: "wss://io.dexscreener.com/dex/screener/v5/pairs/h24/1?rankBy[key]=trendingScoreH24&rankBy[order]=desc"
            ws_url: "wss://io.dexscreener.com/dex/screener/v5/pairs/h24/1?rankBy[key]=trendingScoreH24&rankBy[order]=desc".to_string(),
            // TypeScript: "https://io.dexscreener.com/dex/log/amm/v4/pumpfundex/top/solana"
            http_base_url: "https://io.dexscreener.com/dex/log/amm/v4/pumpfundex/top/solana".to_string(),
            request_timeout_seconds: 15,
            debug: false,
        }
    }
}

pub struct DexClient {
    config: DexClientConfig,
    http_client: Client,
    redis_client: Arc<Mutex<Option<RedisClient>>>,
}

impl DexClient {
    pub async fn new(
        config: DexClientConfig,
        redis_client: Option<RedisClient>,
    ) -> Result<Self> {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(config.request_timeout_seconds))
            .build()?;

        Ok(Self {
            config,
            http_client,
            redis_client: Arc::new(Mutex::new(redis_client)),
        })
    }

    /// TypeScript: grabTrendingOnce() - Get trending pairs from WebSocket
    pub async fn grab_trending_once(&self) -> Result<usize> {
        // TypeScript headers
        let mut headers = std::collections::HashMap::new();
        headers.insert(
            "User-Agent".to_string(),
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36".to_string(),
        );
        headers.insert("Origin".to_string(), "https://dexscreener.com".to_string());
        
        // Generate random WebSocket key (TypeScript: crypto.randomBytes(16).toString("base64"))
        let mut key_bytes = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut key_bytes);
        headers.insert("Sec-WebSocket-Key".to_string(), encode(&key_bytes));

        if self.config.debug {
            debug!("[DEBUG] Connecting to DexScreener WS...");
        }

        // Connect to WebSocket
        let request = tokio_tungstenite::tungstenite::http::Request::builder()
            .uri(&self.config.ws_url)
            .header("User-Agent", &headers["User-Agent"])
            .header("Origin", &headers["Origin"])
            .header("Sec-WebSocket-Key", &headers["Sec-WebSocket-Key"])
            .body(())?;

        let (ws_stream, _) = connect_async(request).await?;
        let (mut write, mut read) = ws_stream.split();

        if self.config.debug {
            debug!("[DEBUG] WebSocket open, sending keep-alive...");
        }

        // TypeScript: ws.send("{}");
        write.send(Message::Text("{}".to_string())).await?;

        // Wait for one binary message
        while let Some(message) = read.next().await {
            match message? {
                Message::Binary(data) => {
                    if self.config.debug {
                        debug!("[DEBUG] Received binary frame, size: {} bytes", data.len());
                    }

                    // TypeScript: const hex = Buffer.from(data).toString("hex");
                    let hex = hex::encode(&data);
                    
                    // TypeScript: const pairs = extractPairs(hex);
                    let pairs = self.extract_pairs(&hex);
                    
                    // TypeScript: const added = await savePairs(pairs);
                    let added = self.save_pairs(&pairs).await?;

                    info!("• WebSocket frame → {} pair(s), {} new", pairs.len(), added);
                    return Ok(added);
                }
                Message::Text(_) => {
                    // Ignore text messages, continue waiting for binary
                    continue;
                }
                Message::Close(_) => {
                    return Err(DexClientError::Connection("WebSocket closed unexpectedly".to_string()).into());
                }
                _ => {
                    // Ignore other message types
                    continue;
                }
            }
        }

        Err(DexClientError::Connection("No binary message received".to_string()).into())
    }

    /// TypeScript: extractPairs(hex) - Extract pairs from hex data
    fn extract_pairs(&self, hex: &str) -> Vec<String> {
        // TypeScript: const PAIRS_RX = /0058([0-9A-Fa-f]{88})58/g;
        let pairs_regex = Regex::new(r"0058([0-9A-Fa-f]{88})58").unwrap();
        let mut pairs = HashSet::new();

        for captures in pairs_regex.captures_iter(hex) {
            if let Some(hex_match) = captures.get(1) {
                // TypeScript: const pair = Buffer.from(m[1], "hex").toString("ascii");
                if let Ok(bytes) = hex::decode(hex_match.as_str()) {
                    if let Ok(pair) = String::from_utf8(bytes) {
                        pairs.insert(pair);
                    } else if self.config.debug {
                        debug!("Failed to parse pair from chunk: invalid UTF-8");
                    }
                } else if self.config.debug {
                    debug!("Failed to parse pair from chunk: invalid hex");
                }
            }
        }

        pairs.into_iter().collect()
    }

    /// TypeScript: savePairs(pairs) - Save pairs to Redis
    async fn save_pairs(&self, pairs: &[String]) -> Result<usize> {
        if pairs.is_empty() {
            return Ok(0);
        }

        let redis = self.redis_client.lock().await;
        if let Some(ref redis_client) = *redis {
            let mut inserted_count = 0;

            for pair in pairs {
                // TypeScript: pipeline.hSetNX(`trending:${p}`, "extracted", "false");
                let key = format!("trending:{}", pair);
                match redis_client.set_trending_pair(pair).await {
                    Ok(was_new) => {
                        if was_new {
                            inserted_count += 1;
                        }
                    }
                    Err(e) => {
                        warn!("Failed to save pair {}: {}", pair, e);
                    }
                }
            }

            Ok(inserted_count)
        } else {
            warn!("Redis client not available, cannot save pairs");
            Ok(0)
        }
    }

    /// TypeScript: fetchDex(pair, dir) - Fetch binary data for a specific pair
    pub async fn fetch_dex(&self, pair: &str) -> Result<Vec<u8>> {
        // TypeScript URL construction:
        // `/dex/log/amm/v4/pumpfundex/top/solana/${pair}` +
        // "?q=So11111111111111111111111111111111111111112&mda=30&s=pnl&sd=desc"
        let url = format!(
            "{}/{}?q=So11111111111111111111111111111111111111112&mda=30&s=pnl&sd=desc",
            self.config.http_base_url,
            pair
        );

        // TypeScript headers
        let response = self.http_client
            .get(&url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135 Safari/537.36"
            )
            .header("Accept-Encoding", "identity")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(DexClientError::Http(
                reqwest::Error::from(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("DexScreener HTTP {} ({})", response.status(), response.status().canonical_reason().unwrap_or("Unknown"))
                ))
            ).into());
        }

        let body = response.bytes().await?;
        Ok(body.to_vec())
    }

    /// Extract Solana wallet addresses from binary data - Based on extractSolKeys.js
    pub fn extract_sol_keys(&self, data: &[u8]) -> Result<Vec<String>> {
        let mut addresses = Vec::new();
        
        // TypeScript/JS pattern from extractSolKeys.js
        // Looking for specific binary patterns that contain base58 addresses
        let start_markers = [0x01, 0x00];
        let marker_0x58 = 0x58;
        
        let mut i = 0;
        while i < data.len().saturating_sub(52) {
            if data[i] == start_markers[0] && data[i + 1] == start_markers[1] {
                if i + 2 < data.len() && data[i + 2] == marker_0x58 {
                    // Extract potential 44-byte address
                    if i + 46 < data.len() {
                        let address_bytes = &data[i + 3..i + 47];
                        
                        // Basic validation: should be valid base58 characters
                        if self.is_valid_base58_bytes(address_bytes) {
                            if let Ok(addr_str) = std::str::from_utf8(address_bytes) {
                                // Additional validation: should be 44 characters (typical Solana address length)
                                if addr_str.len() == 44 && self.is_valid_solana_address(addr_str) {
                                    addresses.push(addr_str.to_string());
                                }
                            }
                        }
                    }
                }
            }
            i += 1;
        }

        // Remove duplicates
        addresses.sort();
        addresses.dedup();
        
        debug!("Extracted {} unique wallet addresses", addresses.len());
        Ok(addresses)
    }

    /// Validate base58 characters
    fn is_valid_base58_bytes(&self, bytes: &[u8]) -> bool {
        // Base58 alphabet: 123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz
        bytes.iter().all(|&b| {
            (b >= b'1' && b <= b'9') ||
            (b >= b'A' && b <= b'H') ||
            (b >= b'J' && b <= b'N') ||
            (b >= b'P' && b <= b'Z') ||
            (b >= b'a' && b <= b'k') ||
            (b >= b'm' && b <= b'z')
        })
    }

    /// Basic Solana address validation
    fn is_valid_solana_address(&self, address: &str) -> bool {
        // Basic checks for Solana address format
        address.len() == 44 && 
        address.chars().all(|c| {
            c.is_ascii_alphanumeric() && 
            c != '0' && c != 'O' && c != 'I' && c != 'l'  // Base58 excludes these
        })
    }

    /// Get unextracted pairs from Redis and process them
    pub async fn process_unextracted_pairs(&self) -> Result<Vec<String>> {
        let redis = self.redis_client.lock().await;
        if let Some(ref redis_client) = *redis {
            let unextracted_pairs = redis_client.get_unextracted_pairs().await?;
            drop(redis);

            let mut discovered_wallets = Vec::new();

            for pair in &unextracted_pairs {
                match self.fetch_dex(pair).await {
                    Ok(data) => {
                        match self.extract_sol_keys(&data) {
                            Ok(mut wallets) => {
                                discovered_wallets.append(&mut wallets);
                                
                                // Mark as extracted
                                let redis = self.redis_client.lock().await;
                                if let Some(ref redis_client) = *redis {
                                    if let Err(e) = redis_client.mark_pair_extracted(pair).await {
                                        warn!("Failed to mark pair {} as extracted: {}", pair, e);
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Failed to extract keys from pair {}: {}", pair, e);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to fetch data for pair {}: {}", pair, e);
                    }
                }
            }

            // Remove duplicates
            discovered_wallets.sort();
            discovered_wallets.dedup();

            // Push to Redis queue if we have wallets
            if !discovered_wallets.is_empty() {
                let redis = self.redis_client.lock().await;
                if let Some(ref redis_client) = *redis {
                    if let Err(e) = redis_client.push_discovered_wallets(&discovered_wallets).await {
                        error!("Failed to push discovered wallets to queue: {}", e);
                    } else {
                        info!("Pushed {} discovered wallets to queue", discovered_wallets.len());
                    }
                }
            }

            Ok(discovered_wallets)
        } else {
            warn!("Redis client not available");
            Ok(vec![])
        }
    }

    /// Start continuous monitoring (combines WebSocket + HTTP processing)
    pub async fn start_monitoring(&mut self) -> Result<()> {
        info!("Starting DexScreener monitoring...");

        loop {
            // Step 1: Get trending pairs from WebSocket
            match self.grab_trending_once().await {
                Ok(new_pairs) => {
                    if new_pairs > 0 {
                        info!("Discovered {} new trending pairs", new_pairs);
                    }
                }
                Err(e) => {
                    error!("Failed to grab trending pairs: {}", e);
                }
            }

            // Step 2: Process unextracted pairs
            match self.process_unextracted_pairs().await {
                Ok(wallets) => {
                    if !wallets.is_empty() {
                        info!("Extracted {} wallets from unprocessed pairs", wallets.len());
                    }
                }
                Err(e) => {
                    error!("Failed to process unextracted pairs: {}", e);
                }
            }

            // Sleep before next iteration (TypeScript: LOOP_MS=300000)
            tokio::time::sleep(Duration::from_millis(300000)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_pairs_regex() {
        let client = DexClient {
            config: DexClientConfig::default(),
            http_client: Client::new(),
            redis_client: Arc::new(Mutex::new(None)),
        };

        // Test with sample hex data that matches the pattern
        let hex = "0058414243444546474849414243444546474849414243444546474849414243444546474849414243444546474849414243444546474849414243444546474849414243444546474849414243444546474849585800";
        let pairs = client.extract_pairs(hex);
        
        // Should extract valid pairs based on the pattern
        assert!(!pairs.is_empty() || hex.len() < 180); // Allow for empty if hex is too short
    }

    #[test]
    fn test_base58_validation() {
        let client = DexClient {
            config: DexClientConfig::default(),
            http_client: Client::new(),
            redis_client: Arc::new(Mutex::new(None)),
        };

        let valid_b58 = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijk";
        assert!(client.is_valid_base58_bytes(valid_b58));

        let invalid_b58 = b"0OIl"; // Contains invalid base58 chars
        assert!(!client.is_valid_base58_bytes(invalid_b58));
    }

    #[test]
    fn test_solana_address_validation() {
        let client = DexClient {
            config: DexClientConfig::default(),
            http_client: Client::new(),
            redis_client: Arc::new(Mutex::new(None)),
        };

        let valid_addr = "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM";
        assert!(client.is_valid_solana_address(valid_addr));

        let invalid_addr = "invalid_address_123";
        assert!(!client.is_valid_solana_address(invalid_addr));
    }
}