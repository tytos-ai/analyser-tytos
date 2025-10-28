use anyhow::Result;
use base64::{engine::general_purpose, Engine as _};
use chrono::{DateTime, Utc};
use config_manager::normalize_chain_for_zerion;
use pnl_core::{NewEventType, NewFinancialEvent};
use reqwest::{header::HeaderMap, Client};
use retry_utils::{retry_with_backoff, RetryableError, RetryConfig};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::str::FromStr;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod time_utils;
use tracing::{debug, error, info, warn};

#[derive(Error, Debug)]
pub enum ZerionError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON parsing failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("API error: {message}")]
    Api { message: String },
    #[error("No transaction data found")]
    NoData,
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("Invalid time range: {0}")]
    InvalidTimeRange(String),
    #[error("Invalid chain parameter: {0}")]
    InvalidChain(String),
    #[error("Rate limit exceeded (429)")]
    RateLimit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZerionTransaction {
    #[serde(rename = "type")]
    pub transaction_type: String,
    pub id: String,
    pub attributes: ZerionTransactionAttributes,
    pub relationships: Option<ZerionRelationships>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZerionRelationships {
    pub chain: Option<ZerionChainRelation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZerionChainRelation {
    pub data: ZerionChainData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZerionChainData {
    #[serde(rename = "type")]
    pub chain_type: String,
    pub id: String,  // This is the chain_id like "solana", "ethereum", etc.
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZerionTransactionAttributes {
    pub operation_type: String,
    pub hash: String,
    pub mined_at_block: i64,
    pub mined_at: DateTime<Utc>,
    pub sent_from: String,
    pub sent_to: String,
    pub status: String,
    pub nonce: i32,
    pub fee: ZerionFee,
    pub transfers: Vec<ZerionTransfer>,
    pub approvals: Vec<serde_json::Value>, // Not used for our purposes
    pub flags: ZerionFlags,
    pub acts: Vec<ZerionAct>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZerionFee {
    pub fungible_info: ZerionFungibleInfo,
    pub quantity: ZerionQuantity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZerionTransfer {
    pub fungible_info: Option<ZerionFungibleInfo>,
    pub direction: String,
    pub quantity: ZerionQuantity,
    pub value: Option<f64>,
    pub price: Option<f64>,
    pub sender: String,
    pub recipient: String,
    pub act_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZerionFlags {
    pub is_trash: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZerionAct {
    pub id: String,
    #[serde(rename = "type")]
    pub act_type: String,
    pub application_metadata: Option<ZerionApplicationMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZerionApplicationMetadata {
    pub contract_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZerionFungibleInfo {
    pub id: String,
    pub name: String,
    pub symbol: String,
    pub description: Option<String>,
    pub icon: Option<ZerionIcon>,
    pub flags: ZerionFungibleFlags,
    pub implementations: Vec<ZerionImplementation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZerionIcon {
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZerionFungibleFlags {
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZerionImplementation {
    pub chain_id: String,
    pub address: Option<String>,
    pub decimals: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZerionQuantity {
    pub int: String,
    pub decimals: u8,
    pub float: f64,
    pub numeric: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZerionResponse {
    pub data: Vec<ZerionTransaction>,
    pub links: ZerionLinks,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZerionLinks {
    pub next: Option<String>,
    pub prev: Option<String>,
}

/// Multi-hop swap context for transactions needing enrichment
#[derive(Debug, Clone)]
pub struct MultiHopSwapContext {
    pub tx_hash: String,
    pub tx_id: String,
    pub timestamp: DateTime<Utc>,
    pub chain_id: String,
    pub all_transfers: Vec<TransferSummary>,  // All transfers in the transaction
}

/// Result of conversion with enrichment tracking
#[derive(Debug, Clone)]
pub struct ConversionResult {
    pub events: Vec<NewFinancialEvent>,
    pub skipped_transactions: Vec<SkippedTransactionInfo>,
    pub incomplete_trades_count: u32,  // Count of trades with only OUT transfers (no IN side)
    pub multi_hop_contexts: HashMap<String, MultiHopSwapContext>,  // tx_hash → context
}

/// Summary of a transfer for multi-hop context tracking
#[derive(Debug, Clone)]
pub struct TransferSummary {
    pub token_address: String,
    pub token_symbol: String,
    pub direction: String,  // "in" or "out"
    pub quantity: Decimal,
    pub usd_value: Option<f64>,
}

/// Information about a skipped transaction that needs price enrichment
#[derive(Debug, Clone)]
pub struct SkippedTransactionInfo {
    pub zerion_tx_id: String,
    pub tx_hash: String,              // Solana signature for BirdEye historical price lookup
    pub wallet_address: String,
    pub token_mint: String,
    pub token_symbol: String,
    pub token_amount: Decimal,
    pub event_type: NewEventType,     // Buy/Sell/Receive
    pub timestamp: DateTime<Utc>,
    pub unix_timestamp: i64,
    pub chain_id: String,
    pub skip_reason: String,

    // === MULTI-HOP SWAP CONTEXT ===
    // For transactions where both sides need enrichment (e.g., USDUC → SOL → INFINITY)
    pub is_multi_hop_swap: bool,
    pub swap_input_token: Option<String>,
    pub swap_input_quantity: Option<Decimal>,
    // Store all other transfers in same transaction to calculate OUT-side value after enrichment
    pub other_transfers_in_tx: Vec<TransferSummary>,
}

/// Paired IN and OUT transfers for a trade (same act_id)
#[derive(Debug, Clone)]
struct TradePair<'a> {
    in_transfers: Vec<&'a ZerionTransfer>,
    out_transfers: Vec<&'a ZerionTransfer>,
    act_id: String,
}

/// Known stable currencies with reliable prices (native currencies, wrapped tokens, stablecoins)
/// Covers: Solana, Ethereum, Base, Binance Smart Chain
const STABLE_CURRENCIES: &[&str] = &[
    // === SOLANA ===
    // Native & Wrapped SOL
    "So11111111111111111111111111111111111111112",  // Wrapped SOL
    "11111111111111111111111111111111",              // Native SOL
    // Stablecoins
    "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", // USDC
    "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB", // USDT
    "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU", // USDC (native)
    // Liquid Staking Tokens (behave like native)
    "mSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So",  // mSOL (Marinade)
    "7dHbWXmci3dT8UFYWYZweBLXgycu7Y3iL6trKn1Y7ARj", // stSOL (Lido)
    "J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn", // jitoSOL

    // === ETHEREUM ===
    // Native & Wrapped ETH
    "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", // WETH
    "0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE", // ETH (placeholder)
    // Stablecoins
    "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48", // USDC
    "0xdAC17F958D2ee523a2206206994597C13D831ec7", // USDT
    "0x6B175474E89094C44Da98b954EedeAC495271d0F", // DAI
    "0x4Fabb145d64652a948d72533023f6E7A623C7C53", // BUSD
    "0x853d955aCEf822Db058eb8505911ED77F175b99e", // FRAX
    // Liquid Staking ETH
    "0xae7ab96520DE3A18E5e111B5EaAb095312D7fE84", // stETH (Lido)
    "0xBe9895146f7AF43049ca1c1AE358B0541Ea49704", // cbETH (Coinbase)
    "0xac3E018457B222d93114458476f3E3416Abbe38F", // sfrxETH (Frax)

    // === BASE ===
    // Native & Wrapped ETH on Base
    "0x4200000000000000000000000000000000000006", // WETH (Base)
    // Stablecoins on Base
    "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913", // USDC (Base native)
    "0xd9aAEc86B65D86f6A7B5B1b0c42FFA531710b6CA", // USDbC (Bridged USDC)

    // === BINANCE SMART CHAIN ===
    // Native & Wrapped BNB
    "0xbb4CdB9CBd36B01bD1cBaEBF2De08d9173bc095c", // WBNB
    // Stablecoins
    "0x8AC76a51cc950d9822D68b83fE1Ad97B32Cd580d", // USDC (BSC)
    "0x55d398326f99059fF775485246999027B3197955", // USDT (BSC)
    "0xe9e7CEA3DedcA5984780Bafc599bD69ADd087D56", // BUSD (BSC)
    "0x1AF3F329e8BE154074D8769D1FFa4eE058B1DBc3", // DAI (BSC)
];

/// Helper function to classify Zerion errors for retry strategy
fn classify_zerion_error(error: &ZerionError) -> RetryableError {
    match error {
        ZerionError::RateLimit => RetryableError::RateLimit,
        ZerionError::Api { message } if message.contains("HTTP 5") || message.starts_with("HTTP 5") => {
            RetryableError::ServerError
        }
        ZerionError::Http(_) => RetryableError::Timeout,
        _ => RetryableError::Other,
    }
}

/// Retry configuration for critical Zerion operations
/// Max delays: Rate Limit = 2s, Server Error = 1.2s, Timeout = 1s
fn get_zerion_retry_config() -> RetryConfig {
    RetryConfig {
        max_attempts: 3,
        rate_limit_delays_ms: vec![500, 1000, 2000],
        server_error_delays_ms: vec![300, 600, 1200],
        timeout_delays_ms: vec![500, 1000],
    }
}

#[derive(Clone)]
pub struct ZerionClient {
    client: Client,
    base_url: String,
    #[allow(dead_code)]
    api_key: String,
    page_size: u32,
    operation_types: String,
    chain_ids: String,
    trash_filter: String,
}

impl ZerionClient {
    pub fn new(
        base_url: String,
        api_key: String,
        page_size: u32,
        operation_types: String,
        chain_ids: String,
        trash_filter: String,
    ) -> Result<Self> {
        let mut headers = HeaderMap::new();

        // Create Basic Auth header
        let auth_string = format!("{}:", api_key);
        let encoded = general_purpose::STANDARD.encode(auth_string.as_bytes());
        let auth_header = format!("Basic {}", encoded);

        headers.insert(
            "Authorization",
            auth_header
                .parse()
                .map_err(|e| ZerionError::Config(format!("Invalid auth header: {}", e)))?,
        );

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .default_headers(headers)
            .build()?;

        Ok(Self {
            client,
            base_url,
            api_key,
            page_size,
            operation_types,
            chain_ids,
            trash_filter,
        })
    }

    /// Get wallet transactions for a specific chain (with retry)
    pub async fn get_wallet_transactions_for_chain(
        &self,
        wallet_address: &str,
        currency: &str,
        chain_id: &str,
    ) -> Result<Vec<ZerionTransaction>, ZerionError> {
        let wallet_owned = wallet_address.to_string();
        let currency_owned = currency.to_string();
        let chain_owned = chain_id.to_string();

        retry_with_backoff(
            || self.get_wallet_transactions_for_chain_internal(&wallet_owned, &currency_owned, &chain_owned),
            &get_zerion_retry_config(),
            classify_zerion_error,
        )
        .await
    }

    /// Internal implementation of get_wallet_transactions_for_chain (without retry)
    async fn get_wallet_transactions_for_chain_internal(
        &self,
        wallet_address: &str,
        currency: &str,
        chain_id: &str,
    ) -> Result<Vec<ZerionTransaction>, ZerionError> {
        // Final normalization before Zerion API call - this is the critical layer
        let original_chain = chain_id.to_string();
        let normalized_chain = normalize_chain_for_zerion(chain_id)
            .map_err(|e| ZerionError::InvalidChain(e))?;

        if original_chain != normalized_chain {
            info!(
                "Final chain normalization at Zerion client: '{}' -> '{}'",
                original_chain, normalized_chain
            );
        }

        let start_time = std::time::Instant::now();
        let mut all_transactions = Vec::new();
        let mut page_num = 1u32;
        let mut next_url = Some(format!(
            "{}/v1/wallets/{}/transactions/?currency={}&page[size]={}&filter[chain_ids]={}&filter[trash]={}&filter[operation_types]={}",
            self.base_url, wallet_address, currency, self.page_size, normalized_chain, self.trash_filter, self.operation_types
        ));

        info!(
            "🔄 Starting unlimited transaction fetch for wallet: {} on chain: {}",
            wallet_address, normalized_chain
        );
        info!(
            "🎯 Filters: page_size={}, chain={}, trash={}, types={}",
            self.page_size, normalized_chain, self.trash_filter, self.operation_types
        );

        while let Some(url) = next_url.take() {
            let page_start = std::time::Instant::now();

            info!("📄 Page {}: Fetching from Zerion API...", page_num);
            debug!("🌐 URL: {}", url);

            let response = self.client.get(&url).send().await?;

            if !response.status().is_success() {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();

                // Check for rate limit (429)
                if status.as_u16() == 429 {
                    error!("⏱️  Rate limit hit on page {}", page_num);
                    return Err(ZerionError::RateLimit);
                }

                error!(
                    "❌ Zerion API error {} on page {}: {}",
                    status, page_num, text
                );
                return Err(ZerionError::Api {
                    message: format!("HTTP {}: {}", status, text),
                });
            }

            let response_text = response.text().await?;
            debug!(
                "📄 Page {}: Response size: {} bytes",
                page_num,
                response_text.len()
            );

            let zerion_response: ZerionResponse = match serde_json::from_str(&response_text) {
                Ok(response) => response,
                Err(e) => {
                    error!("❌ JSON parsing failed on page {}: {}", page_num, e);
                    error!(
                        "🔍 Response snippet: {}",
                        &response_text.chars().take(500).collect::<String>()
                    );
                    return Err(ZerionError::Json(e));
                }
            };

            if zerion_response.data.is_empty() {
                info!(
                    "📄 Page {}: No more transactions, stopping pagination",
                    page_num
                );
                break;
            }

            let page_elapsed = page_start.elapsed();
            let has_next = zerion_response.links.next.is_some();

            info!(
                "📄 Page {}: Fetched {} transactions in {}ms, has_next: {}",
                page_num,
                zerion_response.data.len(),
                page_elapsed.as_millis(),
                has_next
            );

            all_transactions.extend(zerion_response.data);

            // Use the complete next URL provided by Zerion API
            next_url = zerion_response.links.next;
            if next_url.is_some() {
                debug!("🔗 Next URL available for page {}", page_num + 1);
            }

            page_num += 1;
        }

        let total_elapsed = start_time.elapsed();
        let avg_per_page = if page_num > 1 {
            total_elapsed.as_millis() / (page_num - 1) as u128
        } else {
            0
        };

        info!(
            "📊 Pagination Summary: {} pages, {} total transactions in {}ms",
            page_num - 1,
            all_transactions.len(),
            total_elapsed.as_millis()
        );
        info!(
            "⏱️ Performance: avg {}ms per page, {} transactions/second",
            avg_per_page,
            if total_elapsed.as_secs() > 0 {
                all_transactions.len() as u64 / total_elapsed.as_secs()
            } else {
                0
            }
        );

        Ok(all_transactions)
    }

    /// Get wallet transactions across all configured chains (with retry)
    pub async fn get_wallet_transactions(
        &self,
        wallet_address: &str,
        currency: &str,
    ) -> Result<Vec<ZerionTransaction>, ZerionError> {
        let wallet_owned = wallet_address.to_string();
        let currency_owned = currency.to_string();

        retry_with_backoff(
            || self.get_wallet_transactions_internal(&wallet_owned, &currency_owned),
            &get_zerion_retry_config(),
            classify_zerion_error,
        )
        .await
    }

    /// Internal implementation of get_wallet_transactions (without retry)
    async fn get_wallet_transactions_internal(
        &self,
        wallet_address: &str,
        currency: &str,
    ) -> Result<Vec<ZerionTransaction>, ZerionError> {
        let start_time = std::time::Instant::now();
        let mut all_transactions = Vec::new();
        let mut page_num = 1u32;
        let mut next_url = Some(format!(
            "{}/v1/wallets/{}/transactions/?currency={}&page[size]={}&filter[chain_ids]={}&filter[trash]={}&filter[operation_types]={}",
            self.base_url, wallet_address, currency, self.page_size, self.chain_ids, self.trash_filter, self.operation_types
        ));

        info!(
            "🔄 Starting unlimited transaction fetch for wallet: {}",
            wallet_address
        );
        info!(
            "🎯 Filters: page_size={}, chain={}, trash={}, types={}",
            self.page_size, self.chain_ids, self.trash_filter, self.operation_types
        );

        while let Some(url) = next_url.take() {
            let page_start = std::time::Instant::now();

            info!("📄 Page {}: Fetching from Zerion API...", page_num);
            debug!("🌐 URL: {}", url);

            let response = self.client.get(&url).send().await?;

            if !response.status().is_success() {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();

                // Check for rate limit (429)
                if status.as_u16() == 429 {
                    error!("⏱️  Rate limit hit on page {}", page_num);
                    return Err(ZerionError::RateLimit);
                }

                error!(
                    "❌ Zerion API error {} on page {}: {}",
                    status, page_num, text
                );
                return Err(ZerionError::Api {
                    message: format!("HTTP {}: {}", status, text),
                });
            }

            let response_text = response.text().await?;
            debug!(
                "📄 Page {}: Response size: {} bytes",
                page_num,
                response_text.len()
            );

            let zerion_response: ZerionResponse = match serde_json::from_str(&response_text) {
                Ok(response) => response,
                Err(e) => {
                    error!("❌ JSON parsing failed on page {}: {}", page_num, e);
                    error!(
                        "🔍 Response snippet: {}",
                        &response_text.chars().take(500).collect::<String>()
                    );
                    return Err(ZerionError::Json(e));
                }
            };

            if zerion_response.data.is_empty() {
                info!(
                    "📄 Page {}: No more transactions, stopping pagination",
                    page_num
                );
                break;
            }

            let page_elapsed = page_start.elapsed();
            let has_next = zerion_response.links.next.is_some();

            info!(
                "📄 Page {}: Fetched {} transactions in {}ms, has_next: {}",
                page_num,
                zerion_response.data.len(),
                page_elapsed.as_millis(),
                has_next
            );

            all_transactions.extend(zerion_response.data);

            // Use the complete next URL provided by Zerion API
            next_url = zerion_response.links.next;
            if next_url.is_some() {
                debug!("🔗 Next URL available for page {}", page_num + 1);
            }

            page_num += 1;
        }

        let total_elapsed = start_time.elapsed();
        let avg_per_page = if page_num > 1 {
            total_elapsed.as_millis() / (page_num - 1) as u128
        } else {
            0
        };

        info!(
            "📊 Pagination Summary: {} pages, {} total transactions in {}ms",
            page_num - 1,
            all_transactions.len(),
            total_elapsed.as_millis()
        );
        info!(
            "⏱️ Performance: avg {}ms per page, {} transactions/second",
            avg_per_page,
            if total_elapsed.as_secs() > 0 {
                all_transactions.len() as u64 / total_elapsed.as_secs()
            } else {
                0
            }
        );

        Ok(all_transactions)
    }


    /// Get wallet transactions with a limit (with retry)
    pub async fn get_wallet_transactions_with_limit(
        &self,
        wallet_address: &str,
        currency: &str,
        limit: usize,
        chain_id: Option<&str>,
    ) -> Result<Vec<ZerionTransaction>, ZerionError> {
        let wallet_owned = wallet_address.to_string();
        let currency_owned = currency.to_string();
        let chain_owned = chain_id.map(|s| s.to_string());

        retry_with_backoff(
            || self.get_wallet_transactions_with_limit_internal(&wallet_owned, &currency_owned, limit, chain_owned.as_deref()),
            &get_zerion_retry_config(),
            classify_zerion_error,
        )
        .await
    }

    /// Internal implementation of get_wallet_transactions_with_limit (without retry)
    async fn get_wallet_transactions_with_limit_internal(
        &self,
        wallet_address: &str,
        currency: &str,
        limit: usize,
        chain_id: Option<&str>,
    ) -> Result<Vec<ZerionTransaction>, ZerionError> {
        let start_time = std::time::Instant::now();
        let mut all_transactions = Vec::new();
        let mut page_num = 1u32;

        // Use provided chain_id or fallback to configured chain_ids
        let base_chain_filter = chain_id.unwrap_or(&self.chain_ids);

        // Final normalization before Zerion API call
        let normalized_chain = normalize_chain_for_zerion(base_chain_filter)
            .map_err(|e| ZerionError::InvalidChain(e))?;

        if base_chain_filter != normalized_chain {
            info!(
                "Final chain normalization at Zerion client (with limit): '{}' -> '{}'",
                base_chain_filter, normalized_chain
            );
        }

        let chain_filter = normalized_chain;

        let mut next_url = Some(format!(
            "{}/v1/wallets/{}/transactions/?currency={}&page[size]={}&filter[chain_ids]={}&filter[trash]={}&filter[operation_types]={}",
            self.base_url, wallet_address, currency, self.page_size, chain_filter, self.trash_filter, self.operation_types
        ));

        info!(
            "🔄 Starting limited transaction fetch for wallet: {} (limit: {})",
            wallet_address, limit
        );
        info!(
            "🎯 Filters: page_size={}, chain={}, trash={}, types={}",
            self.page_size, chain_filter, self.trash_filter, self.operation_types
        );

        while all_transactions.len() < limit {
            let Some(url) = next_url.take() else {
                info!(
                    "📄 No more pages available, stopping at {} transactions",
                    all_transactions.len()
                );
                break;
            };

            let page_start = std::time::Instant::now();
            let remaining_needed = limit - all_transactions.len();

            info!(
                "📄 Page {}: Fetching {} more transactions ({}/{})",
                page_num,
                remaining_needed,
                all_transactions.len(),
                limit
            );
            debug!("🌐 URL: {}", url);

            let response = self.client.get(&url).send().await?;

            if !response.status().is_success() {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();

                // Check for rate limit (429)
                if status.as_u16() == 429 {
                    error!("⏱️  Rate limit hit on page {}", page_num);
                    return Err(ZerionError::RateLimit);
                }

                error!(
                    "❌ Zerion API error {} on page {}: {}",
                    status, page_num, text
                );
                return Err(ZerionError::Api {
                    message: format!("HTTP {}: {}", status, text),
                });
            }

            let response_text = response.text().await?;
            debug!(
                "📄 Page {}: Response size: {} bytes",
                page_num,
                response_text.len()
            );

            let zerion_response: ZerionResponse = match serde_json::from_str(&response_text) {
                Ok(response) => response,
                Err(e) => {
                    error!("❌ JSON parsing failed on page {}: {}", page_num, e);
                    error!(
                        "🔍 Response snippet: {}",
                        &response_text.chars().take(500).collect::<String>()
                    );
                    return Err(ZerionError::Json(e));
                }
            };

            if zerion_response.data.is_empty() {
                info!(
                    "📄 Page {}: No more transactions, stopping pagination",
                    page_num
                );
                break;
            }

            let available_count = zerion_response.data.len();
            let to_take = std::cmp::min(remaining_needed, available_count);
            let page_elapsed = page_start.elapsed();
            let has_next = zerion_response.links.next.is_some();

            all_transactions.extend(zerion_response.data.into_iter().take(to_take));

            info!(
                "📄 Page {}: Took {} of {} transactions in {}ms, has_next: {}",
                page_num,
                to_take,
                available_count,
                page_elapsed.as_millis(),
                has_next
            );

            let progress_percentage = (all_transactions.len() as f64 / limit as f64 * 100.0) as u32;
            info!(
                "🎯 Progress: {}/{} transactions ({}% complete)",
                all_transactions.len(),
                limit,
                progress_percentage
            );

            if all_transactions.len() >= limit {
                info!("🎯 Target limit of {} transactions reached", limit);
                break;
            }

            // Use the complete next URL provided by Zerion API
            next_url = zerion_response.links.next;
            if next_url.is_some() {
                debug!("🔗 Next URL available for page {}", page_num + 1);
            }

            page_num += 1;
        }

        let total_elapsed = start_time.elapsed();
        let avg_per_page = if page_num > 1 {
            total_elapsed.as_millis() / (page_num - 1) as u128
        } else {
            0
        };

        info!(
            "📊 Pagination Summary: {} pages, {} total transactions in {}ms",
            page_num - 1,
            all_transactions.len(),
            total_elapsed.as_millis()
        );
        info!(
            "⏱️ Performance: avg {}ms per page, {} transactions/second",
            avg_per_page,
            if total_elapsed.as_secs() > 0 {
                all_transactions.len() as u64 / total_elapsed.as_secs()
            } else {
                0
            }
        );

        Ok(all_transactions)
    }

    /// Get wallet transactions with time-range filtering (ignores transaction limits)
    /// Get wallet transactions with time range filter (with retry)
    /// When time_range is provided, fetches ALL transactions within that period
    /// When time_range is None, falls back to max_transactions limit behavior
    pub async fn get_wallet_transactions_with_time_range(
        &self,
        wallet_address: &str,
        currency: &str,
        time_range: Option<&str>,
        max_transactions: Option<usize>,
        chain_id: Option<&str>,
    ) -> Result<Vec<ZerionTransaction>, ZerionError> {
        let wallet_owned = wallet_address.to_string();
        let currency_owned = currency.to_string();
        let time_range_owned = time_range.map(|s| s.to_string());
        let chain_owned = chain_id.map(|s| s.to_string());

        retry_with_backoff(
            || self.get_wallet_transactions_with_time_range_internal(
                &wallet_owned,
                &currency_owned,
                time_range_owned.as_deref(),
                max_transactions,
                chain_owned.as_deref(),
            ),
            &get_zerion_retry_config(),
            classify_zerion_error,
        )
        .await
    }

    /// Internal implementation of get_wallet_transactions_with_time_range (without retry)
    async fn get_wallet_transactions_with_time_range_internal(
        &self,
        wallet_address: &str,
        currency: &str,
        time_range: Option<&str>,
        max_transactions: Option<usize>,
        chain_id: Option<&str>,
    ) -> Result<Vec<ZerionTransaction>, ZerionError> {
        // If time_range is provided, use time-based filtering (ignore transaction limits)
        if let Some(time_range) = time_range {
            return self.get_wallet_transactions_time_filtered(
                wallet_address,
                currency,
                time_range,
                chain_id,
            ).await;
        }

        // Otherwise use limit-based approach
        if let Some(max_tx) = max_transactions {
            self.get_wallet_transactions_with_limit(wallet_address, currency, max_tx, chain_id).await
        } else {
            // Default: unlimited fetch
            self.get_wallet_transactions_for_chain(wallet_address, currency, chain_id.unwrap_or(&self.chain_ids)).await
        }
    }

    /// Get wallet transactions filtered by time range (fetches ALL transactions in period)
    async fn get_wallet_transactions_time_filtered(
        &self,
        wallet_address: &str,
        currency: &str,
        time_range: &str,
        chain_id: Option<&str>,
    ) -> Result<Vec<ZerionTransaction>, ZerionError> {
        use crate::time_utils::calculate_time_range;

        let start_time = std::time::Instant::now();
        let mut all_transactions = Vec::new();
        let mut page_num = 1u32;

        // Calculate time range timestamps
        let (min_mined_at, max_mined_at) = calculate_time_range(time_range)
            .map_err(|e| ZerionError::InvalidTimeRange(e.to_string()))?;

        // Use provided chain_id or fallback to configured chain_ids
        let base_chain_filter = chain_id.unwrap_or(&self.chain_ids);

        // Final normalization before Zerion API call
        let normalized_chain = normalize_chain_for_zerion(base_chain_filter)
            .map_err(|e| ZerionError::InvalidChain(e))?;

        if base_chain_filter != normalized_chain {
            info!(
                "Final chain normalization at Zerion client (time filtered): '{}' -> '{}'",
                base_chain_filter, normalized_chain
            );
        }

        let chain_filter = normalized_chain;

        // Build URL with time filters
        let mut next_url = Some(format!(
            "{}/v1/wallets/{}/transactions/?currency={}&page[size]={}&filter[chain_ids]={}&filter[trash]={}&filter[operation_types]={}&filter[min_mined_at]={}&filter[max_mined_at]={}",
            self.base_url, wallet_address, currency, self.page_size, chain_filter, self.trash_filter, self.operation_types, min_mined_at, max_mined_at
        ));

        info!(
            "🔄 Starting time-filtered transaction fetch for wallet: {} (time_range: {})",
            wallet_address, time_range
        );
        info!(
            "🎯 Time filters: {} to {} ({}ms to {}ms)",
            time_range, "now", min_mined_at, max_mined_at
        );
        info!(
            "🎯 Other filters: page_size={}, chain={}, trash={}, types={}",
            self.page_size, chain_filter, self.trash_filter, self.operation_types
        );

        // Fetch all pages (no transaction limit when using time filtering)
        while let Some(url) = next_url.take() {
            let page_start = std::time::Instant::now();

            info!("📄 Page {}: Fetching transactions in time range", page_num);
            debug!("🌐 URL: {}", url);

            let response = self.client.get(&url).send().await?;

            if !response.status().is_success() {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();

                // Check for rate limit (429)
                if status.as_u16() == 429 {
                    error!("⏱️  Rate limit hit on page {}", page_num);
                    return Err(ZerionError::RateLimit);
                }

                error!(
                    "❌ Zerion API error {} on page {}: {}",
                    status, page_num, text
                );
                return Err(ZerionError::Api {
                    message: format!("HTTP {}: {}", status, text),
                });
            }

            let zerion_response: ZerionResponse = response.json().await?;
            let page_elapsed = page_start.elapsed();
            let has_next = zerion_response.links.next.is_some();

            info!(
                "📄 Page {}: Fetched {} transactions in {}ms, has_next: {}",
                page_num,
                zerion_response.data.len(),
                page_elapsed.as_millis(),
                has_next
            );

            if zerion_response.data.is_empty() {
                info!(
                    "📄 Page {}: No more transactions in time range, stopping pagination",
                    page_num
                );
                break;
            }

            all_transactions.extend(zerion_response.data);

            // Use the complete next URL provided by Zerion API
            next_url = zerion_response.links.next;
            if next_url.is_some() {
                debug!("🔗 Next URL available for page {}", page_num + 1);
            }

            page_num += 1;
        }

        let total_elapsed = start_time.elapsed();
        let avg_per_page = if page_num > 1 {
            total_elapsed.as_millis() / (page_num - 1) as u128
        } else {
            0
        };

        info!(
            "📊 Time-filtered fetch summary: {} pages, {} total transactions in {}ms",
            page_num - 1,
            all_transactions.len(),
            total_elapsed.as_millis()
        );
        info!(
            "⏱️ Performance: avg {}ms per page, {} transactions/second",
            avg_per_page,
            if total_elapsed.as_secs() > 0 {
                all_transactions.len() as u64 / total_elapsed.as_secs()
            } else {
                0
            }
        );

        Ok(all_transactions)
    }

    /// Check if a token address is a known stable currency
    fn is_stable_currency(token_address: &str) -> bool {
        STABLE_CURRENCIES.contains(&token_address)
    }

    /// Group transfers by act_id for trade pairing
    fn pair_trade_transfers<'a>(transfers: &'a [ZerionTransfer]) -> Vec<TradePair<'a>> {
        use std::collections::HashMap;

        debug!("📦 Pairing {} transfers", transfers.len());

        let mut pairs_map: HashMap<String, TradePair<'a>> = HashMap::new();

        for transfer in transfers {
            let act_id = transfer.act_id.clone();
            let pair = pairs_map.entry(act_id.clone()).or_insert(TradePair {
                in_transfers: Vec::new(),
                out_transfers: Vec::new(),
                act_id,
            });

            match transfer.direction.as_str() {
                "in" | "self" => pair.in_transfers.push(transfer),
                "out" => pair.out_transfers.push(transfer),
                _ => {}
            }
        }

        let pairs: Vec<TradePair<'a>> = pairs_map.into_values().collect();

        debug!("📦 Created {} trade pairs", pairs.len());
        for pair in &pairs {
            debug!(
                "  Pair (act_id: {}): {} IN, {} OUT",
                pair.act_id,
                pair.in_transfers.len(),
                pair.out_transfers.len()
            );
        }

        pairs
    }

    /// Convert a trade pair using implicit swap pricing
    /// Returns a vector of financial events (BUY for IN side, SELL for OUT side)
    /// Also populates multi_hop_contexts for transactions needing enrichment
    fn convert_trade_pair_to_events(
        &self,
        tx: &ZerionTransaction,
        trade_pair: &TradePair,
        wallet_address: &str,
        chain_id: &str,
        multi_hop_contexts: &mut HashMap<String, MultiHopSwapContext>,
    ) -> Vec<NewFinancialEvent> {
        debug!(
            "💱 Converting trade pair (act_id: {}) for tx {}",
            trade_pair.act_id, tx.attributes.hash
        );
        debug!(
            "  IN transfers: {}, OUT transfers: {}",
            trade_pair.in_transfers.len(),
            trade_pair.out_transfers.len()
        );

        let mut events = Vec::new();

        // === MULTI-HOP SWAP DETECTION ===
        // Count unique token addresses to detect multi-hop swaps (Token A → SOL → Token B)
        // When we have 3+ unique assets including a stable currency, it's a multi-hop swap
        // and we should NOT use implicit pricing (which would use fee amounts incorrectly)
        use std::collections::HashSet;
        let unique_assets: HashSet<String> = trade_pair.in_transfers.iter()
            .chain(trade_pair.out_transfers.iter())
            .filter_map(|t| {
                t.fungible_info.as_ref().and_then(|f| {
                    f.implementations.iter()
                        .find(|i| i.chain_id == chain_id)
                        .and_then(|i| i.address.as_ref())
                })
            })
            .cloned()
            .collect();

        // Check if any asset is a stable currency (SOL, USDC, etc.)
        let has_stable = unique_assets.iter().any(|addr| Self::is_stable_currency(addr));

        // Detect multi-hop swap: 3+ unique assets including stable currency
        // Example: Moon (out) + KICK (in) + SOL (out, fees) = 3 assets
        // In this case, use NET transfer analysis to exclude intermediary tokens
        if unique_assets.len() >= 3 && has_stable {
            info!(
                "🔄 Multi-hop swap detected in tx {}: {} unique assets with stable currency. \
                 Using NET transfer analysis to filter intermediaries.",
                tx.id, unique_assets.len()
            );

            // Calculate NET transfers per token (IN - OUT)
            use std::collections::HashMap;
            let mut net_transfers: HashMap<String, (Decimal, Option<f64>, bool)> = HashMap::new();
            // Map: token_address -> (net_quantity, total_value, is_net_positive)

            // Add IN transfers (positive)
            for transfer in &trade_pair.in_transfers {
                if let Some(fungible_info) = &transfer.fungible_info {
                    if let Some(impl_) = fungible_info.implementations.iter().find(|i| i.chain_id == chain_id) {
                        if let Some(address) = &impl_.address {
                            let amount = match Self::parse_decimal_with_precision_handling(&transfer.quantity.numeric) {
                                Ok(amt) => amt,
                                Err(_) => continue,
                            };

                            let entry = net_transfers.entry(address.clone()).or_insert((Decimal::ZERO, None, true));
                            entry.0 += amount;
                            if let Some(value) = transfer.value {
                                entry.1 = Some(entry.1.unwrap_or(0.0) + value);
                            }
                        }
                    }
                }
            }

            // Subtract OUT transfers (negative)
            for transfer in &trade_pair.out_transfers {
                if let Some(fungible_info) = &transfer.fungible_info {
                    if let Some(impl_) = fungible_info.implementations.iter().find(|i| i.chain_id == chain_id) {
                        if let Some(address) = &impl_.address {
                            let amount = match Self::parse_decimal_with_precision_handling(&transfer.quantity.numeric) {
                                Ok(amt) => amt,
                                Err(_) => continue,
                            };

                            let entry = net_transfers.entry(address.clone()).or_insert((Decimal::ZERO, None, false));
                            entry.0 -= amount;
                            if let Some(value) = transfer.value {
                                entry.1 = Some(entry.1.unwrap_or(0.0) - value);
                            }
                            entry.2 = entry.0 > Decimal::ZERO;
                        }
                    }
                }
            }

            // Filter: Only create events for tokens with significant NET transfers
            // Threshold: abs(net_quantity) > 0.001 OR abs(net_value) > $0.01
            let net_threshold_quantity = Decimal::new(1, 3); // 0.001
            let net_threshold_value = 0.01; // $0.01

            info!(
                "📊 Multi-hop swap analysis: {} unique tokens, filtering by net transfers (threshold: qty > {} or value > ${})",
                net_transfers.len(), net_threshold_quantity, net_threshold_value
            );

            // === FIX FOR MULTI-HOP SWAP INVESTED CALCULATION ===
            // For BUY events (net positive transfers), we need to track what was actually SPENT (OUT-side)
            // to correctly calculate total_invested_usd (not the market value of what was received)
            //
            // Collect all OUT-side (negative net) tokens - these are what the user spent
            let out_side_tokens: Vec<(String, Decimal, Option<f64>)> = net_transfers.iter()
                .filter_map(|(addr, (qty, value, _))| {
                    if *qty < Decimal::ZERO {
                        // Negative net = spent/sold
                        Some((addr.clone(), qty.abs(), *value))
                    } else {
                        None
                    }
                })
                .collect();

            // Calculate total OUT-side USD value (sum of all tokens spent)
            let total_out_usd_value: f64 = out_side_tokens.iter()
                .filter_map(|(_, _, value)| *value)
                .sum();

            // Collect token symbols for OUT-side (for logging)
            let out_token_symbols: Vec<String> = out_side_tokens.iter()
                .filter_map(|(addr, _, _)| {
                    trade_pair.out_transfers.iter()
                        .find_map(|t| {
                            t.fungible_info.as_ref().and_then(|f| {
                                f.implementations.iter()
                                    .find(|i| i.chain_id == chain_id)
                                    .and_then(|i| i.address.as_ref())
                                    .filter(|a| *a == addr)
                                    .map(|_| f.symbol.clone())
                            })
                        })
                })
                .collect();

            if !out_side_tokens.is_empty() && total_out_usd_value > 0.0 {
                info!(
                    "💸 Multi-hop OUT-side: {} tokens spent, total value ${:.4} (tokens: {:?})",
                    out_side_tokens.len(),
                    total_out_usd_value,
                    out_token_symbols
                );
            }
            // === END FIX ===

            for (token_addr, (net_qty, net_value, is_positive)) in &net_transfers {
                let abs_net_qty = net_qty.abs();
                let abs_net_value = net_value.unwrap_or(0.0).abs();

                if abs_net_qty > net_threshold_quantity || abs_net_value > net_threshold_value {
                    // Significant net transfer - this is a real buy or sell
                    info!(
                        "  ✅ Including token {}: net_qty = {}, net_value = ${:.4} ({})",
                        &token_addr[..8.min(token_addr.len())],
                        net_qty,
                        net_value.unwrap_or(0.0),
                        if *is_positive { "BUY" } else { "SELL" }
                    );

                    // Find the corresponding transfer to create event
                    let relevant_transfers: Vec<&ZerionTransfer> = if *is_positive {
                        trade_pair.in_transfers.iter()
                            .copied()
                            .filter(|t| {
                                t.fungible_info.as_ref().and_then(|f| {
                                    f.implementations.iter()
                                        .find(|i| i.chain_id == chain_id)
                                        .and_then(|i| i.address.as_ref())
                                }).map(|addr| addr == token_addr).unwrap_or(false)
                            })
                            .collect()
                    } else {
                        trade_pair.out_transfers.iter()
                            .copied()
                            .filter(|t| {
                                t.fungible_info.as_ref().and_then(|f| {
                                    f.implementations.iter()
                                        .find(|i| i.chain_id == chain_id)
                                        .and_then(|i| i.address.as_ref())
                                }).map(|addr| addr == token_addr).unwrap_or(false)
                            })
                            .collect()
                    };

                    // Create event from the first matching transfer
                    if let Some(transfer) = relevant_transfers.first() {
                        // === FIX: For BUY events, pass swap_input info (what was spent) ===
                        if *is_positive && !out_side_tokens.is_empty() && total_out_usd_value > 0.0 {
                            // This is a BUY - use OUT-side as swap input (what user spent)
                            // For simplicity, we'll use a summary format for multi-token spends
                            let swap_input_token = if out_token_symbols.len() == 1 {
                                Some(out_token_symbols[0].clone())
                            } else {
                                Some(format!("Multi-token ({})", out_token_symbols.join("+")))
                            };

                            let swap_input_quantity = if out_side_tokens.len() == 1 {
                                Some(out_side_tokens[0].1)
                            } else {
                                None // Multiple tokens, can't meaningfully represent as single quantity
                            };

                            let swap_input_usd_value = Some(Decimal::from_f64_retain(total_out_usd_value).unwrap_or(Decimal::ZERO));

                            info!(
                                "  💰 BUY event swap_input: {} (${:.4})",
                                swap_input_token.as_ref().unwrap_or(&"unknown".to_string()),
                                total_out_usd_value
                            );

                            if let Some(event) = self.convert_transfer_to_event_with_swap_info(
                                tx, transfer, wallet_address, chain_id,
                                swap_input_token, swap_input_quantity, swap_input_usd_value
                            ) {
                                events.push(event);
                            }
                        } else {
                            // SELL event or BUY without OUT-side value - use standard conversion
                            //
                            // === MULTI-HOP CONTEXT TRACKING ===
                            // If this is a BUY event in a multi-hop swap, but we couldn't populate swap_input
                            // (because total_out_usd_value == 0), we need to store context for enrichment.
                            // This happens when BOTH the bought token AND the spent tokens need BirdEye enrichment.
                            if *is_positive && !out_side_tokens.is_empty() {
                                // This is a BUY without OUT-side USD value - store multi-hop context
                                // Create transfer summaries for ALL transfers in this trade pair
                                let all_transfer_summaries: Vec<TransferSummary> = trade_pair.in_transfers.iter()
                                    .chain(trade_pair.out_transfers.iter())
                                    .filter_map(|t| {
                                        let fungible_info = t.fungible_info.as_ref()?;
                                        let impl_ = fungible_info.implementations.iter()
                                            .find(|i| i.chain_id == chain_id)?;
                                        let token_address = impl_.address.as_ref()?.clone();
                                        let quantity = Self::parse_decimal_with_precision_handling(&t.quantity.numeric).ok()?;

                                        Some(TransferSummary {
                                            token_address,
                                            token_symbol: fungible_info.symbol.clone(),
                                            direction: t.direction.clone(),
                                            quantity,
                                            usd_value: t.value,
                                        })
                                    })
                                    .collect();

                                if !all_transfer_summaries.is_empty() {
                                    let context = MultiHopSwapContext {
                                        tx_hash: tx.attributes.hash.clone(),
                                        tx_id: tx.id.clone(),
                                        timestamp: tx.attributes.mined_at,
                                        chain_id: chain_id.to_string(),
                                        all_transfers: all_transfer_summaries,
                                    };

                                    multi_hop_contexts.insert(tx.attributes.hash.clone(), context);

                                    info!(
                                        "🔄 Stored multi-hop context for tx {} ({} transfers, BUY event needs enrichment)",
                                        tx.attributes.hash,
                                        trade_pair.in_transfers.len() + trade_pair.out_transfers.len()
                                    );
                                }
                            }
                            // === END MULTI-HOP CONTEXT TRACKING ===

                            if let Some(event) = self.convert_transfer_to_event(tx, transfer, wallet_address, chain_id) {
                                events.push(event);
                            }
                        }
                    }
                } else {
                    // Negligible net transfer - likely an intermediary (routing fee, etc.)
                    info!(
                        "  ⏭️  Filtering intermediary token {}: net_qty = {}, net_value = ${:.4} (below threshold)",
                        &token_addr[..8.min(token_addr.len())],
                        net_qty,
                        net_value.unwrap_or(0.0)
                    );
                }
            }

            info!(
                "✅ Multi-hop swap processing complete: {} events created from {} tokens (filtered {} intermediaries)",
                events.len(),
                net_transfers.len(),
                net_transfers.len() - events.len()
            );

            return events;
        }
        // === END MULTI-HOP SWAP DETECTION ===

        // Skip if missing either side
        if trade_pair.in_transfers.is_empty() || trade_pair.out_transfers.is_empty() {
            warn!(
                "Skipping incomplete trade pair in tx {}: {} IN transfers, {} OUT transfers",
                tx.id,
                trade_pair.in_transfers.len(),
                trade_pair.out_transfers.len()
            );
            return events;
        }

        // Find stable currency side (SOL, USDC, etc.) - this will have the reliable price
        // IMPORTANT: Sum ALL stable currency transfers, not just the first one!
        let mut stable_side_total_value: Option<f64> = None;
        let mut stable_transfers_count = 0usize;
        let mut volatile_transfers: Vec<&ZerionTransfer> = Vec::new();
        let mut stable_is_out = false;

        // Check OUT transfers for stable currency and SUM all their values
        let stable_out_total: f64 = trade_pair.out_transfers.iter()
            .filter_map(|transfer| {
                transfer.fungible_info.as_ref().and_then(|fungible_info| {
                    fungible_info.implementations.iter()
                        .find(|impl_| impl_.chain_id == chain_id)
                        .and_then(|impl_| impl_.address.as_ref())
                        .filter(|addr| Self::is_stable_currency(addr))
                        .and_then(|_| {
                            stable_transfers_count += 1;
                            transfer.value
                        })
                })
            })
            .sum();

        if stable_out_total > 0.0 && stable_transfers_count > 0 {
            stable_side_total_value = Some(stable_out_total);
            stable_is_out = true;
            // ALL IN transfers are volatile side
            volatile_transfers = trade_pair.in_transfers.clone();

            info!(
                "Found {} stable currency OUT transfers totaling ${:.4} in tx {} (act_id: {})",
                stable_transfers_count, stable_out_total, tx.id, trade_pair.act_id
            );
        } else {
            // Check IN transfers for stable currency and SUM all their values
            stable_transfers_count = 0;
            let stable_in_total: f64 = trade_pair.in_transfers.iter()
                .filter_map(|transfer| {
                    transfer.fungible_info.as_ref().and_then(|fungible_info| {
                        fungible_info.implementations.iter()
                            .find(|impl_| impl_.chain_id == chain_id)
                            .and_then(|impl_| impl_.address.as_ref())
                            .filter(|addr| Self::is_stable_currency(addr))
                            .and_then(|_| {
                                stable_transfers_count += 1;
                                transfer.value
                            })
                    })
                })
                .sum();

            if stable_in_total > 0.0 && stable_transfers_count > 0 {
                stable_side_total_value = Some(stable_in_total);
                stable_is_out = false;
                // ALL OUT transfers are volatile side
                volatile_transfers = trade_pair.out_transfers.clone();

                info!(
                    "Found {} stable currency IN transfers totaling ${:.4} in tx {} (act_id: {})",
                    stable_transfers_count, stable_in_total, tx.id, trade_pair.act_id
                );
            }
        }

        // === CRITICAL BUG FIX: Validate trade pair integrity ===
        // Before using implicit pricing, ensure all volatile transfers have the SAME direction.
        // Mixed directions (e.g., one "in" and one "out") indicate the trade pair incorrectly
        // grouped transfers from DIFFERENT blockchain transactions.
        // This causes events from SELL transactions to be labeled as BUY (and vice versa).
        if !volatile_transfers.is_empty() {
            use std::collections::HashSet;
            let volatile_directions: HashSet<&String> = volatile_transfers.iter()
                .map(|t| &t.direction)
                .collect();

            if volatile_directions.len() > 1 {
                warn!(
                    "⚠️  MIXED DIRECTIONS in trade pair for tx {} (act_id: {}): {:?}. \
                     This indicates transfers from DIFFERENT blockchain transactions were grouped together. \
                     Skipping implicit pricing and using standard conversion to avoid mis-labeling events.",
                    tx.id, trade_pair.act_id, volatile_directions
                );

                // Fall back to standard conversion - process each transfer independently
                for transfer in trade_pair.in_transfers.iter().chain(trade_pair.out_transfers.iter()) {
                    if let Some(event) = self.convert_transfer_to_event(tx, transfer, wallet_address, chain_id) {
                        events.push(event);
                    }
                }

                return events;
            }
        }
        // === END BUG FIX ===

        // If we found a stable side, use implicit pricing for ALL volatile transfers
        // BUT ONLY if there's exactly ONE volatile transfer (otherwise we'd multiply the value)
        if let Some(stable_total_value) = stable_side_total_value {
            if volatile_transfers.len() == 1 {
                // Safe to use implicit pricing with single volatile transfer
                info!(
                    "💱 Using implicit swap pricing for tx {} (act_id: {}): stable side TOTAL value = ${:.4} (from {} transfers), 1 volatile transfer",
                    tx.id, trade_pair.act_id, stable_total_value, stable_transfers_count
                );

                // Process the single volatile transfer with calculated implicit price
                if let Some(volatile_xfer) = volatile_transfers.first() {
                    if let Some(event) = self.convert_transfer_with_implicit_price(
                        tx,
                        volatile_xfer,
                        wallet_address,
                        chain_id,
                        stable_total_value,  // Use the TOTAL value, not just first transfer
                    ) {
                        events.push(event);
                    }
                }

                // Process ALL stable side transfers with Zerion's prices (they're reliable)
                let stable_side_transfers = if stable_is_out {
                    &trade_pair.out_transfers
                } else {
                    &trade_pair.in_transfers
                };

                for transfer in stable_side_transfers.iter() {
                    // Only process stable currency transfers
                    let is_stable = transfer.fungible_info.as_ref()
                        .and_then(|f| f.implementations.iter()
                            .find(|i| i.chain_id == chain_id)
                            .and_then(|i| i.address.as_ref()))
                        .map(|addr| Self::is_stable_currency(addr))
                        .unwrap_or(false);

                    if is_stable {
                        if let Some(event) = self.convert_transfer_to_event(tx, transfer, wallet_address, chain_id) {
                            events.push(event);
                        }
                    }
                }

                info!(
                    "✅ Implicit pricing complete: {} events created (1 volatile + {} stable)",
                    events.len(),
                    stable_transfers_count
                );
            } else {
                // Multiple volatile transfers - DON'T use implicit pricing!
                // Using implicit pricing with multiple transfers would give each the FULL stable value,
                // multiplying the investment (e.g., 3 transfers * $800 = $2,400 instead of $800)
                warn!(
                    "⚠️  Found {} volatile transfers in tx {} (act_id: {}) - SKIPPING implicit pricing to avoid value multiplication. Using standard conversion instead.",
                    volatile_transfers.len(),
                    tx.id,
                    trade_pair.act_id
                );

                // Process all transfers normally (using Zerion's prices)
                for transfer in trade_pair.in_transfers.iter().chain(trade_pair.out_transfers.iter()) {
                    if let Some(event) = self.convert_transfer_to_event(tx, transfer, wallet_address, chain_id) {
                        events.push(event);
                    }
                }

                info!(
                    "✅ Standard conversion complete: {} events created from {} total transfers",
                    events.len(),
                    trade_pair.in_transfers.len() + trade_pair.out_transfers.len()
                );
            }
        } else {
            // No stable currency found, fall back to regular conversion
            warn!(
                "No stable currency found in trade pair for tx {} (act_id: {}), using standard conversion",
                tx.id, trade_pair.act_id
            );

            // Process all transfers normally
            for transfer in trade_pair.in_transfers.iter().chain(trade_pair.out_transfers.iter()) {
                if let Some(event) = self.convert_transfer_to_event(tx, transfer, wallet_address, chain_id) {
                    events.push(event);
                }
            }
        }

        // Final debug output before returning
        debug!(
            "  ✅ Created {} events from this trade pair (act_id: {})",
            events.len(),
            trade_pair.act_id
        );
        for (i, event) in events.iter().enumerate() {
            let event_type = if event.event_type == NewEventType::Buy {
                "BUY"
            } else {
                "SELL"
            };
            debug!(
                "    Event #{}: {} {} {} @ ${:.10} = ${:.2} (tx: {})",
                i + 1,
                event_type,
                event.quantity,
                event.token_symbol,
                event.usd_price_per_token,
                event.usd_value,
                &event.transaction_hash[..8.min(event.transaction_hash.len())]
            );
        }

        events
    }

    /// Convert a transfer using an implicit price calculated from the swap
    fn convert_transfer_with_implicit_price(
        &self,
        tx: &ZerionTransaction,
        transfer: &ZerionTransfer,
        wallet_address: &str,
        chain_id: &str,
        stable_side_value_usd: f64,
    ) -> Option<NewFinancialEvent> {
        let fungible_info = transfer.fungible_info.as_ref()?;

        let amount = match Self::parse_decimal_with_precision_handling(&transfer.quantity.numeric) {
            Ok(amt) => amt,
            Err(e) => {
                warn!(
                    "Failed to parse amount '{}' for implicit pricing: {}",
                    transfer.quantity.numeric, e
                );
                return None;
            }
        };

        // Calculate implicit price: stable_value / quantity
        let quantity_f64 = amount.to_f64().unwrap_or(0.0);
        if quantity_f64 == 0.0 {
            warn!("Cannot calculate implicit price: zero quantity for {}", fungible_info.symbol);
            return None;
        }

        let implicit_price = stable_side_value_usd / quantity_f64;

        info!(
            "🔄 Calculated implicit price for {}: ${:.10} per token (from swap value ${:.2} / quantity {})",
            fungible_info.symbol, implicit_price, stable_side_value_usd, amount
        );

        // Determine event type
        let event_type = match transfer.direction.as_str() {
            "in" | "self" => NewEventType::Buy,
            "out" => NewEventType::Sell,
            _ => {
                warn!("Unknown direction in implicit pricing: {}", transfer.direction);
                return None;
            }
        };

        // Extract token address
        let mint_address = fungible_info
            .implementations
            .iter()
            .find(|impl_| impl_.chain_id == chain_id)
            .and_then(|impl_| impl_.address.as_ref())?;

        Some(NewFinancialEvent {
            wallet_address: wallet_address.to_string(),
            token_address: mint_address.clone(),
            token_symbol: fungible_info.symbol.clone(),
            chain_id: chain_id.to_string(),
            event_type,
            quantity: amount,
            usd_price_per_token: Decimal::from_f64_retain(implicit_price).unwrap_or(Decimal::ZERO),
            usd_value: Decimal::from_f64_retain(stable_side_value_usd).unwrap_or(Decimal::ZERO),
            swap_input_token: None,
            swap_input_quantity: None,
            swap_input_usd_value: None,
            timestamp: tx.attributes.mined_at,
            transaction_hash: tx.attributes.hash.clone(),
        })
    }

    /// Parse a decimal string with robust precision handling.
    /// Truncates excessive decimal places to fit within Decimal's 28-digit limit.
    fn parse_decimal_with_precision_handling(numeric_str: &str) -> Result<Decimal, String> {
        // First try exact parsing
        if let Ok(decimal) = Decimal::from_str_exact(numeric_str) {
            return Ok(decimal);
        }

        // If exact parsing fails, try regular parsing (allows some precision loss)
        if let Ok(decimal) = Decimal::from_str(numeric_str) {
            debug!("🔧 Truncated precision for amount: {} -> {}", numeric_str, decimal);
            return Ok(decimal);
        }

        // If both fail, try to manually truncate excessive decimal places
        if let Some(dot_pos) = numeric_str.find('.') {
            let integer_part = &numeric_str[..dot_pos];
            let decimal_part = &numeric_str[dot_pos + 1..];

            // Rust Decimal supports up to 28 decimal places, so truncate if needed
            let truncated_decimal_part = if decimal_part.len() > 28 {
                warn!("⚠️  Truncating excessive decimal precision: {} decimal places -> 28", decimal_part.len());
                &decimal_part[..28]
            } else {
                decimal_part
            };

            let truncated_str = format!("{}.{}", integer_part, truncated_decimal_part);

            if let Ok(decimal) = Decimal::from_str(&truncated_str) {
                debug!("🔧 Successfully parsed after truncation: {} -> {}", numeric_str, decimal);
                return Ok(decimal);
            }
        }

        // If all else fails, try parsing as f64 and converting
        if let Ok(float_val) = numeric_str.parse::<f64>() {
            if let Ok(decimal) = Decimal::try_from(float_val) {
                warn!("⚠️  Parsed via f64 conversion (precision loss): {} -> {}", numeric_str, decimal);
                return Ok(decimal);
            }
        }

        Err(format!("Unable to parse '{}' as Decimal with any method", numeric_str))
    }

    pub fn convert_to_financial_events(
        &self,
        transactions: &[ZerionTransaction],
        wallet_address: &str,
    ) -> ConversionResult {
        let start_time = std::time::Instant::now();
        let mut events = Vec::new();
        let mut processed_count = 0u32;
        let mut skipped_count = 0u32;
        let mut error_count = 0u32;
        let mut incomplete_trades_count = 0u32;  // NEW: Track incomplete trades
        let mut multi_hop_contexts: HashMap<String, MultiHopSwapContext> = HashMap::new();  // NEW: Track multi-hop contexts

        info!(
            "🔄 Converting {} transactions to financial events for wallet: {}",
            transactions.len(),
            wallet_address
        );

        for (tx_index, tx) in transactions.iter().enumerate() {
            debug!(
                "🔍 Processing transaction {}/{}: {} (hash: {}, type: {})",
                tx_index + 1,
                transactions.len(),
                tx.id,
                tx.attributes.hash,
                tx.attributes.operation_type
            );

            // Only process trade and send operations
            match tx.attributes.operation_type.as_str() {
                "trade" => {
                    processed_count += 1;
                    let transfer_count = tx.attributes.transfers.len();
                    debug!(
                        "💱 Transaction {}: {} transfers to process (TRADE)",
                        tx.id, transfer_count
                    );

                    // Extract chain_id from transaction relationships
                    let chain_id = tx
                        .relationships
                        .as_ref()
                        .and_then(|rel| rel.chain.as_ref())
                        .map(|chain| chain.data.id.as_str())
                        .unwrap_or("unknown");

                    // NEW: Use implicit swap pricing for trades
                    let trade_pairs = Self::pair_trade_transfers(&tx.attributes.transfers);

                    debug!(
                        "📊 Transaction {}: Grouped into {} trade pair(s)",
                        tx.id, trade_pairs.len()
                    );

                    let mut tx_events = 0u32;
                    for trade_pair in trade_pairs {
                        // Check if trade is complete
                        if trade_pair.in_transfers.is_empty() || trade_pair.out_transfers.is_empty() {
                            incomplete_trades_count += 1;
                            warn!(
                                "Incomplete trade detected in tx {} (hash: {}): {} IN transfers, {} OUT transfers",
                                tx.id, tx.attributes.hash,
                                trade_pair.in_transfers.len(),
                                trade_pair.out_transfers.len()
                            );
                        }

                        let pair_events = self.convert_trade_pair_to_events(tx, &trade_pair, wallet_address, chain_id, &mut multi_hop_contexts);
                        tx_events += pair_events.len() as u32;

                        for event in pair_events {
                            debug!(
                                "✅ Created {} event: {} {} @ ${:.10} = ${:.2}",
                                if event.event_type == NewEventType::Buy { "BUY" } else { "SELL" },
                                event.quantity,
                                event.token_symbol,
                                event.usd_price_per_token,
                                event.usd_value
                            );
                            events.push(event);
                        }
                    }

                    debug!(
                        "📊 Transaction {}: {} transfers → {} events (via trade pairing)",
                        tx.id, transfer_count, tx_events
                    );
                }
                "send" => {
                    processed_count += 1;
                    let transfer_count = tx.attributes.transfers.len();
                    debug!(
                        "📤 Transaction {}: {} transfers to process (SEND)",
                        tx.id, transfer_count
                    );

                    let mut tx_events = 0u32;
                    let chain_id = tx
                        .relationships
                        .as_ref()
                        .and_then(|rel| rel.chain.as_ref())
                        .map(|chain| chain.data.id.as_str())
                        .unwrap_or("unknown");

                    for (transfer_index, transfer) in tx.attributes.transfers.iter().enumerate() {
                        debug!(
                            "🔄 Processing transfer {}/{} in tx {} (chain: {}): {} {} (direction: {})",
                            transfer_index + 1,
                            transfer_count,
                            tx.id,
                            chain_id,
                            transfer.quantity.numeric,
                            transfer.fungible_info.as_ref().map(|f| &f.symbol).unwrap_or(&"UNKNOWN".to_string()),
                            transfer.direction
                        );

                        match self.convert_transfer_to_event(tx, transfer, wallet_address, chain_id) {
                            Some(event) => {
                                debug!(
                                    "✅ Created {} event: {} {} @ {} USD = {} USD",
                                    if event.event_type == NewEventType::Buy {
                                        "BUY"
                                    } else {
                                        "SELL"
                                    },
                                    event.quantity,
                                    event.token_symbol,
                                    event.usd_price_per_token,
                                    event.usd_value
                                );
                                events.push(event);
                                tx_events += 1;
                            }
                            None => {
                                let skip_reason = if transfer.fungible_info.is_none() {
                                    "missing fungible_info (token metadata)"
                                } else if transfer.price.is_none() && transfer.value.is_none() {
                                    "both price and value are null"
                                } else {
                                    "conversion failed (check earlier warnings for details)"
                                };

                                warn!("⚠️ Skipped transfer {}/{} in tx {} (hash: {}) due to: {} (price: {:?}, value: {:?})",
                                      transfer_index + 1, transfer_count, tx.id, tx.attributes.hash,
                                      skip_reason,
                                      transfer.price, transfer.value);
                                error_count += 1;
                            }
                        }
                    }

                    debug!(
                        "📊 Transaction {}: {} transfers → {} events",
                        tx.id, transfer_count, tx_events
                    );
                }
                _ => {
                    debug!(
                        "⏭️ Skipping transaction {} (type: {})",
                        tx.id, tx.attributes.operation_type
                    );
                    skipped_count += 1;
                }
            }
        }

        let total_elapsed = start_time.elapsed();
        let processing_rate = if total_elapsed.as_millis() > 0 {
            (transactions.len() as f64 / total_elapsed.as_millis() as f64 * 1000.0) as u32
        } else {
            0
        };

        info!(
            "✅ Conversion complete: {} transactions → {} financial events in {}ms",
            transactions.len(),
            events.len(),
            total_elapsed.as_millis()
        );
        info!(
            "📊 Processing stats: {} processed, {} skipped, {} errors",
            processed_count, skipped_count, error_count
        );
        info!(
            "⏱️ Processing rate: {} transactions/second",
            processing_rate
        );

        if error_count > 0 {
            warn!(
                "⚠️ {} transfers had data quality issues and were skipped",
                error_count
            );
        }

        if incomplete_trades_count > 0 {
            info!(
                "📊 Detected {} incomplete trade(s) with only OUT transfers",
                incomplete_trades_count
            );
        }

        info!(
            "📊 Multi-hop contexts tracked: {} transactions",
            multi_hop_contexts.len()
        );

        ConversionResult {
            events,
            skipped_transactions: Vec::new(),
            incomplete_trades_count,
            multi_hop_contexts,
        }
    }

    fn convert_transfer_to_event(
        &self,
        tx: &ZerionTransaction,
        transfer: &ZerionTransfer,
        wallet_address: &str,
        chain_id: &str,
    ) -> Option<NewFinancialEvent> {
        self.convert_transfer_to_event_with_swap_info(tx, transfer, wallet_address, chain_id, None, None, None)
    }

    fn convert_transfer_to_event_with_swap_info(
        &self,
        tx: &ZerionTransaction,
        transfer: &ZerionTransfer,
        wallet_address: &str,
        chain_id: &str,
        swap_input_token: Option<String>,
        swap_input_quantity: Option<Decimal>,
        swap_input_usd_value: Option<Decimal>,
    ) -> Option<NewFinancialEvent> {
        // Check if fungible_info is available, if not skip this transfer
        let fungible_info = match &transfer.fungible_info {
            Some(info) => info,
            None => {
                warn!(
                    "⚠️  Skipping transfer in tx {} due to missing fungible_info (this can happen with tokens that have no current holdings)",
                    tx.id
                );
                return None;
            }
        };

        // All transfers (including SOL sends) are treated as trading events for P&L calculation

        let amount = match Self::parse_decimal_with_precision_handling(&transfer.quantity.numeric) {
            Ok(amt) => amt,
            Err(e) => {
                warn!(
                    "Failed to parse amount '{}' after precision handling: {}",
                    transfer.quantity.numeric, e
                );
                return None;
            }
        };

        // Determine event type based on operation type and direction
        let event_type = match tx.attributes.operation_type.as_str() {
            "trade" => match transfer.direction.as_str() {
                "in" | "self" => NewEventType::Buy,  // "self" means tokens received into same wallet (common in DEX swaps)
                "out" => NewEventType::Sell,
                _ => {
                    warn!("Unknown direction in trade: {}", transfer.direction);
                    return None;
                }
            },
            "send" => {
                // All sends are treated as sells (disposing of tokens)
                match transfer.direction.as_str() {
                    "out" => NewEventType::Sell,
                    _ => {
                        // Skip "in" transfers for sends (those are receives, handled by "receive" operation type)
                        return None;
                    }
                }
            },
            "receive" => {
                // Handle received tokens (airdrops, transfers from other wallets)
                match transfer.direction.as_str() {
                    "in" => NewEventType::Receive,
                    _ => {
                        // Skip "out" transfers for receives (doesn't make logical sense)
                        return None;
                    }
                }
            }
            _ => {
                warn!(
                    "Unexpected operation type: {}",
                    tx.attributes.operation_type
                );
                return None;
            }
        };

        // Extract token address for the specific chain
        let mint_address = match fungible_info
            .implementations
            .iter()
            .find(|impl_| impl_.chain_id == chain_id)
            .and_then(|impl_| impl_.address.as_ref())
        {
            Some(addr) => addr,
            None => {
                // Log available implementations for debugging
                let available_chains: Vec<String> = fungible_info
                    .implementations
                    .iter()
                    .map(|impl_| impl_.chain_id.clone())
                    .collect();

                warn!(
                    "⚠️  No {} implementation found for token {} in tx {}. Available chains: {:?}",
                    chain_id,
                    fungible_info.symbol,
                    tx.id,
                    available_chains
                );
                return None;
            }
        };

        // Handle potentially null price/value fields with smart inference
        let (usd_price_per_token, usd_value) = match (transfer.price, transfer.value) {
            (Some(price), Some(value)) => {
                // Both price and value available - use directly
                debug!(
                    "✅ Using direct price data: price=${:.6}, value=${:.6} for {}",
                    price, value, fungible_info.symbol
                );
                (
                    Decimal::from_f64_retain(price).unwrap_or(Decimal::ZERO),
                    Decimal::from_f64_retain(value).unwrap_or(Decimal::ZERO),
                )
            }
            (Some(price), None) => {
                // Price available but no value - calculate value = price * quantity
                let calculated_value = price * amount.to_f64().unwrap_or(0.0);
                info!(
                    "🔄 Inferring value from price: price=${:.6} * quantity={} = ${:.6} for {}",
                    price, amount, calculated_value, fungible_info.symbol
                );
                (
                    Decimal::from_f64_retain(price).unwrap_or(Decimal::ZERO),
                    Decimal::from_f64_retain(calculated_value).unwrap_or(Decimal::ZERO),
                )
            }
            (None, Some(value)) => {
                // Value available but no price - calculate price = value / quantity
                let quantity_f64 = amount.to_f64().unwrap_or(0.0);
                if quantity_f64 > 0.0 {
                    let calculated_price = value / quantity_f64;
                    info!(
                        "🔄 Inferring price from value: value=${:.6} / quantity={} = ${:.6} for {}",
                        value, amount, calculated_price, fungible_info.symbol
                    );
                    (
                        Decimal::from_f64_retain(calculated_price).unwrap_or(Decimal::ZERO),
                        Decimal::from_f64_retain(value).unwrap_or(Decimal::ZERO),
                    )
                } else {
                    warn!(
                        "⚠️  Cannot calculate price: zero quantity for {}",
                        fungible_info.symbol
                    );
                    (
                        Decimal::ZERO,
                        Decimal::from_f64_retain(value).unwrap_or(Decimal::ZERO),
                    )
                }
            }
            (None, None) => {
                // Both price and value are null - skip this transfer
                // extract_skipped_transactions() will identify it from raw data for BirdEye enrichment
                info!(
                    "⚠️  Skipping transfer with NULL price/value: {} ({}) in tx {} - tx_hash: {} (will be enriched by BirdEye)",
                    fungible_info.symbol, mint_address, tx.id, tx.attributes.hash
                );
                return None;
            }
        };

        Some(NewFinancialEvent {
            wallet_address: wallet_address.to_string(),
            token_address: mint_address.clone(),
            token_symbol: fungible_info.symbol.clone(),
            chain_id: chain_id.to_string(),
            event_type,
            quantity: amount,
            usd_price_per_token,
            usd_value,
            swap_input_token,
            swap_input_quantity,
            swap_input_usd_value,
            timestamp: tx.attributes.mined_at,
            transaction_hash: tx.attributes.hash.clone(),
        })
    }

    /// Extract information about transactions that would be skipped (missing price/value data)
    /// This is used for enrichment via BirdEye historical price API
    /// multi_hop_contexts: Contains context for multi-hop swaps needing enrichment
    pub fn extract_skipped_transaction_info(
        &self,
        transactions: &[ZerionTransaction],
        wallet_address: &str,
        multi_hop_contexts: &HashMap<String, MultiHopSwapContext>,
    ) -> Vec<SkippedTransactionInfo> {
        let mut skipped_info = Vec::new();

        for tx in transactions {
            // Only process trade, send, and receive types
            if !matches!(
                tx.attributes.operation_type.as_str(),
                "trade" | "send" | "receive"
            ) {
                continue;
            }

            // Extract chain_id
            let chain_id = tx
                .relationships
                .as_ref()
                .and_then(|r| r.chain.as_ref())
                .map(|c| c.data.id.clone())
                .unwrap_or_else(|| "solana".to_string());

            // Check each transfer
            for transfer in &tx.attributes.transfers {
                // Skip if no fungible_info
                let fungible_info = match &transfer.fungible_info {
                    Some(info) => info,
                    None => continue,
                };

                // Only collect skip info if BOTH price and value are None
                if transfer.price.is_none() && transfer.value.is_none() {
                    // Parse amount
                    let amount = match Self::parse_decimal_with_precision_handling(&transfer.quantity.numeric) {
                        Ok(amt) => amt,
                        Err(_) => continue, // Skip if can't parse amount
                    };

                    // Determine event type
                    let event_type = match tx.attributes.operation_type.as_str() {
                        "trade" => match transfer.direction.as_str() {
                            "in" => NewEventType::Buy,
                            "out" => NewEventType::Sell,
                            _ => continue,
                        },
                        "send" => match transfer.direction.as_str() {
                            "out" => NewEventType::Sell,
                            _ => continue,
                        },
                        "receive" => match transfer.direction.as_str() {
                            "in" => NewEventType::Receive,
                            _ => continue,
                        },
                        _ => continue,
                    };

                    // Extract token address for the specific chain
                    let mint_address = match fungible_info
                        .implementations
                        .iter()
                        .find(|impl_| impl_.chain_id == chain_id)
                        .and_then(|impl_| impl_.address.as_ref())
                    {
                        Some(addr) => addr.clone(),
                        None => continue, // Skip if no implementation for this chain
                    };

                    // === MULTI-HOP CONTEXT POPULATION ===
                    // Check if this transaction is in the multi-hop contexts
                    let multi_hop_context = multi_hop_contexts.get(&tx.attributes.hash);
                    let (is_multi_hop, swap_input_token, swap_input_qty, other_transfers) = if let Some(context) = multi_hop_context {
                        // This is a multi-hop swap transaction
                        // For BUY events, collect OUT-side transfers as swap inputs
                        let out_transfers: Vec<TransferSummary> = context.all_transfers.iter()
                            .filter(|t| t.direction == "out")
                            .cloned()
                            .collect();

                        let swap_token = if out_transfers.len() == 1 {
                            Some(out_transfers[0].token_symbol.clone())
                        } else if !out_transfers.is_empty() {
                            let symbols: Vec<String> = out_transfers.iter().map(|t| t.token_symbol.clone()).collect();
                            Some(format!("Multi-token ({})", symbols.join("+")))
                        } else {
                            None
                        };

                        let swap_qty = if out_transfers.len() == 1 {
                            Some(out_transfers[0].quantity)
                        } else {
                            None
                        };

                        info!(
                            "🔄 Multi-hop swap detected for {}: swap_input = {:?}, {} other transfers",
                            fungible_info.symbol,
                            swap_token,
                            context.all_transfers.len()
                        );

                        (true, swap_token, swap_qty, context.all_transfers.clone())
                    } else {
                        // Not a multi-hop swap
                        (false, None, None, Vec::new())
                    };
                    // === END MULTI-HOP CONTEXT POPULATION ===

                    // Create skip info
                    let skip_info = SkippedTransactionInfo {
                        zerion_tx_id: tx.id.clone(),
                        tx_hash: tx.attributes.hash.clone(),
                        wallet_address: wallet_address.to_string(),
                        token_mint: mint_address,
                        token_symbol: fungible_info.symbol.clone(),
                        token_amount: amount,
                        event_type,
                        timestamp: tx.attributes.mined_at,
                        unix_timestamp: tx.attributes.mined_at.timestamp(),
                        chain_id: chain_id.clone(),
                        skip_reason: "missing_price_and_value".to_string(),
                        // Multi-hop context
                        is_multi_hop_swap: is_multi_hop,
                        swap_input_token,
                        swap_input_quantity: swap_input_qty,
                        other_transfers_in_tx: other_transfers,
                    };

                    info!(
                        "📋 Identified skipped transaction for enrichment: {} {} ({}) {}",
                        skip_info.token_symbol, skip_info.token_mint, skip_info.tx_hash,
                        if is_multi_hop { "[MULTI-HOP]" } else { "" }
                    );

                    skipped_info.push(skip_info);
                }
            }
        }

        if !skipped_info.is_empty() {
            info!("📊 Found {} transactions needing price enrichment", skipped_info.len());
        }

        skipped_info
    }
}

