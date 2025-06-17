// Simplified tx_parser for testing - returns empty results for now
// TODO: Implement proper Solana transaction parsing with updated SDK

use chrono::{DateTime, Utc};
use pnl_core::{FinancialEvent, EventMetadata, EventType};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json;
use solana_transaction_status::EncodedConfirmedTransactionWithStatusMeta;
use std::collections::HashMap;
use thiserror::Error;
use tracing::{debug, warn};
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Transaction parsing error: {0}")]
    Parsing(String),
    #[error("Invalid transaction format: {0}")]
    InvalidFormat(String),
    #[error("Missing required data: {0}")]
    MissingData(String),
}

pub type Result<T> = std::result::Result<T, ParseError>;

/// Configuration for transaction parsing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParserConfig {
    pub stable_coins: Vec<String>,
    pub aggregator_programs: Vec<String>,
}

impl Default for ParserConfig {
    fn default() -> Self {
        Self {
            stable_coins: vec![
                "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(), // USDC
                "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB".to_string(), // USDT
            ],
            aggregator_programs: vec![
                "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4".to_string(), // Jupiter
                "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc".to_string(), // Whirlpool
            ],
        }
    }
}

/// Simplified transaction parser
pub struct TransactionParser {
    config: ParserConfig,
}

impl TransactionParser {
    pub fn new(config: ParserConfig) -> Self {
        Self { config }
    }

