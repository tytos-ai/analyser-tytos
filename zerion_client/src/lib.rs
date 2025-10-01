use anyhow::Result;
use base64::{engine::general_purpose, Engine as _};
use chrono::{DateTime, Utc};
use config_manager::normalize_chain_for_zerion;
use pnl_core::{NewEventType, NewFinancialEvent};
use reqwest::{header::HeaderMap, Client};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
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

/// Result of conversion with enrichment tracking
#[derive(Debug, Clone)]
pub struct ConversionResult {
    pub events: Vec<NewFinancialEvent>,
    pub skipped_transactions: Vec<SkippedTransactionInfo>,
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

    /// Get wallet transactions for a specific chain
    pub async fn get_wallet_transactions_for_chain(
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

    pub async fn get_wallet_transactions(
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


    pub async fn get_wallet_transactions_with_limit(
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
    ) -> Vec<NewFinancialEvent> {
        let start_time = std::time::Instant::now();
        let mut events = Vec::new();
        let mut processed_count = 0u32;
        let mut skipped_count = 0u32;
        let mut error_count = 0u32;

        info!(
            "🔄 Converting {} transactions to financial events for wallet: {}",
            transactions.len(),
            wallet_address
        );

        for (tx_index, tx) in transactions.iter().enumerate() {
            debug!(
                "🔍 Processing transaction {}/{}: {} (type: {})",
                tx_index + 1,
                transactions.len(),
                tx.id,
                tx.attributes.operation_type
            );

            // Only process trade and send operations
            match tx.attributes.operation_type.as_str() {
                "trade" | "send" => {
                    processed_count += 1;
                    let transfer_count = tx.attributes.transfers.len();
                    debug!(
                        "💱 Transaction {}: {} transfers to process",
                        tx.id, transfer_count
                    );

                    let mut tx_events = 0u32;
                    // Extract chain_id from transaction relationships
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
                                // Log more specific reason for skipping the transfer
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

        events
    }

    fn convert_transfer_to_event(
        &self,
        tx: &ZerionTransaction,
        transfer: &ZerionTransfer,
        wallet_address: &str,
        chain_id: &str,
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
                "in" => NewEventType::Buy,
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
                // Both price and value are null - skip this transaction
                warn!(
                    "⚠️  Skipping transaction {}: both price and value are null for {} ({}) - tx_hash: {}",
                    tx.id, fungible_info.symbol, mint_address, tx.attributes.hash
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
            timestamp: tx.attributes.mined_at,
            transaction_hash: tx.attributes.hash.clone(),
        })
    }

    /// Extract information about transactions that would be skipped (missing price/value data)
    /// This is used for enrichment via BirdEye historical price API
    pub fn extract_skipped_transaction_info(
        &self,
        transactions: &[ZerionTransaction],
        wallet_address: &str,
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
                    };

                    info!(
                        "📋 Identified skipped transaction for enrichment: {} {} ({})",
                        skip_info.token_symbol, skip_info.token_mint, skip_info.tx_hash
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

