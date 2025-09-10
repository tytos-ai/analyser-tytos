use crate::{
    error::GoldRushError,
    types::{
        GoldRushTransaction, ParsedGoldRushTransaction, TokenChange, TokenChangeType,
        TransactionType,
    },
};
use rust_decimal::Decimal;
use std::{collections::HashMap, str::FromStr};
use tracing::{debug, warn};

/// Parser for GoldRush transactions into P&L-ready format
#[derive(Debug, Clone)]
pub struct EvmTransactionParser {
    /// Wallet address being analyzed (to determine if changes are inbound/outbound)
    target_wallet: String,
}

impl EvmTransactionParser {
    pub fn new(target_wallet: String) -> Self {
        Self {
            target_wallet: target_wallet.to_lowercase(),
        }
    }

    /// Parse raw GoldRush transactions into P&L-ready format
    pub fn parse_transactions(
        &self,
        transactions: Vec<GoldRushTransaction>,
    ) -> Result<Vec<ParsedGoldRushTransaction>, GoldRushError> {
        let mut parsed_transactions = Vec::new();

        for tx in transactions {
            match self.parse_single_transaction(tx) {
                Ok(Some(parsed)) => parsed_transactions.push(parsed),
                Ok(None) => {
                    // Transaction was filtered out (no relevant token changes)
                    continue;
                }
                Err(e) => {
                    warn!("Failed to parse transaction: {}", e);
                    // Continue with other transactions instead of failing entirely
                    continue;
                }
            }
        }

        debug!(
            "Parsed {} transactions for wallet {}",
            parsed_transactions.len(),
            self.target_wallet
        );

        Ok(parsed_transactions)
    }

    /// Parse a single transaction
    fn parse_single_transaction(
        &self,
        tx: GoldRushTransaction,
    ) -> Result<Option<ParsedGoldRushTransaction>, GoldRushError> {
        // Calculate gas fee in USD
        let gas_fee_usd = if !tx.fees_paid.is_empty() {
            // fees_paid is in Wei, convert to ETH then to USD if value_quote available
            if let (Ok(fee_wei), Some(eth_price)) = (
                Decimal::from_str(&tx.fees_paid),
                tx.value_quote, // This represents ETH price in USD context
            ) {
                let fee_eth = fee_wei / Decimal::from(10_u64.pow(18)); // Wei to ETH
                Some(fee_eth * eth_price)
            } else {
                None
            }
        } else {
            None
        };

        // Extract token changes from log events
        let token_changes = self.extract_token_changes(&tx)?;

        // Skip transactions with no relevant token changes
        if token_changes.is_empty() {
            return Ok(None);
        }

        // Determine transaction type based on token changes and transaction structure
        let transaction_type = self.classify_transaction(&tx, &token_changes);

        // For sends (outbound transfers without corresponding inbound), we need to ensure
        // they are classified correctly as disposal events
        let final_transaction_type = self.refine_transaction_type(&tx, &token_changes, transaction_type);

        Ok(Some(ParsedGoldRushTransaction {
            tx_hash: tx.tx_hash,
            block_time: tx.block_signed_at,
            from_address: tx.from_address,
            to_address: tx.to_address,
            transaction_type: final_transaction_type,
            token_changes,
            gas_fee_usd,
        }))
    }

