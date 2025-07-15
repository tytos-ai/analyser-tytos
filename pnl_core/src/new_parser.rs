use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, trace, warn};

use crate::GeneralTraderTransaction;

/// Parameters for creating a financial event
#[derive(Debug)]
struct EventCreationParams<'a> {
    token_address: &'a str,
    token_symbol: &'a str,
    event_type: NewEventType,
    quantity: Decimal,
    price_per_token: Decimal,
    timestamp: DateTime<Utc>,
    transaction_hash: &'a str,
}

/// Financial event types for the new P&L algorithm
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NewEventType {
    Buy,
    Sell,
}

/// Standardized financial event created from BirdEye transaction data
/// Following the new algorithm specification exactly
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewFinancialEvent {
    /// Wallet address that performed the transaction
    pub wallet_address: String,
    
    /// Token address (mint)
    pub token_address: String,
    
    /// Token symbol
    pub token_symbol: String,
    
    /// Event type (BUY or SELL)
    pub event_type: NewEventType,
    
    /// Token quantity (always positive, using absolute value)
    pub quantity: Decimal,
    
    /// USD price per token at transaction time
    pub usd_price_per_token: Decimal,
    
    /// USD value (quantity × price)
    pub usd_value: Decimal,
    
    /// Transaction timestamp
    pub timestamp: DateTime<Utc>,
    
    /// Transaction hash
    pub transaction_hash: String,
}

/// Result of parsing a single BirdEye transaction
#[derive(Debug, Clone)]
pub struct ParsedTransaction {
    /// The buy event from this transaction
    pub buy_event: NewFinancialEvent,
    
    /// The sell event from this transaction  
    pub sell_event: NewFinancialEvent,
}

/// Data Preparation & Parsing Module
/// Processes raw BirdEye transaction data into standardized financial events
/// Designed for parallel processing of multiple wallets
pub struct NewTransactionParser {
    wallet_address: String,
}

impl NewTransactionParser {
    /// Create a new parser for a specific wallet
    pub fn new(wallet_address: String) -> Self {
        Self { wallet_address }
    }
    
    /// Core algorithm: Parse BirdEye transactions into financial events
    /// 
    /// For every single transaction from BirdEye:
    /// - Examine both `quote` and `base` sides
    /// - Check the `ui_change_amount` sign for each side
    /// - Negative amount → SELL event (token disposed of)
    /// - Positive amount → BUY event (token acquired)
    /// - Always create exactly 2 events per transaction (one buy, one sell)
    /// - Use embedded price from BirdEye data for USD value calculation
    /// - Use absolute values for quantities to ensure mathematical consistency
    pub async fn parse_transactions(
        &self,
        transactions: Vec<GeneralTraderTransaction>,
    ) -> Result<Vec<NewFinancialEvent>, String> {
        let mut all_events = Vec::new();
        
        debug!(
            "Parsing {} BirdEye transactions for wallet {} using new algorithm",
            transactions.len(),
            self.wallet_address
        );
        
        for (tx_index, transaction) in transactions.iter().enumerate() {
            trace!(
                "Processing transaction {}/{}: {} at block_time: {}",
                tx_index + 1,
                transactions.len(),
                transaction.tx_hash,
                transaction.block_unix_time
            );
            
            match self.parse_single_transaction(transaction).await {
                Ok(parsed) => {
                    let buy_symbol = parsed.buy_event.token_symbol.clone();
                    let sell_symbol = parsed.sell_event.token_symbol.clone();
                    
                    all_events.push(parsed.buy_event);
                    all_events.push(parsed.sell_event);
                    
                    trace!(
                        "Successfully parsed transaction {} into buy ({}) and sell ({}) events",
                        transaction.tx_hash,
                        buy_symbol,
                        sell_symbol
                    );
                }
                Err(e) => {
                    warn!(
                        "Failed to parse transaction {}: {}. Skipping.",
                        transaction.tx_hash, e
                    );
                    continue;
                }
            }
        }
        
        debug!(
            "Successfully parsed {} transactions into {} financial events for wallet {}",
            transactions.len(),
            all_events.len(),
            self.wallet_address
        );
        
        Ok(all_events)
    }
    
