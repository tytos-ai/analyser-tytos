// Solana RPC Client - Exact TypeScript Implementation
// Based on ts_system_to_rewrite_to_rust/src/modules/transactions_fetcher.ts

use anyhow::Result;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, error, info, warn};

#[derive(Error, Debug)]
pub enum SolanaClientError {
    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("RPC error: {0}")]
    Rpc(String),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    #[error("Timeout")]
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaClientConfig {
    /// TypeScript: process.env.SOLANA_RPC_URL || "https://api.mainnet-beta.solana.com"
    pub rpc_url: String,
    /// Request timeout in seconds
    pub rpc_timeout_seconds: u64,
    /// Max concurrent requests
    pub max_concurrent_requests: usize,
    /// Max signatures to fetch per wallet (TypeScript: MAX_SIGNATURE)
    pub max_signatures: u64,
}

impl Default for SolanaClientConfig {
    fn default() -> Self {
        Self {
            rpc_url: "https://api.mainnet-beta.solana.com".to_string(),
            rpc_timeout_seconds: 30,
            max_concurrent_requests: 5,
            max_signatures: 50000,
        }
    }
}

/// TypeScript: interface TransactionRecord
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRecord {
    pub signature: String,
    pub block_time: Option<i64>,
}

/// TypeScript: interface FetchResult
#[derive(Debug, Clone)]
pub struct FetchResult {
    pub results: Vec<TransactionRecord>,
    pub skipped: bool, // If we exceeded max signatures before hitting the cutoff
}

/// Solana RPC response structures
#[derive(Debug, Deserialize)]
pub struct RpcResponse<T> {
    pub jsonrpc: String,
    pub id: u64,
    pub result: Option<T>,
    pub error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

/// TypeScript: ConfirmedSignatureInfo equivalent
#[derive(Debug, Deserialize)]
pub struct SignatureInfo {
    pub signature: String,
    #[serde(rename = "blockTime")]
    pub block_time: Option<i64>,
    pub err: Option<Value>,
}

/// Main Solana RPC Client
pub struct SolanaClient {
    config: SolanaClientConfig,
    http_client: Client,
    request_id_counter: std::sync::atomic::AtomicU64,
}

impl SolanaClient {
    pub fn new(config: SolanaClientConfig) -> Result<Self> {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(config.rpc_timeout_seconds))
            .build()?;

        Ok(Self {
            config,
            http_client,
            request_id_counter: std::sync::atomic::AtomicU64::new(1),
        })
    }