    /// Extract token changes from transaction log events
    fn extract_token_changes(
        &self,
        tx: &GoldRushTransaction,
    ) -> Result<Vec<TokenChange>, GoldRushError> {
        let mut token_changes = HashMap::<String, TokenChange>::new();

        // Process each log event for ERC-20 transfers and DEX swaps (if log events exist)
        let empty_vec = vec![];
        let log_events = tx.log_events.as_ref().unwrap_or(&empty_vec);
        debug!("Processing {} log events for tx {}", log_events.len(), tx.tx_hash);
        
        for log in log_events {
            if let Some(decoded) = &log.decoded {
                debug!("Found decoded event: {} in tx {}", decoded.name, tx.tx_hash);
                match decoded.name.as_str() {
                    "Transfer" => {
                        // ERC-20 Transfer event
                        if let Some(change) = self.extract_transfer_change(log, decoded)? {
                            debug!("Extracted Transfer: {} {} {} ({})", 
                                  match change.change_type {
                                      TokenChangeType::Increase => "+",
                                      TokenChangeType::Decrease => "-",
                                  },
                                  change.amount_formatted,
                                  change.token_symbol,
                                  change.token_address);
                            // Aggregate changes for the same token
                            match token_changes.get_mut(&change.token_address) {
                                Some(existing) => {
                                    // Combine amounts (positive for increase, negative for decrease)
                                    let existing_amount = match existing.change_type {
                                        TokenChangeType::Increase => existing.amount_formatted,
                                        TokenChangeType::Decrease => -existing.amount_formatted,
                                    };
                                    let new_amount = match change.change_type {
                                        TokenChangeType::Increase => change.amount_formatted,
                                        TokenChangeType::Decrease => -change.amount_formatted,
                                    };
                                    let combined = existing_amount + new_amount;

                                    existing.amount_formatted = combined.abs();
                                    existing.change_type = if combined >= Decimal::ZERO {
                                        TokenChangeType::Increase
                                    } else {
                                        TokenChangeType::Decrease
                                    };
                                    
                                    // Update USD value (take the most recent/highest)
                                    if change.usd_value.is_some() {
                                        existing.usd_value = change.usd_value;
                                    }
                                }
                                None => {
                                    token_changes.insert(change.token_address.clone(), change);
                                }
                            }
                        }
                    }
                    "Swap" | "SwapExactTokensForTokens" | "SwapTokensForExactTokens" => {
                        // DEX swap events - extract both sides of the swap
                        if let Some(swap_changes) = self.extract_swap_changes(log, decoded)? {
                            for change in swap_changes {
                                token_changes.insert(change.token_address.clone(), change);
                            }
                        }
                    }
                    _ => {
                        // Other decoded events might be relevant for specific DEXs
                        debug!("Unhandled decoded event: {}", decoded.name);
                    }
                }
            }
        }

        // Also check for ETH value transfers in the main transaction
        if let (Ok(value_wei), Some(value_usd)) = (Decimal::from_str(&tx.value), tx.value_quote) {
            if value_wei > Decimal::ZERO {
                let eth_change = self.create_eth_change(tx, value_wei, value_usd)?;
                if let Some(change) = eth_change {
                    token_changes.insert(change.token_address.clone(), change);
                }
            }
        }

        let changes: Vec<TokenChange> = token_changes.into_values().collect();
        debug!("Extracted {} token changes from tx {}", changes.len(), tx.tx_hash);
        Ok(changes)
    }