    /// Parse a single BirdEye transaction into exactly 2 financial events
    async fn parse_single_transaction(
        &self,
        transaction: &GeneralTraderTransaction,
    ) -> Result<ParsedTransaction, String> {
        let timestamp = DateTime::from_timestamp(transaction.block_unix_time, 0)
            .ok_or_else(|| format!("Invalid timestamp: {}", transaction.block_unix_time))?;
        
        // Examine quote side
        let quote_change = Decimal::try_from(transaction.quote.ui_change_amount)
            .map_err(|e| format!("Invalid quote ui_change_amount: {}", e))?;
        
        let quote_price = transaction.quote.price
            .map(Decimal::try_from)
            .transpose()
            .map_err(|e| format!("Invalid quote price: {}", e))?
            .unwrap_or(Decimal::ZERO);
        
        // Examine base side
        let base_change = Decimal::try_from(transaction.base.ui_change_amount)
            .map_err(|e| format!("Invalid base ui_change_amount: {}", e))?;
        
        let base_price = transaction.base.price
            .map(Decimal::try_from)
            .transpose()
            .map_err(|e| format!("Invalid base price: {}", e))?
            .unwrap_or(Decimal::ZERO);
        
        // Create events based on ui_change_amount signs
        let (buy_event, sell_event) = if quote_change < Decimal::ZERO && base_change > Decimal::ZERO {
            // Quote is negative (SELL), Base is positive (BUY)
            let sell_event = self.create_financial_event(&EventCreationParams {
                token_address: &transaction.quote.address,
                token_symbol: &transaction.quote.symbol,
                event_type: NewEventType::Sell,
                quantity: quote_change.abs(), // Use absolute value for quantity
                price_per_token: quote_price,
                timestamp,
                transaction_hash: &transaction.tx_hash,
            })?;
            
            let buy_event = self.create_financial_event(&EventCreationParams {
                token_address: &transaction.base.address,
                token_symbol: &transaction.base.symbol,
                event_type: NewEventType::Buy,
                quantity: base_change.abs(), // Use absolute value for quantity
                price_per_token: base_price,
                timestamp,
                transaction_hash: &transaction.tx_hash,
            })?;
            
            (buy_event, sell_event)
        } else if quote_change > Decimal::ZERO && base_change < Decimal::ZERO {
            // Quote is positive (BUY), Base is negative (SELL)
            let buy_event = self.create_financial_event(&EventCreationParams {
                token_address: &transaction.quote.address,
                token_symbol: &transaction.quote.symbol,
                event_type: NewEventType::Buy,
                quantity: quote_change.abs(), // Use absolute value for quantity
                price_per_token: quote_price,
                timestamp,
                transaction_hash: &transaction.tx_hash,
            })?;
            
            let sell_event = self.create_financial_event(&EventCreationParams {
                token_address: &transaction.base.address,
                token_symbol: &transaction.base.symbol,
                event_type: NewEventType::Sell,
                quantity: base_change.abs(), // Use absolute value for quantity
                price_per_token: base_price,
                timestamp,
                transaction_hash: &transaction.tx_hash,
            })?;
            
            (buy_event, sell_event)
        } else {
            return Err(format!(
                "Invalid transaction: both sides have same sign or zero. Quote: {}, Base: {}",
                quote_change, base_change
            ));
        };
        
        debug!(
            "Parsed transaction {}: BUY {} {} @ ${}, SELL {} {} @ ${}",
            transaction.tx_hash,
            buy_event.quantity,
            buy_event.token_symbol,
            buy_event.usd_price_per_token,
            sell_event.quantity,
            sell_event.token_symbol,
            sell_event.usd_price_per_token
        );
        
        Ok(ParsedTransaction {
            buy_event,
            sell_event,
        })
    }
    
    /// Create a standardized financial event
    fn create_financial_event(
        &self,
        params: &EventCreationParams,
    ) -> Result<NewFinancialEvent, String> {
        // Calculate USD value using absolute quantity and price
        let usd_value = params.quantity * params.price_per_token;
        
        // Validate that we have meaningful values
        if params.quantity <= Decimal::ZERO {
            return Err(format!(
                "Invalid quantity: {} (must be positive after taking absolute value)",
                params.quantity
            ));
        }
        
        if params.price_per_token < Decimal::ZERO {
            return Err(format!(
                "Invalid price: {} (must be non-negative)",
                params.price_per_token
            ));
        }
        
        Ok(NewFinancialEvent {
            wallet_address: self.wallet_address.clone(),
            token_address: params.token_address.to_string(),
            token_symbol: params.token_symbol.to_string(),
            event_type: params.event_type.clone(),
            quantity: params.quantity,
            usd_price_per_token: params.price_per_token,
            usd_value,
            timestamp: params.timestamp,
            transaction_hash: params.transaction_hash.to_string(),
        })
    }
    
