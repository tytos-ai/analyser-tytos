use crate::{
    error::GoldRushError,
    types::{
        BalancesResponse, GoldRushChain, GoldRushConfig, GoldRushResponse, GoldRushTransaction, 
        TokenBalance, TokenTransfer, TransactionRequest, TransactionsResponse, TransfersResponse,
    },
};
use reqwest::Client;
use std::time::Duration;
use tracing::{error, info};

/// GoldRush API client for EVM chain transaction data
#[derive(Debug, Clone)]
pub struct GoldRushClient {
    client: Client,
    config: GoldRushConfig,
}

impl GoldRushClient {
    /// Create a new GoldRush client with default configuration
    pub fn new() -> Result<Self, GoldRushError> {
        Self::with_config(GoldRushConfig::default())
    }

    /// Create a new GoldRush client with custom configuration
    pub fn with_config(config: GoldRushConfig) -> Result<Self, GoldRushError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .connect_timeout(Duration::from_secs(30)) // Connection timeout
            .read_timeout(Duration::from_secs(config.timeout_seconds)) // Read timeout for large responses
            .pool_idle_timeout(Duration::from_secs(90))
            .build()?;

        Ok(Self { client, config })
    }

    /// Fetch recent transactions for a wallet address on specified chain
    pub async fn get_wallet_transactions(
        &self,
        wallet_address: &str,
        chain: GoldRushChain,
        limit: Option<u32>,
    ) -> Result<Vec<GoldRushTransaction>, GoldRushError> {
        // Try with requested limit first, then fallback to smaller sizes
        let fallback_sizes = if let Some(limit_val) = limit {
            if limit_val >= 1000 {
                vec![1000, 500, 250, 100]
            } else if limit_val >= 500 {
                vec![limit_val, 250, 100]
            } else if limit_val >= 100 {
                vec![limit_val, 100]
            } else {
                vec![limit_val]
            }
        } else {
            vec![100] // Conservative default for no limit
        };

        let mut last_error = None;
        
        for (attempt, page_size) in fallback_sizes.iter().enumerate() {
            let request = TransactionRequest {
                wallet_address: wallet_address.to_string(),
                chain,
                page_number: None, // Let API default to most recent transactions
                page_size: Some(*page_size),
                block_signed_at_asc: Some(false), // Latest first (newest transactions)
                no_logs: Some(false), // Include logs for DEX analysis
            };

            match self.fetch_transactions_page(&request).await {
                Ok(transactions) => {
                    if attempt > 0 {
                        info!(
                            "✅ Successfully fetched {} transactions with fallback page size {} (attempt {})",
                            transactions.len(),
                            page_size,
                            attempt + 1
                        );
                    } else {
                        info!(
                            "Fetched {} transactions for wallet {} on chain {}",
                            transactions.len(),
                            wallet_address,
                            chain.as_str()
                        );
                    }
                    return Ok(transactions);
                }
                Err(e) => {
                    if attempt == 0 {
                        info!("⚠️ Initial request with page size {} failed, trying smaller sizes...", page_size);
                    } else {
                        info!("⚠️ Fallback attempt {} (page size {}) failed", attempt + 1, page_size);
                    }
                    last_error = Some(e);
                }
            }
        }

        // If all attempts failed, return the last error
        Err(last_error.unwrap_or_else(|| GoldRushError::ApiError {
            message: "All retry attempts failed".to_string(),
        }))
    }

    /// Fetch a single page of transactions
    async fn fetch_transactions_page(
        &self,
        request: &TransactionRequest,
    ) -> Result<Vec<GoldRushTransaction>, GoldRushError> {
        let url = format!(
            "{}/{}/address/{}/transactions_v3/",
            self.config.base_url,
            request.chain.as_str(),
            request.wallet_address
        );

        info!("🔍 GoldRush API Request:");
        info!("  URL: {}", url);
        info!("  Chain: {}", request.chain.as_str());
        info!("  Wallet: {}", request.wallet_address);
        info!("  API Key: {}...{}", &self.config.api_key[..8], &self.config.api_key[self.config.api_key.len()-4..]);

        let mut req_builder = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key));

        // Add query parameters
        let mut query_params = Vec::new();
        if let Some(page) = request.page_number {
            query_params.push(format!("page-number={}", page));
            req_builder = req_builder.query(&[("page-number", page.to_string())]);
        }
        if let Some(size) = request.page_size {
            query_params.push(format!("page-size={}", size));
            req_builder = req_builder.query(&[("page-size", size.to_string())]);
        }
        if let Some(asc) = request.block_signed_at_asc {
            query_params.push(format!("block-signed-at-asc={}", asc));
            req_builder = req_builder.query(&[("block-signed-at-asc", asc.to_string())]);
        }
        if let Some(no_logs) = request.no_logs {
            query_params.push(format!("no-logs={}", no_logs));
            req_builder = req_builder.query(&[("no-logs", no_logs.to_string())]);
        }

        if !query_params.is_empty() {
            info!("  Query params: {}", query_params.join("&"));
        }

        info!("📡 Sending request to GoldRush API...");
        let start_time = std::time::Instant::now();
        let response = req_builder.send().await?;
        let elapsed = start_time.elapsed();
        let status = response.status();
        
        // Log response metadata
        info!("📨 Response status: {}", status);
        if let Some(content_length) = response.headers().get("content-length") {
            if let Ok(length_str) = content_length.to_str() {
                info!("📊 Content-Length: {} bytes", length_str);
            }
        }
        info!("⏱️ Request took: {:.2}s", elapsed.as_secs_f64());

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            error!("❌ GoldRush API error - Status: {}, Body: {}", status, text);

            return Err(match status.as_u16() {
                401 => GoldRushError::AuthError,
                429 => GoldRushError::RateLimit,
                _ => GoldRushError::ApiError {
                    message: format!("HTTP {}: {}", status, text),
                },
            });
        }

        // Get the response text first for logging
        let parse_start = std::time::Instant::now();
        let response_text = response.text().await?;
        let response_size = response_text.len();
        info!("✅ Received response, size: {} bytes ({:.1} MB)", response_size, response_size as f64 / 1_048_576.0);
        
        // Try to parse the response with enhanced error handling
        let api_response: GoldRushResponse<TransactionsResponse> = match serde_json::from_str(&response_text) {
            Ok(parsed) => {
                let parse_elapsed = parse_start.elapsed();
                info!("✅ Successfully parsed GoldRush response in {:.2}s", parse_elapsed.as_secs_f64());
                parsed
            },
            Err(e) => {
                let parse_elapsed = parse_start.elapsed();
                error!("❌ Failed to parse GoldRush response after {:.2}s: {}", parse_elapsed.as_secs_f64(), e);
                
                // Show response sample for debugging
                let sample_size = response_text.len().min(2000);
                error!("Response sample (first {} chars): {}", sample_size, &response_text[..sample_size]);
                
                // Show response end for debugging
                if response_text.len() > 2000 {
                    let end_start = response_text.len().saturating_sub(500);
                    error!("Response end (last 500 chars): {}", &response_text[end_start..]);
                }
                
                return Err(GoldRushError::ParseError { 
                    message: format!("JSON parse error: {} (response size: {} bytes)", e, response_size) 
                });
            }
        };

        if api_response.error {
            let error_msg = api_response
                .error_message
                .unwrap_or_else(|| "Unknown API error".to_string());
            error!("❌ API returned error flag: {}", error_msg);
            return Err(GoldRushError::ApiError { message: error_msg });
        }

        info!("✅ Found {} transactions in this page", api_response.data.items.len());
        Ok(api_response.data.items)
    }

    /// Get supported chains
    pub fn supported_chains() -> Vec<GoldRushChain> {
        vec![
            GoldRushChain::Ethereum,
            GoldRushChain::Base,
            GoldRushChain::Bsc,
        ]
    }

    /// Validate wallet address format for EVM chains
    pub fn validate_wallet_address(address: &str) -> Result<(), GoldRushError> {
        // Basic EVM address validation (0x + 40 hex chars)
        if !address.starts_with("0x") || address.len() != 42 {
            return Err(GoldRushError::InvalidAddress {
                address: address.to_string(),
            });
        }

        // Check if all characters after 0x are hex
        if !address[2..].chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(GoldRushError::InvalidAddress {
                address: address.to_string(),
            });
        }

        Ok(())
    }

    /// Get token balances for a wallet address
    pub async fn get_wallet_balances(
        &self,
        wallet_address: &str,
        chain: GoldRushChain,
    ) -> Result<Vec<TokenBalance>, GoldRushError> {
        let url = format!(
            "{}/{}/address/{}/balances_v2/",
            self.config.base_url,
            chain.as_str(),
            wallet_address
        );

        info!("📡 GoldRush Balances API Request:");
        info!("  URL: {}", url);
        info!("  Chain: {}", chain.as_str());
        info!("  Wallet: {}", wallet_address);
        info!("  Query: quote-currency=USD&no-spam=true");

        let start_time = std::time::Instant::now();
        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .query(&[("quote-currency", "USD"), ("no-spam", "true")])
            .send()
            .await?;
        
        let elapsed = start_time.elapsed();
        let status = response.status();
        info!("📨 Balances response: {} in {:.2}s", status, elapsed.as_secs_f64());

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            error!("❌ GoldRush Balances API error - Status: {}, Body: {}", status, text);
            return Err(match status.as_u16() {
                401 => GoldRushError::AuthError,
                429 => GoldRushError::RateLimit,
                _ => GoldRushError::ApiError {
                    message: format!("HTTP {}: {}", status, text),
                },
            });
        }

        let response_text = response.text().await?;
        let response_size = response_text.len();
        info!("📊 Response size: {} bytes ({:.1} KB)", response_size, response_size as f64 / 1024.0);
        
        let parse_start = std::time::Instant::now();
        let api_response: GoldRushResponse<BalancesResponse> = serde_json::from_str(&response_text)
            .map_err(|e| GoldRushError::ParseError {
                message: format!("Failed to parse balances response: {}", e),
            })?;
        let parse_elapsed = parse_start.elapsed();

        info!("✅ Parsed {} token balances in {:.3}s", api_response.data.items.len(), parse_elapsed.as_secs_f64());
        
        // Log significant balances preview
        let significant_count = api_response.data.items.iter()
            .filter(|b| !b.is_spam.unwrap_or(false) && b.balance.as_deref().unwrap_or("0") != "0")
            .count();
        info!("💰 Non-spam with balance: {} tokens", significant_count);
        
        Ok(api_response.data.items)
    }

    /// Get token transfers for a wallet address and specific token contract
    pub async fn get_token_transfers(
        &self,
        wallet_address: &str,
        chain: GoldRushChain,
        contract_address: &str,
        page_size: Option<u32>,
    ) -> Result<Vec<TokenTransfer>, GoldRushError> {
        let url = format!(
            "{}/{}/address/{}/transfers_v2/",
            self.config.base_url,
            chain.as_str(),
            wallet_address
        );

        info!("📡 GoldRush Transfers API Request:");
        info!("  URL: {}", url);
        info!("  Chain: {}", chain.as_str());
        info!("  Wallet: {}", wallet_address);
        info!("  Token: {}", contract_address);

        let mut query_params = vec![
            ("quote-currency", "USD".to_string()),
            ("contract-address", contract_address.to_string()),
        ];
        
        if let Some(size) = page_size {
            query_params.push(("page-size", size.to_string()));
            info!("  Page size: {}", size);
        }

        let start_time = std::time::Instant::now();
        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .query(&query_params)
            .send()
            .await?;

        let elapsed = start_time.elapsed();
        let status = response.status();
        info!("📨 Transfers response: {} in {:.2}s", status, elapsed.as_secs_f64());

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            error!("❌ GoldRush Transfers API error - Status: {}, Body: {}", status, text);
            return Err(match status.as_u16() {
                401 => GoldRushError::AuthError,
                429 => GoldRushError::RateLimit,
                _ => GoldRushError::ApiError {
                    message: format!("HTTP {}: {}", status, text),
                },
            });
        }

        let response_text = response.text().await?;
        let response_size = response_text.len();
        info!("📊 Response size: {} bytes ({:.1} KB)", response_size, response_size as f64 / 1024.0);
        
        let parse_start = std::time::Instant::now();
        let api_response: GoldRushResponse<TransfersResponse> = serde_json::from_str(&response_text)
            .map_err(|e| GoldRushError::ParseError {
                message: format!("Failed to parse transfers response: {}", e),
            })?;
        let parse_elapsed = parse_start.elapsed();

        info!("📄 Parsed {} transactions in {:.3}s", api_response.data.items.len(), parse_elapsed.as_secs_f64());

        // Flatten all transfers from all transactions
        let all_transfers: Vec<TokenTransfer> = api_response.data.items
            .into_iter()
            .flat_map(|tx| tx.transfers)
            .collect();

        info!("✅ Extracted {} token transfers for {}", all_transfers.len(), contract_address);
        
        // Log USD value summary
        if !all_transfers.is_empty() {
            let usd_transfers: Vec<_> = all_transfers.iter()
                .filter(|t| t.delta_quote.is_some())
                .collect();
            info!("💰 {} transfers have USD values", usd_transfers.len());
        }
        
        Ok(all_transfers)
    }

    /// Get all wallet transactions with logs using the efficient cross-chain API
    /// This replaces the need to make individual token transfer API calls
    pub async fn get_wallet_transactions_with_logs(
        &self,
        wallet_address: &str,
        chain: GoldRushChain,
        page_size: Option<u32>,
    ) -> Result<Vec<GoldRushTransaction>, GoldRushError> {
        let url = "https://api.covalenthq.com/v1/allchains/transactions/";
        let page_size = page_size.unwrap_or(20); // Start with smaller page size for stability
        
        info!("🚀 GoldRush Efficient Transactions API Request:");
        info!("  URL: {}", url);
        info!("  Chain: {}", chain.as_str());
        info!("  Wallet: {}", wallet_address);
        info!("  Page Size: {}", page_size);
        
        let start_time = std::time::Instant::now();
        
        // Build query parameters for cross-chain API
        let mut query_params = vec![
            ("addresses".to_string(), wallet_address.to_string()),
            ("chains".to_string(), chain.as_str().to_string()),
            ("page-size".to_string(), page_size.to_string()),
            ("with-logs".to_string(), "true".to_string()),
            ("key".to_string(), self.config.api_key.clone()),
        ];

        let response = self
            .client
            .get(url)
            .query(&query_params)
            .send()
            .await?;

        let status = response.status();
        let elapsed = start_time.elapsed();
        info!("📨 Efficient transactions response: {} in {:.2}s", status, elapsed.as_secs_f64());

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unable to read error response".to_string());
            error!("❌ API request failed with status {}: {}", status, error_text);
            return Err(GoldRushError::ApiError { 
                message: format!("API request failed with status {}: {}", status, error_text)
            });
        }

        let response_text = response.text().await?;
        let response_size = response_text.len();
        info!("📊 Response size: {} bytes ({:.1} KB)", response_size, response_size as f64 / 1024.0);

        let parse_start = std::time::Instant::now();
        
        // First, let's try to parse as a basic JSON to see the structure
        let json_value: serde_json::Value = match serde_json::from_str(&response_text) {
            Ok(value) => value,
            Err(e) => {
                error!("❌ Failed to parse even basic JSON: {}", e);
                
                // Show more detailed response sample for debugging
                let sample_size = std::cmp::min(2000, response_text.len());
                error!("Response sample (first {} chars): {}", sample_size, &response_text[..sample_size]);
                
                if response_text.len() > 2000 {
                    let end_start = response_text.len().saturating_sub(500);
                    error!("Response end (last 500 chars): {}", &response_text[end_start..]);
                }
                
                return Err(GoldRushError::ParseError { 
                    message: format!("Basic JSON parse error: {} (response size: {} bytes)", e, response_size) 
                });
            }
        };
        
        // Log the top-level structure
        info!("🔍 JSON structure analysis:");
        if let Some(obj) = json_value.as_object() {
            for key in obj.keys() {
                info!("  📋 Top-level key: {}", key);
            }
        }
        
        // Now try to parse as our specific structure
        let api_response: GoldRushResponse<crate::types::AllChainsTransactionsResponse> = match serde_json::from_value(json_value) {
            Ok(response) => response,
            Err(e) => {
                error!("❌ Failed to parse structured response: {}", e);
                
                return Err(GoldRushError::ParseError { 
                    message: format!("Structured parse error: {} (response size: {} bytes)", e, response_size) 
                });
            }
        };

        let parse_elapsed = parse_start.elapsed();
        info!("✅ Parsed response in {:.3}s", parse_elapsed.as_secs_f64());

        if api_response.error {
            let error_msg = api_response
                .error_message
                .unwrap_or_else(|| "Unknown API error".to_string());
            error!("❌ API returned error flag: {}", error_msg);
            return Err(GoldRushError::ApiError { message: error_msg });
        }

        let transactions = api_response.data.items;
        info!("✅ Found {} transactions with logs for wallet {}", transactions.len(), wallet_address);
        
        // Log statistics about the transactions
        let mut with_logs_count = 0;
        let mut total_log_events = 0;
        for tx in &transactions {
            if let Some(ref logs) = tx.log_events {
                if !logs.is_empty() {
                    with_logs_count += 1;
                    total_log_events += logs.len();
                }
            }
        }
        
        info!("📊 Transaction analysis:");
        info!("  📋 {} transactions with log events", with_logs_count);
        info!("  🏷️  {} total log events found", total_log_events);
        info!("  ⚡ Average {:.1} events per transaction", 
              if with_logs_count > 0 { total_log_events as f64 / with_logs_count as f64 } else { 0.0 });

        Ok(transactions)
    }
}

