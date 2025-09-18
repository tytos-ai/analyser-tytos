use anyhow::Result;
use base64::{engine::general_purpose, Engine as _};
use chrono::{DateTime, Utc};
use pnl_core::{NewEventType, NewFinancialEvent};
use reqwest::{header::HeaderMap, Client};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZerionTransaction {
    #[serde(rename = "type")]
    pub transaction_type: String,
    pub id: String,
    pub attributes: ZerionTransactionAttributes,
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
    pub fungible_info: ZerionFungibleInfo,
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
            .timeout(std::time::Duration::from_secs(30))
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

    pub async fn get_wallet_transactions(
        &self,
        wallet_address: &str,
        currency: &str,
    ) -> Result<Vec<ZerionTransaction>, ZerionError> {
        let start_time = std::time::Instant::now();
        let mut all_transactions = Vec::new();
        let mut page_num = 1u32;
        let mut next_url = Some(format!(
            "{}/wallets/{}/transactions/?currency={}&page[size]={}&filter[chain_ids]={}&filter[trash]={}&filter[operation_types]={}",
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
    ) -> Result<Vec<ZerionTransaction>, ZerionError> {
        let start_time = std::time::Instant::now();
        let mut all_transactions = Vec::new();
        let mut page_num = 1u32;
        let mut next_url = Some(format!(
            "{}/wallets/{}/transactions/?currency={}&page[size]={}&filter[chain_ids]={}&filter[trash]={}&filter[operation_types]={}",
            self.base_url, wallet_address, currency, self.page_size, self.chain_ids, self.trash_filter, self.operation_types
        ));

        info!(
            "🔄 Starting limited transaction fetch for wallet: {} (limit: {})",
            wallet_address, limit
        );
        info!(
            "🎯 Filters: page_size={}, chain={}, trash={}, types={}",
            self.page_size, self.chain_ids, self.trash_filter, self.operation_types
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
                    for (transfer_index, transfer) in tx.attributes.transfers.iter().enumerate() {
                        debug!(
                            "🔄 Processing transfer {}/{} in tx {}: {} {} (direction: {})",
                            transfer_index + 1,
                            transfer_count,
                            tx.id,
                            transfer.quantity.numeric,
                            transfer.fungible_info.symbol,
                            transfer.direction
                        );

                        match self.convert_transfer_to_event(tx, transfer, wallet_address) {
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
                                warn!("⚠️ Skipped transfer {}/{} in tx {} due to invalid data (price: {:?}, value: {:?})",
                                      transfer_index + 1, transfer_count, tx.id,
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
    ) -> Option<NewFinancialEvent> {
        // Skip native SOL transfers in send operations (treating as wallet-to-wallet transfer, not trading)
        if transfer.fungible_info.symbol == "SOL" && tx.attributes.operation_type == "send" {
            return None;
        }

        let amount = match Decimal::from_str_exact(&transfer.quantity.numeric) {
            Ok(amt) => amt,
            Err(e) => {
                warn!(
                    "Failed to parse amount '{}': {}",
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
                        // Skip "in" transfers for sends (those are receives, not relevant for this wallet's P&L)
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

        // Extract Solana contract address
        let mint_address = transfer
            .fungible_info
            .implementations
            .iter()
            .find(|impl_| impl_.chain_id == "solana")
            .and_then(|impl_| impl_.address.as_ref())?;

        // Handle potentially null price/value fields with smart inference
        let (usd_price_per_token, usd_value) = match (transfer.price, transfer.value) {
            (Some(price), Some(value)) => {
                // Both price and value available - use directly
                debug!(
                    "✅ Using direct price data: price=${:.6}, value=${:.6} for {}",
                    price, value, transfer.fungible_info.symbol
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
                    price, amount, calculated_value, transfer.fungible_info.symbol
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
                        value, amount, calculated_price, transfer.fungible_info.symbol
                    );
                    (
                        Decimal::from_f64_retain(calculated_price).unwrap_or(Decimal::ZERO),
                        Decimal::from_f64_retain(value).unwrap_or(Decimal::ZERO),
                    )
                } else {
                    warn!(
                        "⚠️  Cannot calculate price: zero quantity for {}",
                        transfer.fungible_info.symbol
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
                    "⚠️  Skipping transaction: both price and value are null for {} ({})",
                    transfer.fungible_info.symbol, mint_address
                );
                return None;
            }
        };

        Some(NewFinancialEvent {
            wallet_address: wallet_address.to_string(),
            token_address: mint_address.clone(),
            token_symbol: transfer.fungible_info.symbol.clone(),
            event_type,
            quantity: amount,
            usd_price_per_token,
            usd_value,
            timestamp: tx.attributes.mined_at,
            transaction_hash: tx.attributes.hash.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_zerion_client_creation() {
        let client = ZerionClient::new(
            "https://api.zerion.io/v1".to_string(),
            "test_key".to_string(),
            100,
            "trade,send".to_string(),
            "solana".to_string(),
            "only_non_trash".to_string(),
        );
        assert!(client.is_ok());
    }

    #[tokio::test]
    #[ignore] // Run manually with: cargo test test_real_api_call -- --ignored
    async fn test_real_api_call() {
        let client = ZerionClient::new(
            "https://api.zerion.io/v1".to_string(),
            "zk_dev_69a0a2c7a84b433787f44efe5d1d6082".to_string(),
            100,
            "trade,send".to_string(),
            "solana".to_string(),
            "only_non_trash".to_string(),
        )
        .unwrap();

        let wallet_address = "5ngDQ3vMT7GHeFvXP5GLX79t6fqG2t15kyQTtst9zFdw";
        let result = client
            .get_wallet_transactions_with_limit(wallet_address, "usd", 10)
            .await;

        match result {
            Ok(transactions) => {
                println!("Successfully fetched {} transactions", transactions.len());
                for tx in &transactions {
                    println!(
                        "Transaction: {} - Type: {} - Transfers: {}",
                        tx.attributes.hash,
                        tx.attributes.operation_type,
                        tx.attributes.transfers.len()
                    );
                }

                // Test financial event conversion
                let events = client.convert_to_financial_events(&transactions, wallet_address);
                println!("Converted to {} financial events", events.len());

                for event in events.iter().take(5) {
                    println!(
                        "Event: {:?} {} {} @ {} USD = {} USD",
                        event.event_type,
                        event.quantity,
                        event.token_symbol,
                        event.usd_price_per_token,
                        event.usd_value
                    );
                }
            }
            Err(e) => {
                println!("Error fetching transactions: {:?}", e);
                panic!("API call failed: {:?}", e);
            }
        }
    }
}