    /// Parse a Solana transaction to extract financial events
    /// Based on TypeScript logic in accounts.ts computeChanges function
    pub fn parse_transaction(
        &self,
        transaction: &EncodedConfirmedTransactionWithStatusMeta,
        target_wallet: &str,
        transaction_id: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<Vec<FinancialEvent>> {
        debug!(
            "🔍 Parsing transaction {} for wallet {} at {}", 
            transaction_id, target_wallet, timestamp
        );
        
        // Debug: Check if transaction structure exists  
        debug!("📊 Transaction structure check - has meta: {}", 
               transaction.transaction.meta.is_some());
        
        // Extract transaction metadata - correct field access
        let meta = match &transaction.transaction.meta {
            Some(meta) => meta,
            None => {
                debug!("No transaction meta found for {}", transaction_id);
                return Ok(vec![]);
            }
        };
        
        // Get account keys to find wallet index - correct field access
        let account_keys = match &transaction.transaction.transaction {
            solana_transaction_status::EncodedTransaction::Json(ui_tx) => {
                match &ui_tx.message {
                    solana_transaction_status::UiMessage::Parsed(parsed_msg) => {
                        &parsed_msg.account_keys
                    }
                    _ => {
                        debug!("Transaction message not in parsed format for {}", transaction_id);
                        return Ok(vec![]);
                    }
                }
            }
            _ => {
                debug!("Transaction not in JSON format for {}", transaction_id);
                return Ok(vec![]);
            }
        };
        
        // Find wallet index in account keys
        let wallet_index = account_keys.iter().position(|key| {
            key.pubkey == target_wallet
        });
        
        let wallet_index = match wallet_index {
            Some(idx) => idx,
            None => {
                debug!("Target wallet {} not found in account keys for {}", target_wallet, transaction_id);
                return Ok(vec![]);
            }
        };
        
        // Compute SOL balance changes (TypeScript: preLamports vs postLamports)
        let pre_lamports = meta.pre_balances.get(wallet_index).unwrap_or(&0);
        let post_lamports = meta.post_balances.get(wallet_index).unwrap_or(&0);
        let sol_change = (*post_lamports as f64 - *pre_lamports as f64) / 1e9;
        
        debug!("SOL balance change for {}: {} SOL", target_wallet, sol_change);
        
        // Compute token balance changes (TypeScript: preTokenBalances vs postTokenBalances)
        let mut token_changes = Vec::new();
        let mut pre_token_map = HashMap::new();
        let mut post_token_map = HashMap::new();
        
        // Process pre-token balances - convert to Option and handle
        if let Some(pre_tokens) = serde_json::to_value(&meta.pre_token_balances).ok()
            .and_then(|v| serde_json::from_value::<Option<Vec<serde_json::Value>>>(v).ok())
            .flatten() {
            for balance_value in pre_tokens {
                if let Ok(balance) = serde_json::from_value::<serde_json::Value>(balance_value) {
                    if let (Some(owner), Some(mint)) = (
                        balance.get("owner").and_then(|v| v.as_str()),
                        balance.get("mint").and_then(|v| v.as_str())
                    ) {
                        if owner == target_wallet {
                            let amount = balance.get("uiTokenAmount")
                                .and_then(|ui| ui.get("uiAmount"))
                                .and_then(|amt| amt.as_f64())
                                .unwrap_or(0.0);
                            pre_token_map.insert(mint.to_string(), amount);
                        }
                    }
                }
            }
        }
        
        // Process post-token balances - convert to Option and handle
        if let Some(post_tokens) = serde_json::to_value(&meta.post_token_balances).ok()
            .and_then(|v| serde_json::from_value::<Option<Vec<serde_json::Value>>>(v).ok())
            .flatten() {
            for balance_value in post_tokens {
                if let Ok(balance) = serde_json::from_value::<serde_json::Value>(balance_value) {
                    if let (Some(owner), Some(mint)) = (
                        balance.get("owner").and_then(|v| v.as_str()),
                        balance.get("mint").and_then(|v| v.as_str())
                    ) {
                        if owner == target_wallet {
                            let amount = balance.get("uiTokenAmount")
                                .and_then(|ui| ui.get("uiAmount"))
                                .and_then(|amt| amt.as_f64())
                                .unwrap_or(0.0);
                            post_token_map.insert(mint.to_string(), amount);
                        }
                    }
                }
            }
        }
        
        // Calculate token differences (TypeScript: diff = post - pre)
        let mut all_mints = std::collections::HashSet::new();
        for mint in pre_token_map.keys() {
            all_mints.insert(mint.clone());
        }
        for mint in post_token_map.keys() {
            all_mints.insert(mint.clone());
        }
        
        for mint in all_mints {
            let pre_amount = pre_token_map.get(&mint).unwrap_or(&0.0);
            let post_amount = post_token_map.get(&mint).unwrap_or(&0.0);
            let diff = post_amount - pre_amount;
            
            if diff != 0.0 {
                // Skip stable coins for now (they're handled differently in TypeScript)
                if !self.config.stable_coins.contains(&mint) {
                    token_changes.push((mint, diff));
                }
            }
        }
        
        debug!("Found {} token changes for wallet {}", token_changes.len(), target_wallet);
        
        // Create FinancialEvent records for each token change (TypeScript: operation = diff < 0 ? "sell" : "buy")
        let mut events = Vec::new();
        
        for (mint, diff) in token_changes {
            let event_type = if diff < 0.0 { EventType::Sell } else { EventType::Buy };
            let token_amount = Decimal::try_from(diff.abs()).unwrap_or(Decimal::ZERO);
            let sol_amount = Decimal::try_from(sol_change.abs()).unwrap_or(Decimal::ZERO);
            
            let event = FinancialEvent {
                id: Uuid::new_v4(),
                transaction_id: transaction_id.to_string(),
                wallet_address: target_wallet.to_string(),
                event_type,
                token_mint: mint,
                token_amount,
                sol_amount,
                timestamp,
                transaction_fee: Decimal::ZERO, // TODO: Calculate from transaction
                metadata: EventMetadata {
                    instruction_index: Some(0),
                    program_id: None, // TODO: Extract from transaction
                    exchange: None,
                    price_per_token: None,
                    extra: HashMap::new(),
                },
            };
            
            debug!("Created {} event for token {} with amount {}", 
                   if diff < 0.0 { "SELL" } else { "BUY" }, 
                   event.token_mint, 
                   token_amount);
            
            events.push(event);
        }
        
        debug!("Created {} financial events for transaction {}", events.len(), transaction_id);
        Ok(events)
    }

    /// Parse multiple transactions
    pub fn parse_transactions(
        &self,
        transactions: &[EncodedConfirmedTransactionWithStatusMeta],
        target_wallet: &str,
    ) -> Result<Vec<FinancialEvent>> {
        debug!("🔍 Starting to parse {} transactions for wallet {}", transactions.len(), target_wallet);
        
        if transactions.is_empty() {
            warn!("🚨 No transactions provided to parser for wallet {}", target_wallet);
            return Ok(vec![]);
        }
        
        let mut all_events = Vec::new();
        
        for (i, transaction) in transactions.iter().enumerate() {
            // Extract signature as transaction ID
            let tx_id = self.extract_transaction_signature(transaction)
                .unwrap_or_else(|| format!("tx_{}", i));
            
            // Use block time from transaction or current time as fallback
            let timestamp = transaction.block_time
                .and_then(|bt| DateTime::from_timestamp(bt, 0))
                .unwrap_or_else(Utc::now);
            
            match self.parse_transaction(transaction, target_wallet, &tx_id, timestamp) {
                Ok(events) => {
                    debug!("Parsed transaction {} -> {} events", tx_id, events.len());
                    all_events.extend(events);
                }
                Err(e) => {
                    warn!("Failed to parse transaction {}: {}", tx_id, e);
                    // Continue with other transactions instead of failing completely
                }
            }
        }
        
        debug!("Total events parsed for wallet {}: {}", target_wallet, all_events.len());
        Ok(all_events)
    }
    
    /// Extract transaction signature from transaction data
    fn extract_transaction_signature(&self, transaction: &EncodedConfirmedTransactionWithStatusMeta) -> Option<String> {
        match &transaction.transaction.transaction {
            solana_transaction_status::EncodedTransaction::Json(ui_tx) => {
                ui_tx.signatures.first().cloned()
            }
            _ => {
                debug!("Cannot extract signature from non-JSON transaction");
                None
            }
        }
    }

    /// Extract wallet addresses from a transaction  
    /// For now, returns empty results - TODO: implement proper extraction
    pub fn extract_wallet_addresses(
        _transaction: &EncodedConfirmedTransactionWithStatusMeta,
    ) -> Vec<String> {
        debug!("Extracting wallet addresses from transaction");
        
        // TODO: Implement proper wallet address extraction
        // For now, return empty to allow system testing
        warn!("Wallet address extraction not yet implemented - returning empty results");
        
        vec![]
    }

    /// Classify events as buy/sell based on context
    /// For now, returns events as-is - TODO: implement proper classification
    pub fn classify_trade_event(
        events: &[FinancialEvent],
        target_wallet: &str,
    ) -> Vec<FinancialEvent> {
        debug!("Classifying {} events for wallet {}", events.len(), target_wallet);
        
        // TODO: Implement proper trade classification
        // For now, return events as-is to allow system testing
        events.to_vec()
    }

    /// Check if a token is a stable coin
    pub fn is_stable_coin(&self, token_mint: &str) -> bool {
        self.config.stable_coins.contains(&token_mint.to_string())
    }

    /// Create a dummy financial event for testing
    #[cfg(test)]
    pub fn create_test_event(
        wallet: &str,
        token_mint: &str,
        amount: Decimal,
        event_type: EventType,
    ) -> FinancialEvent {
        FinancialEvent {
            id: Uuid::new_v4(),
            transaction_id: "test_tx".to_string(),
            wallet_address: wallet.to_string(),
            event_type,
            token_mint: token_mint.to_string(),
            token_amount: amount,
            sol_amount: Decimal::ZERO,
            timestamp: Utc::now(),
            transaction_fee: Decimal::ZERO,
            metadata: EventMetadata {
                instruction_index: Some(0),
                program_id: Some("test_program".to_string()),
                exchange: None,
                price_per_token: None,
                extra: HashMap::new(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pnl_core::EventType;

    #[test]
    fn test_transaction_parser_creation() {
        let config = TransactionParserConfig::default();
        let parser = TransactionParser::new(config);
        
        assert!(parser.is_stable_coin("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"));
        assert!(!parser.is_stable_coin("So11111111111111111111111111111111111111112"));
    }

    #[test]
    fn test_empty_transaction_parsing() {
        let config = TransactionParserConfig::default();
        let parser = TransactionParser::new(config);
        
        // This would normally use a real transaction, but for now we just test the interface
        // TODO: Add proper test with mock transaction data
    }

    #[test]
    fn test_trade_classification() {
        let events = vec![
            TransactionParser::create_test_event(
                "test_wallet",
                "So11111111111111111111111111111111111111112",
                Decimal::new(1000, 0),
                EventType::Buy,
            ),
        ];
        
        let classified = TransactionParser::classify_trade_event(&events, "test_wallet");
        assert_eq!(classified.len(), 1);
    }
}