    /// Extract token change from Transfer log event
    fn extract_transfer_change(
        &self,
        log: &crate::types::LogEvent,
        decoded: &crate::types::DecodedLogEvent,
    ) -> Result<Option<TokenChange>, GoldRushError> {
        // Extract Transfer(from, to, value) parameters
        let mut from_addr = None;
        let mut to_addr = None;
        let mut value = None;

        if let Some(params) = &decoded.params {
            for param in params {
                match param.name.as_str() {
                    "from" => {
                        // Handle address as string in JSON
                        match &param.value {
                            serde_json::Value::String(addr) => {
                                from_addr = Some(addr.to_lowercase());
                            }
                            _ => {
                                debug!("Unexpected type for 'from' param: {:?}", param.value);
                            }
                        }
                    }
                    "to" => {
                        // Handle address as string in JSON
                        match &param.value {
                            serde_json::Value::String(addr) => {
                                to_addr = Some(addr.to_lowercase());
                            }
                            _ => {
                                debug!("Unexpected type for 'to' param: {:?}", param.value);
                            }
                        }
                    }
                    "value" | "amount" => {
                        // Handle numeric value - can be string or number in JSON
                        match &param.value {
                            serde_json::Value::String(val_str) => {
                                value = Decimal::from_str(val_str).ok();
                                if value.is_none() {
                                    debug!("Failed to parse value string: {}", val_str);
                                }
                            }
                            serde_json::Value::Number(num) => {
                                if let Some(u) = num.as_u64() {
                                    value = Some(Decimal::from(u));
                                } else if let Some(f) = num.as_f64() {
                                    value = Decimal::from_f64_retain(f);
                                }
                            }
                            _ => {
                                debug!("Unexpected type for 'value' param: {:?}", param.value);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Check if this transfer involves our target wallet
        let (is_outbound, is_inbound) = (
            from_addr.as_ref() == Some(&self.target_wallet),
            to_addr.as_ref() == Some(&self.target_wallet),
        );

        if !is_outbound && !is_inbound {
            return Ok(None); // Not relevant to our wallet
        }

        let (change_type, raw_amount) = if is_outbound && !is_inbound {
            (TokenChangeType::Decrease, value.unwrap_or_default())
        } else if is_inbound && !is_outbound {
            (TokenChangeType::Increase, value.unwrap_or_default())
        } else {
            // Both inbound and outbound (rare edge case)
            return Ok(None);
        };

        let token_decimals = log.sender_contract_decimals.unwrap_or(18);
        let formatted_amount = raw_amount / Decimal::from(10_u64.pow(token_decimals));

        Ok(Some(TokenChange {
            token_address: log.sender_address.clone(),
            token_symbol: log.sender_contract_ticker_symbol.clone().unwrap_or_default(),
            token_decimals,
            amount_raw: raw_amount.to_string(),
            amount_formatted: formatted_amount,
            usd_value: None, // Will be enriched later if needed
            change_type,
        }))
    }

    /// Extract token changes from DEX swap events
    fn extract_swap_changes(
        &self,
        _log: &crate::types::LogEvent,
        decoded: &crate::types::DecodedLogEvent,
    ) -> Result<Option<Vec<TokenChange>>, GoldRushError> {
        // This is a simplified implementation
        // Different DEXs have different swap event structures
        // For now, we'll rely on Transfer events to capture the actual token movements
        debug!("DEX swap detected: {}", decoded.name);
        Ok(None)
    }

    /// Create ETH change from main transaction value
    fn create_eth_change(
        &self,
        tx: &GoldRushTransaction,
        value_wei: Decimal,
        value_usd: Decimal,
    ) -> Result<Option<TokenChange>, GoldRushError> {
        let from_is_target = tx.from_address.to_lowercase() == self.target_wallet;
        let to_is_target = tx.to_address.as_ref()
            .map(|addr| addr.to_lowercase() == self.target_wallet)
            .unwrap_or(false);

        if !from_is_target && !to_is_target {
            return Ok(None);
        }

        let change_type = if from_is_target {
            TokenChangeType::Decrease
        } else {
            TokenChangeType::Increase
        };

        let eth_amount = value_wei / Decimal::from(10_u64.pow(18));

        Ok(Some(TokenChange {
            token_address: "0x0000000000000000000000000000000000000000".to_string(), // ETH
            token_symbol: "ETH".to_string(),
            token_decimals: 18,
            amount_raw: value_wei.to_string(),
            amount_formatted: eth_amount,
            usd_value: Some(value_usd),
            change_type,
        }))
    }

    /// Classify transaction type based on structure and token changes
    fn classify_transaction(
        &self,
        _tx: &GoldRushTransaction,
        token_changes: &[TokenChange],
    ) -> TransactionType {
        // Check if this looks like a DEX swap (both increases and decreases)
        let has_increases = token_changes.iter().any(|c| c.change_type == TokenChangeType::Increase);
        let has_decreases = token_changes.iter().any(|c| c.change_type == TokenChangeType::Decrease);

        if has_increases && has_decreases {
            return TransactionType::Swap;
        }

        // Check for simple sends (only decreases from our perspective)
        if has_decreases && !has_increases {
            // Additional check: if to_address is not a contract, it's likely a simple send
            return TransactionType::Send;
        }

        // Only increases (receiving tokens)
        if has_increases && !has_decreases {
            return TransactionType::Receive;
        }

        // Fallback for complex contract interactions
        TransactionType::ContractInteraction
    }

    /// Refine transaction type with additional logic for send detection
    fn refine_transaction_type(
        &self,
        _tx: &GoldRushTransaction,
        token_changes: &[TokenChange],
        initial_type: TransactionType,
    ) -> TransactionType {
        // If initially classified as Send, verify it should be treated as a disposal event
        if initial_type == TransactionType::Send {
            // Check if there are only token decreases and no corresponding increases
            let only_decreases = token_changes.iter()
                .all(|c| c.change_type == TokenChangeType::Decrease);
            
            if only_decreases {
                // This is indeed a send/disposal that should be treated as a sell
                return TransactionType::Send;
            }
        }

        initial_type
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_parser_creation() {
        let parser = EvmTransactionParser::new("0x742d35cc6131b2f6e7f4c3b5e8a8c8d8f0b4c4e3".to_string());
        assert_eq!(parser.target_wallet, "0x742d35cc6131b2f6e7f4c3b5e8a8c8d8f0b4c4e3");
    }

    #[test]
    fn test_transaction_classification() {
        let parser = EvmTransactionParser::new("0x742d35cc6131b2f6e7f4c3b5e8a8c8d8f0b4c4e3".to_string());

        // Test swap classification (both increases and decreases)
        let swap_changes = vec![
            TokenChange {
                token_address: "0x1234".to_string(),
                token_symbol: "USDC".to_string(),
                token_decimals: 6,
                amount_raw: "1000000".to_string(),
                amount_formatted: Decimal::from(1),
                usd_value: Some(Decimal::from(1)),
                change_type: TokenChangeType::Increase,
            },
            TokenChange {
                token_address: "0x0000000000000000000000000000000000000000".to_string(),
                token_symbol: "ETH".to_string(),
                token_decimals: 18,
                amount_raw: "100000000000000000".to_string(),
                amount_formatted: Decimal::from_str("0.1").unwrap(),
                usd_value: Some(Decimal::from(200)),
                change_type: TokenChangeType::Decrease,
            },
        ];

        let tx = create_dummy_transaction();
        let tx_type = parser.classify_transaction(&tx, &swap_changes);
        assert_eq!(tx_type, TransactionType::Swap);

        // Test send classification (only decreases)
        let send_changes = vec![
            TokenChange {
                token_address: "0x1234".to_string(),
                token_symbol: "USDC".to_string(),
                token_decimals: 6,
                amount_raw: "1000000".to_string(),
                amount_formatted: Decimal::from(1),
                usd_value: Some(Decimal::from(1)),
                change_type: TokenChangeType::Decrease,
            },
        ];

        let tx_type = parser.classify_transaction(&tx, &send_changes);
        assert_eq!(tx_type, TransactionType::Send);
    }

    fn create_dummy_transaction() -> GoldRushTransaction {
        GoldRushTransaction {
            block_signed_at: Utc::now(),
            block_height: 12345,
            block_hash: "0xabcd".to_string(),
            tx_hash: "0x1234".to_string(),
            tx_offset: 0,
            successful: true,
            miner_address: "0x0000".to_string(),
            from_address: "0x742d35cc6131b2f6e7f4c3b5e8a8c8d8f0b4c4e3".to_string(),
            from_address_label: None,
            to_address: Some("0x5678".to_string()),
            to_address_label: None,
            value: "0".to_string(),
            value_quote: None,
            pretty_value_quote: None,
            gas_metadata: crate::types::GasMetadata {
                contract_decimals: 18,
                contract_name: "ETH".to_string(),
                contract_ticker_symbol: "ETH".to_string(),
                contract_address: "0x0000000000000000000000000000000000000000".to_string(),
                supports_erc: None,
                logo_url: None,
            },
            gas_offered: 21000,
            gas_spent: 21000,
            gas_price: 1000000000,
            gas_quote: rust_decimal::Decimal::from(21),
            pretty_gas_quote: "$0.02".to_string(),
            gas_quote_rate: rust_decimal::Decimal::from(4000),
            fees_paid: "21000000000000".to_string(),
            explorers: vec![],
            log_events: None, // No log events for simple transfer
        }
    }
}