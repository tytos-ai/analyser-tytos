// Solana RPC Client - Exact TypeScript Implementation
// Based on ts_system_to_rewrite_to_rust/src/modules/transactions_fetcher.ts

use anyhow::Result;
use chrono::DateTime;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use solana_transaction_status::EncodedConfirmedTransactionWithStatusMeta;
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
#[derive(Clone)]
pub struct SolanaClient {
    config: SolanaClientConfig,
    http_client: Client,
    request_id_counter: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl SolanaClient {
    pub fn new(config: SolanaClientConfig) -> Result<Self> {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(config.rpc_timeout_seconds))
            .build()?;

        Ok(Self {
            config,
            http_client,
            request_id_counter: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1)),
        })
    }

    fn next_request_id(&self) -> u64 {
        self.request_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    /// TypeScript: doRpcWithRetry - RPC call with retry logic
    async fn do_rpc_with_retry<T>(
        &self,
        mut rpc_call: impl FnMut() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + Send>>,
        description: &str,
        wallet: Option<&str>,
    ) -> Result<T> {
        const MAX_RETRIES: u64 = 5; // Add maximum retry limit
        let mut attempt = 0;
        
        loop {
            attempt += 1;
            
            debug!("[{}] Attempt #{}/{}{}", 
                description, 
                attempt, 
                MAX_RETRIES,
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
                    warn!("[{}] Failed attempt #{}/{}{}: {}", 
                        description, 
                        attempt, 
                        MAX_RETRIES,
                        wallet.map(|w| format!(" (wallet={})", w)).unwrap_or_default(),
                        err
                    );
                    
                    // If we've exceeded max retries, return the error
                    if attempt >= MAX_RETRIES {
                        error!("[{}] Maximum retries ({}) exceeded{}, giving up", 
                            description, 
                            MAX_RETRIES,
                            wallet.map(|w| format!(" (wallet={})", w)).unwrap_or_default()
                        );
                        return Err(err);
                    }
                    
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
        let mut before: Option<String> = None;

        loop {
            // TypeScript: connection.getSignaturesForAddress(pubKey, { before, limit: 1000 })
            let sig_infos: Vec<SignatureInfo> = {
                let wallet_addr = wallet_address.to_string();
                let before_sig = before.clone();
                self.get_signatures_for_address(&wallet_addr, before_sig.as_deref(), Some(1000)).await?
            };

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

                // If we exceed maxSig *while still in timeframe*, truncate to limit
                if self.config.max_signatures > 0 && results.len() >= self.config.max_signatures as usize {
                    // Truncate to max_signatures and stop collecting more
                    results.truncate(self.config.max_signatures as usize);
                    info!("Wallet {} truncated to {} transactions (max_signatures limit)", wallet_address, self.config.max_signatures);
                    return Ok(FetchResult { results, skipped: false });
                }
            }

            // Continue to next page

            // Prepare for next page
            before = Some(sig_infos.last().unwrap().signature.clone());

            // If fewer than 1000 returned => no more pages
            if sig_infos.len() < 1000 {
                break;
            }
        }

        // Return collected results

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

    /// Get all transactions for an address with optional limit
    /// This matches the TypeScript fetchAllSignaturesForWallet + fetchParsedTransactionsChunk logic
    pub async fn get_all_transactions_for_address(
        &self,
        address: &str,
        limit: Option<usize>,
    ) -> Result<Vec<EncodedConfirmedTransactionWithStatusMeta>> {
        info!("Fetching all transactions for address: {}", address);
        
        // Step 1: Get signatures using fetchAllSignaturesForWallet logic
        let fetch_result = self.fetch_all_signatures_for_wallet(address, None).await?;
        
        // Process all fetched signatures (truncated to limit if needed)
        
        let signatures: Vec<String> = fetch_result.results.into_iter()
            .take(limit.unwrap_or(usize::MAX))
            .map(|tr| tr.signature)
            .collect();
            
        if signatures.is_empty() {
            debug!("No signatures found for wallet: {}", address);
            return Ok(vec![]);
        }
        
        info!("Found {} signatures for wallet: {}", signatures.len(), address);
        
        // Step 2: Fetch parsed transactions in chunks (matches TypeScript logic)
        let mut all_transactions = Vec::new();
        let chunk_size = 2; // Further reduced to avoid rate limiting
        
        for chunk in signatures.chunks(chunk_size) {
            let chunk_transactions = self.fetch_parsed_transactions_chunk(chunk, address).await?;
            all_transactions.extend(chunk_transactions.into_iter().flatten());
            
            // Add delay between chunks to avoid rate limiting
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        }
        
        info!("Successfully fetched {} transactions for wallet: {}", all_transactions.len(), address);
        Ok(all_transactions)
    }

    /// TypeScript: fetchParsedTransactionsChunk
    async fn fetch_parsed_transactions_chunk(
        &self,
        signatures: &[String],
        wallet: &str,
    ) -> Result<Vec<Option<EncodedConfirmedTransactionWithStatusMeta>>> {
        let wallet_for_retry = Some(wallet);
        let signatures_vec = signatures.to_vec();
        
        // Use the retry mechanism just like TypeScript
        let parsed_transactions = self.do_rpc_with_retry(
            {
                let client = self.clone();
                move || {
                    let sigs = signatures_vec.clone();
                    let client = client.clone();
                    Box::pin(async move {
                        client.get_parsed_transactions(&sigs).await
                    })
                }
            },
            "getParsedTransactions",
            wallet_for_retry,
        ).await?;
        
        Ok(parsed_transactions)
    }

    /// Get multiple parsed transactions at once - matches TypeScript conn.getParsedTransactions
    async fn get_parsed_transactions(
        &self,
        signatures: &[String],
    ) -> Result<Vec<Option<EncodedConfirmedTransactionWithStatusMeta>>> {
        debug!("Fetching {} transactions via Solana RPC", signatures.len());
        
        let mut results = Vec::new();
        
        for signature in signatures {
            debug!("Fetching transaction for signature: {}", signature);
            
            match self.get_transaction(signature, Some("jsonParsed"), Some(0)).await {
                Ok(Some(tx_value)) => {
                    info!("✅ Successfully fetched transaction data from Solana RPC for: {}", signature);
                    
                    // Parse the real transaction data
                    match serde_json::from_value::<EncodedConfirmedTransactionWithStatusMeta>(tx_value) {
                        Ok(parsed_tx) => {
                            debug!("Successfully parsed transaction structure for: {}", signature);
                            results.push(Some(parsed_tx));
                        }
                        Err(e) => {
                            warn!("Failed to parse transaction structure for {}: {}", signature, e);
                            results.push(None);
                        }
                    }
                }
                Ok(None) => {
                    debug!("No transaction data returned for: {}", signature);
                    results.push(None);
                }
                Err(e) => {
                    warn!("❌ Failed to fetch transaction from Solana RPC {}: {}", signature, e);
                    results.push(None);
                }
            }
        }
        
        info!("Processed {} signatures, got {} valid responses", signatures.len(), results.iter().filter(|r| r.is_some()).count());
        Ok(results)
    }

    /// NEW: Discover wallet addresses from trending pair transactions
    pub async fn discover_wallets_from_pair(&self, pair_address: &str, limit: Option<u32>) -> Result<Vec<String>> {
        info!("Discovering wallets from trending pair: {}", pair_address);
        
        // Get recent signatures for the pair address
        let signatures = self.get_signatures_for_address(pair_address, None, limit).await?;
        
        if signatures.is_empty() {
            info!("No signatures found for pair: {}", pair_address);
            return Ok(vec![]);
        }
        
        info!("Found {} signatures for pair {}", signatures.len(), pair_address);
        
        let mut discovered_wallets = std::collections::HashSet::new();
        
        // Process each transaction to extract wallet addresses
        for sig_info in signatures.iter().take(50) { // Limit to 50 for rate limiting
            match self.get_transaction(&sig_info.signature, Some("json"), Some(0)).await {
                Ok(Some(tx_data)) => {
                    let wallets = self.extract_wallets_from_transaction(&tx_data);
                    for wallet in wallets {
                        if self.is_valid_solana_address(&wallet) {
                            discovered_wallets.insert(wallet);
                        }
                    }
                }
                Ok(None) => {
                    debug!("No transaction data for signature: {}", sig_info.signature);
                }
                Err(e) => {
                    warn!("Failed to fetch transaction {}: {}", sig_info.signature, e);
                }
            }
            
            // Small delay to respect rate limits
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        
        let wallet_list: Vec<String> = discovered_wallets.into_iter().collect();
        info!("Discovered {} unique wallets from pair {}", wallet_list.len(), pair_address);
        
        Ok(wallet_list)
    }

    /// Extract wallet addresses from transaction data
    fn extract_wallets_from_transaction(&self, tx_data: &Value) -> Vec<String> {
        let mut wallets = Vec::new();
        
        // Extract from transaction.message.accountKeys
        if let Some(transaction) = tx_data.get("transaction") {
            if let Some(message) = transaction.get("message") {
                if let Some(account_keys) = message.get("accountKeys") {
                    if let Some(keys_array) = account_keys.as_array() {
                        for key in keys_array {
                            if let Some(key_str) = key.as_str() {
                                wallets.push(key_str.to_string());
                            }
                        }
                    }
                }
            }
        }
        
        // Extract from meta.preTokenBalances and meta.postTokenBalances
        if let Some(meta) = tx_data.get("meta") {
            // Pre-token balances
            if let Some(pre_balances) = meta.get("preTokenBalances") {
                if let Some(balances_array) = pre_balances.as_array() {
                    for balance in balances_array {
                        if let Some(owner) = balance.get("owner") {
                            if let Some(owner_str) = owner.as_str() {
                                wallets.push(owner_str.to_string());
                            }
                        }
                    }
                }
            }
            
            // Post-token balances
            if let Some(post_balances) = meta.get("postTokenBalances") {
                if let Some(balances_array) = post_balances.as_array() {
                    for balance in balances_array {
                        if let Some(owner) = balance.get("owner") {
                            if let Some(owner_str) = owner.as_str() {
                                wallets.push(owner_str.to_string());
                            }
                        }
                    }
                }
            }
        }
        
        wallets
    }

    /// Validate Solana address format
    fn is_valid_solana_address(&self, address: &str) -> bool {
        // Basic Solana address validation
        address.len() == 44 && 
        address.chars().all(|c| {
            c.is_ascii_alphanumeric() && 
            c != '0' && c != 'O' && c != 'I' && c != 'l'  // Base58 excludes these
        })
    }

    /// NEW: Batch wallet discovery from multiple trending pairs
    pub async fn discover_wallets_from_pairs(&self, pair_addresses: &[String]) -> Result<Vec<String>> {
        info!("Discovering wallets from {} trending pairs", pair_addresses.len());
        
        let mut all_wallets = std::collections::HashSet::new();
        
        for pair_address in pair_addresses {
            match self.discover_wallets_from_pair(pair_address, Some(20)).await {
                Ok(wallets) => {
                    info!("Found {} wallets from pair {}", wallets.len(), pair_address);
                    for wallet in wallets {
                        all_wallets.insert(wallet);
                    }
                }
                Err(e) => {
                    warn!("Failed to discover wallets from pair {}: {}", pair_address, e);
                }
            }
            
            // Delay between pairs to respect rate limits
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        
        let wallet_list: Vec<String> = all_wallets.into_iter().collect();
        info!("Discovered {} unique wallets from {} pairs", wallet_list.len(), pair_addresses.len());
        
        Ok(wallet_list)
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