impl Default for GoldRushClient {
    fn default() -> Self {
        Self::new().expect("Failed to create default GoldRushClient")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_wallet_address() {
        // Valid addresses
        assert!(GoldRushClient::validate_wallet_address(
            "0x742d35cc6131b2f6e7f4c3b5e8a8c8d8f0b4c4e3"
        )
        .is_ok());
        assert!(GoldRushClient::validate_wallet_address(
            "0x742D35CC6131B2F6E7F4C3B5E8A8C8D8F0B4C4E3"
        )
        .is_ok());

        // Invalid addresses
        assert!(GoldRushClient::validate_wallet_address("742d35cc6131b2f6e7f4c3b5e8a8c8d8f0b4c4e3")
            .is_err()); // No 0x prefix
        assert!(GoldRushClient::validate_wallet_address(
            "0x742d35cc6131b2f6e7f4c3b5e8a8c8d8f0b4c4"
        )
        .is_err()); // Too short
        assert!(GoldRushClient::validate_wallet_address(
            "0x742d35cc6131b2f6e7f4c3b5e8a8c8d8f0b4c4e3aa"
        )
        .is_err()); // Too long
        assert!(GoldRushClient::validate_wallet_address(
            "0x742d35cc6131b2f6e7f4c3b5e8a8c8d8f0b4c4g3"
        )
        .is_err()); // Invalid hex character
    }

    #[test]
    fn test_chain_conversion() {
        assert_eq!(GoldRushChain::Ethereum.as_str(), "eth-mainnet");
        assert_eq!(GoldRushChain::Base.as_str(), "base-mainnet");
        assert_eq!(GoldRushChain::Bsc.as_str(), "bsc-mainnet");

        assert_eq!(
            GoldRushChain::from_str("eth-mainnet").unwrap(),
            GoldRushChain::Ethereum
        );
        assert_eq!(
            GoldRushChain::from_str("ethereum").unwrap(),
            GoldRushChain::Ethereum
        );
        assert_eq!(
            GoldRushChain::from_str("base-mainnet").unwrap(),
            GoldRushChain::Base
        );
        assert_eq!(
            GoldRushChain::from_str("base").unwrap(),
            GoldRushChain::Base
        );

        assert!(GoldRushChain::from_str("invalid-chain").is_err());
    }
}