    /// Group financial events by token address for P&L processing
    /// This is the data handoff interface to the P&L Engine
    pub fn group_events_by_token(
        events: Vec<NewFinancialEvent>,
    ) -> HashMap<String, Vec<NewFinancialEvent>> {
        let mut grouped = HashMap::new();
        
        for event in events {
            grouped
                .entry(event.token_address.clone())
                .or_insert_with(Vec::new)
                .push(event);
        }
        
        // Sort events within each token by timestamp
        for events in grouped.values_mut() {
            events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        }
        
        debug!(
            "Grouped events into {} token groups for P&L processing",
            grouped.len()
        );
        
        grouped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TokenTransactionSide;
    
    #[tokio::test]
    async fn test_parse_sol_to_bonk_transaction() {
        let parser = NewTransactionParser::new("test_wallet".to_string());
        
        // Sample transaction from documentation: SOL → BONK
        let transaction = GeneralTraderTransaction {
            quote: TokenTransactionSide {
                symbol: "SOL".to_string(),
                decimals: 9,
                address: "So11111111111111111111111111111111111111112".to_string(),
                amount: 0,
                transfer_type: None,
                type_swap: "from".to_string(),
                ui_amount: 0.0,
                ui_change_amount: -3.54841245, // Negative = SELL
                price: Some(150.92661594596476),
                nearest_price: None,
                change_amount: 0,
                fee_info: None,
            },
            base: TokenTransactionSide {
                symbol: "Bonk".to_string(),
                decimals: 5,
                address: "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263".to_string(),
                amount: 0,
                transfer_type: None,
                type_swap: "to".to_string(),
                ui_amount: 0.0,
                ui_change_amount: 31883370.79991, // Positive = BUY
                price: Some(1.6796824680689412e-05),
                nearest_price: None,
                change_amount: 0,
                fee_info: None,
            },
            base_price: Some(1.6796824680689412e-05),
            quote_price: 150.92661594596476,
            tx_hash: "VKQDkkQ3V6zHayKvmXXmMJVuBWqnaQdUDgkAdPmr9nEa1tkiLZaZvhzkM1gim865EnXxVomSNM1TcBxHDyi5AW7".to_string(),
            source: "test".to_string(),
            block_unix_time: 1751614209,
            tx_type: "swap".to_string(),
            address: "test".to_string(),
            owner: "test_wallet".to_string(),
            volume_usd: 535.5498830590299,
        };
        
        let result = parser.parse_single_transaction(&transaction).await.unwrap();
        
        // Should create BUY event for BONK (positive change)
        assert_eq!(result.buy_event.event_type, NewEventType::Buy);
        assert_eq!(result.buy_event.token_symbol, "Bonk");
        assert_eq!(result.buy_event.quantity, Decimal::try_from(31883370.79991).unwrap());
        
        // Should create SELL event for SOL (negative change, using absolute value)
        assert_eq!(result.sell_event.event_type, NewEventType::Sell);
        assert_eq!(result.sell_event.token_symbol, "SOL");
        assert_eq!(result.sell_event.quantity, Decimal::try_from(3.54841245).unwrap());
        
        // USD values should be calculated correctly using absolute quantities
        let expected_bonk_value = Decimal::try_from(31883370.79991).unwrap() 
            * Decimal::try_from(1.6796824680689412e-05).unwrap();
        let expected_sol_value = Decimal::try_from(3.54841245).unwrap() 
            * Decimal::try_from(150.92661594596476).unwrap();
        
        assert!((result.buy_event.usd_value - expected_bonk_value).abs() < Decimal::try_from(0.01).unwrap());
        assert!((result.sell_event.usd_value - expected_sol_value).abs() < Decimal::try_from(0.01).unwrap());
    }
    
    #[tokio::test]
    async fn test_group_events_by_token() {
        let events = vec![
            NewFinancialEvent {
                wallet_address: "test".to_string(),
                token_address: "token1".to_string(),
                token_symbol: "T1".to_string(),
                event_type: NewEventType::Buy,
                quantity: Decimal::from(100),
                usd_price_per_token: Decimal::from(1),
                usd_value: Decimal::from(100),
                timestamp: DateTime::from_timestamp(1000, 0).unwrap(),
                transaction_hash: "tx1".to_string(),
            },
            NewFinancialEvent {
                wallet_address: "test".to_string(),
                token_address: "token1".to_string(),
                token_symbol: "T1".to_string(),
                event_type: NewEventType::Sell,
                quantity: Decimal::from(50),
                usd_price_per_token: Decimal::from(2),
                usd_value: Decimal::from(100),
                timestamp: DateTime::from_timestamp(2000, 0).unwrap(),
                transaction_hash: "tx2".to_string(),
            },
            NewFinancialEvent {
                wallet_address: "test".to_string(),
                token_address: "token2".to_string(),
                token_symbol: "T2".to_string(),
                event_type: NewEventType::Buy,
                quantity: Decimal::from(200),
                usd_price_per_token: Decimal::from(3),
                usd_value: Decimal::from(600),
                timestamp: DateTime::from_timestamp(1500, 0).unwrap(),
                transaction_hash: "tx3".to_string(),
            },
        ];
        
        let grouped = NewTransactionParser::group_events_by_token(events);
        
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped["token1"].len(), 2);
        assert_eq!(grouped["token2"].len(), 1);
        
        // Events should be sorted by timestamp within each group
        assert!(grouped["token1"][0].timestamp < grouped["token1"][1].timestamp);
    }
}