    fn next_request_id(&self) -> u64 {
        self.request_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    /// TypeScript: doRpcWithRetry - RPC call with retry logic
    async fn do_rpc_with_retry<T>(
        &self,
        rpc_call: impl Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + Send>>,
        description: &str,
        wallet: Option<&str>,
    ) -> Result<T> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            
            debug!("[{}] Attempt #{}{}", 
                description, 
                attempt, 
                wallet.map(|w| format!(" (wallet={})", w)).unwrap_or_default()
            );

            match rpc_call().await {
                Ok(result) => {
                    debug!("[{}] Success on attempt #{}{}", 
                        description, 
                        attempt, 
                        wallet.map(|w| format!(" (wallet={})", w)).unwrap_or_default()
                    );
                    return Ok(result);
                }
                Err(err) => {
                    warn!("[{}] Failed attempt #{}{}: {}", 
                        description, 
                        attempt, 
                        wallet.map(|w| format!(" (wallet={})", w)).unwrap_or_default(),
                        err
                    );
                    
                    // TypeScript: await new Promise((r) => setTimeout(r, 1000 * attempt));
                    tokio::time::sleep(Duration::from_millis(1000 * attempt)).await;
                }
            }
        }
    }

    /// TypeScript: fetchAllSignaturesForWallet
    pub async fn fetch_all_signatures_for_wallet(
        &self,
        wallet_address: &str,
        cutoff_unix_time: Option<i64>,
    ) -> Result<FetchResult> {
        let mut results = Vec::new();
        let mut skipped = false;
        let mut before: Option<String> = None;

        loop {
            // TypeScript: connection.getSignaturesForAddress(pubKey, { before, limit: 1000 })
            let sig_infos: Vec<SignatureInfo> = self.do_rpc_with_retry(
                || {
                    let wallet_address = wallet_address.to_string();
                    let before = before.clone();
                    Box::pin(async move {
                        self.get_signatures_for_address(&wallet_address, before.as_deref(), Some(1000)).await
                    })
                },
                "getSignaturesForAddress",
                Some(wallet_address),
            ).await?;

            // No more txs => stop
            if sig_infos.is_empty() {
                break;
            }

            // Process each signature, respecting cutoff + maxSig
            for info in &sig_infos {
                let signature = &info.signature;
                let block_time = info.block_time;

                // If we have a cutoff, and this blockTime is older => stop
                if let (Some(bt), Some(cutoff)) = (block_time, cutoff_unix_time) {
                    if bt < cutoff {
                        // We can return right away, since everything beyond is older
                        return Ok(FetchResult { results, skipped: false });
                    }
                }

                // Add the signature
                results.push(TransactionRecord {
                    signature: signature.clone(),
                    block_time,
                });

                // If we exceed maxSig *while still in timeframe*, skip entire wallet
                if self.config.max_signatures > 0 && results.len() > self.config.max_signatures as usize {
                    skipped = true;
                    break;
                }
            }

            // If we flagged "skipped," break out of the main while-loop
            if skipped {
                break;
            }

            // Prepare for next page
            before = Some(sig_infos.last().unwrap().signature.clone());

            // If fewer than 1000 returned => no more pages
            if sig_infos.len() < 1000 {
                break;
            }
        }

        // If we decided to skip => discard everything
        if skipped {
            return Ok(FetchResult {
                results: vec![],
                skipped: true,
            });
        }

        Ok(FetchResult { results, skipped: false })
    }

    /// Get signatures for address - matches Solana RPC getSignaturesForAddress
    async fn get_signatures_for_address(
        &self,
        address: &str,
        before: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<SignatureInfo>> {
        let mut params = vec![json!(address)];
        
        let mut options = serde_json::Map::new();
        if let Some(before_sig) = before {
            options.insert("before".to_string(), json!(before_sig));
        }
        if let Some(limit_val) = limit {
            options.insert("limit".to_string(), json!(limit_val));
        }
        
        if !options.is_empty() {
            params.push(json!(options));
        }

        let response = self.rpc_request("getSignaturesForAddress", json!(params)).await?;
        
        match response.result {
            Some(result) => {
                let sig_infos: Vec<SignatureInfo> = serde_json::from_value(result)?;
                Ok(sig_infos)
            }
            None => {
                if let Some(error) = response.error {
                    return Err(SolanaClientError::Rpc(error.message).into());
                }
                Ok(vec![])
            }
        }
    }

    /// Get parsed transaction - matches Solana RPC getTransaction
    pub async fn get_transaction(
        &self,
        signature: &str,
        encoding: Option<&str>,
        max_supported_transaction_version: Option<u32>,
    ) -> Result<Option<Value>> {
        let mut options = serde_json::Map::new();
        options.insert("encoding".to_string(), json!(encoding.unwrap_or("json")));
        if let Some(version) = max_supported_transaction_version {
            options.insert("maxSupportedTransactionVersion".to_string(), json!(version));
        }

        let params = json!([signature, options]);
        let response = self.rpc_request("getTransaction", params).await?;

        match response.result {
            Some(result) => {
                if result.is_null() {
                    Ok(None)
                } else {
                    Ok(Some(result))
                }
            }
            None => {
                if let Some(error) = response.error {
                    return Err(SolanaClientError::Rpc(error.message).into());
                }
                Ok(None)
            }
        }
    }

    /// Generic RPC request
    async fn rpc_request(&self, method: &str, params: Value) -> Result<RpcResponse<Value>> {
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": self.next_request_id(),
            "method": method,
            "params": params
        });

        let response = self.http_client
            .post(&self.config.rpc_url)
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(SolanaClientError::InvalidResponse(
                format!("HTTP {}", response.status())
            ).into());
        }

        let rpc_response: RpcResponse<Value> = response.json().await?;
        Ok(rpc_response)
    }
}

/// Timeframe parsing - matches TypeScript exactly
pub fn parse_timeframe_cutoff(
    timeframe_mode: &str,
    timeframe_general: &str,
    timeframe_specific: &str,
) -> Option<i64> {
    match timeframe_mode {
        "none" => None,
        "specific" => {
            // TypeScript: const date = new Date(dateStr);
            match DateTime::parse_from_rfc3339(timeframe_specific) {
                Ok(dt) => Some(dt.timestamp()),
                Err(_) => {
                    warn!("Invalid specific timeframe: {}", timeframe_specific);
                    None
                }
            }
        }
        "general" => {
            // TypeScript regex: /^(\d+)(s|min|h|d|m|y)$/
            if let Some(captures) = regex::Regex::new(r"^(\d+)(s|min|h|d|m|y)$")
                .unwrap()
                .captures(timeframe_general)
            {
                let amount: u64 = captures[1].parse().ok()?;
                let unit = &captures[2];

                let offset_ms = match unit {
                    "s" => amount * 1000,
                    "min" => amount * 60_000,
                    "h" => amount * 3_600_000,
                    "d" => amount * 86_400_000,
                    "m" => amount * 2_592_000_000, // ~30 days (matches TypeScript)
                    "y" => amount * 31_536_000_000, // ~365 days (matches TypeScript)
                    _ => return None,
                };

                let now = chrono::Utc::now().timestamp_millis() as u64;
                Some(((now - offset_ms) / 1000) as i64)
            } else {
                warn!("Invalid general timeframe: {}", timeframe_general);
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeframe_parsing() {
        // Test cases from TypeScript
        assert_eq!(
            parse_timeframe_cutoff("general", "1h", ""),
            Some((chrono::Utc::now().timestamp_millis() / 1000 - 3600) as i64)
        );
    }

    #[tokio::test]
    async fn test_solana_client_creation() {
        let config = SolanaClientConfig::default();
        let client = SolanaClient::new(config).unwrap();
        assert_eq!(client.config.rpc_url, "https://api.mainnet-beta.solana.com");
    }
}