use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::collections::HashMap;
use tracing::{debug, trace, warn};

use crate::new_parser::{NewFinancialEvent, NewEventType};

/// Transaction parser for Birdeye transaction history API format
/// Handles both swap and send transactions using balanceChange data
pub struct HistoryTransactionParser {
    wallet_address: String,
}

/// Result of parsing a wallet transaction from history API
#[derive(Debug, Clone)]
pub struct ParsedHistoryTransaction {
    /// All financial events generated from this transaction
    pub events: Vec<NewFinancialEvent>,
    /// Transaction type (swap, send, received)
    pub transaction_type: String,
    /// Whether price enrichment is required
    pub needs_price_enrichment: bool,
}

/// Balance change with enriched price data for parsing
#[derive(Debug, Clone)]
pub struct PricedBalanceChange {
    pub amount_ui: Decimal,
    pub token_address: String,
    pub token_symbol: String,
    pub price_per_token: Decimal,
    pub is_positive: bool,
}

impl HistoryTransactionParser {
    /// Create a new history transaction parser for a specific wallet
    pub fn new(wallet_address: String) -> Self {
        Self { wallet_address }
    }

    /// Parse enriched transactions from history API into financial events
    /// 
    /// Algorithm:
    /// - For SWAP transactions: Create buy/sell pairs based on balance changes
    /// - For SEND transactions: Create sell events only (disposal of assets)
    /// - For RECEIVED transactions: Skip (no P&L impact)
    pub async fn parse_enriched_transactions<T>(
        &self,
        enriched_transactions: Vec<T>,
    ) -> Result<Vec<NewFinancialEvent>, String>
    where
        T: HistoryTransaction,
    {
        let mut all_events = Vec::new();
        
        debug!(
            "Parsing {} enriched history transactions for wallet {} using new history algorithm",
            enriched_transactions.len(),
            self.wallet_address
        );
        
        for (tx_index, transaction) in enriched_transactions.iter().enumerate() {
            trace!(
                "Processing history transaction {}/{}: {} (type: {})",
                tx_index + 1,
                enriched_transactions.len(),
                transaction.get_tx_hash(),
                transaction.get_main_action()
            );
            
            match self.parse_single_history_transaction(transaction).await {
                Ok(parsed) => {
                    debug!(
                        "Successfully parsed {} transaction {} into {} events",
                        parsed.transaction_type,
                        transaction.get_tx_hash(),
                        parsed.events.len()
                    );
                    
                    all_events.extend(parsed.events);
                }
                Err(e) => {
                    warn!(
                        "Failed to parse history transaction {}: {}. Skipping.",
                        transaction.get_tx_hash(), e
                    );
                    continue;
                }
            }
        }
        
        debug!(
            "Successfully parsed {} history transactions into {} financial events for wallet {}",
            enriched_transactions.len(),
            all_events.len(),
            self.wallet_address
        );
        
        Ok(all_events)
    }

    /// Parse a single enriched history transaction into financial events
    async fn parse_single_history_transaction<T>(
        &self,
        transaction: &T,
    ) -> Result<ParsedHistoryTransaction, String>
    where
        T: HistoryTransaction,
    {
        let main_action = transaction.get_main_action().to_lowercase();
        let tx_hash = transaction.get_tx_hash();
        
        // Parse transaction timestamp
        let timestamp = self.parse_timestamp(transaction.get_block_time())?;
        
        // Get enriched balance changes with prices
        let balance_changes = transaction.get_enriched_balance_changes();
        
        // Filter out balance changes without resolved prices (we'll skip these)
        let priced_changes: Vec<PricedBalanceChange> = balance_changes
            .into_iter()
            .filter_map(|change| {
                if change.price_resolved {
                    Some(PricedBalanceChange {
                        amount_ui: self.calculate_ui_amount(change.amount, change.decimals),
                        token_address: change.address.clone(),
                        token_symbol: change.symbol.clone(),
                        price_per_token: Decimal::try_from(change.price_per_token.unwrap_or(0.0)).ok()?,
                        is_positive: change.amount > 0,
                    })
                } else {
                    debug!("Skipping balance change for {} - price not resolved", change.symbol);
                    None
                }
            })
            .collect();
        
        if priced_changes.is_empty() {
            return Err("No balance changes with resolved prices".to_string());
        }
        
        let events = match main_action.as_str() {
            "swap" => self.parse_swap_transaction(&priced_changes, &timestamp, tx_hash)?,
            "send" => self.parse_send_transaction(&priced_changes, &timestamp, tx_hash)?,
            "received" => {
                debug!("Skipping 'received' transaction {} - no P&L impact", tx_hash);
                Vec::new()
            }
            _ => {
                warn!("Unknown transaction type '{}' for tx {}, treating as swap", main_action, tx_hash);
                self.parse_swap_transaction(&priced_changes, &timestamp, tx_hash)?
            }
        };
        
        Ok(ParsedHistoryTransaction {
            events,
            transaction_type: main_action,
            needs_price_enrichment: false, // Already enriched
        })
    }

    /// Parse a swap transaction - create buy/sell event pairs
    fn parse_swap_transaction(
        &self,
        priced_changes: &[PricedBalanceChange],
        timestamp: &DateTime<Utc>,
        tx_hash: &str,
    ) -> Result<Vec<NewFinancialEvent>, String> {
        let mut events = Vec::new();
        
        // For swaps, expect both positive and negative balance changes
        let positive_changes: Vec<&PricedBalanceChange> = priced_changes.iter()
            .filter(|change| change.is_positive)
            .collect();
        let negative_changes: Vec<&PricedBalanceChange> = priced_changes.iter()
            .filter(|change| !change.is_positive)
            .collect();
        
        if positive_changes.is_empty() || negative_changes.is_empty() {
            return Err(format!(
                "Invalid swap transaction: expected both positive and negative changes, got {} positive, {} negative",
                positive_changes.len(),
                negative_changes.len()
            ));
        }
        
        // Create buy events for positive changes (tokens received)
        for change in &positive_changes {
            let buy_event = self.create_financial_event(
                &change.token_address,
                &change.token_symbol,
                NewEventType::Buy,
                change.amount_ui.abs(),
                change.price_per_token,
                *timestamp,
                tx_hash,
            )?;
            events.push(buy_event);
        }
        
        // Create sell events for negative changes (tokens disposed)
        for change in &negative_changes {
            let sell_event = self.create_financial_event(
                &change.token_address,
                &change.token_symbol,
                NewEventType::Sell,
                change.amount_ui.abs(),
                change.price_per_token,
                *timestamp,
                tx_hash,
            )?;
            events.push(sell_event);
        }
        
        debug!(
            "Parsed swap transaction {}: {} buys, {} sells",
            tx_hash,
            positive_changes.len(),
            negative_changes.len()
        );
        
        Ok(events)
    }

    /// Parse a send transaction - create sell events only (disposal of assets)
    fn parse_send_transaction(
        &self,
        priced_changes: &[PricedBalanceChange],
        timestamp: &DateTime<Utc>,
        tx_hash: &str,
    ) -> Result<Vec<NewFinancialEvent>, String> {
        let mut events = Vec::new();
        
        // For sends, we only care about negative balance changes (tokens leaving wallet)
        let outgoing_changes: Vec<&PricedBalanceChange> = priced_changes.iter()
            .filter(|change| !change.is_positive && change.amount_ui.abs() > Decimal::ZERO)
            .collect();
        
        if outgoing_changes.is_empty() {
            return Err("No outgoing tokens found in send transaction".to_string());
        }
        
        // Create sell events for all outgoing tokens
        for change in &outgoing_changes {
            let sell_event = self.create_financial_event(
                &change.token_address,
                &change.token_symbol,
                NewEventType::Sell,
                change.amount_ui.abs(),
                change.price_per_token,
                *timestamp,
                tx_hash,
            )?;
            events.push(sell_event);
        }
        
        debug!(
            "Parsed send transaction {}: {} tokens disposed",
            tx_hash,
            outgoing_changes.len()
        );
        
        Ok(events)
    }

    /// Create a standardized financial event
    fn create_financial_event(
        &self,
        token_address: &str,
        token_symbol: &str,
        event_type: NewEventType,
        quantity: Decimal,
        price_per_token: Decimal,
        timestamp: DateTime<Utc>,
        transaction_hash: &str,
    ) -> Result<NewFinancialEvent, String> {
        // Calculate USD value
        let usd_value = quantity * price_per_token;
        
        // Validation
        if quantity <= Decimal::ZERO {
            return Err(format!(
                "Invalid quantity: {} (must be positive)",
                quantity
            ));
        }
        
        if price_per_token < Decimal::ZERO {
            return Err(format!(
                "Invalid price: {} (must be non-negative)",
                price_per_token
            ));
        }
        
        Ok(NewFinancialEvent {
            wallet_address: self.wallet_address.clone(),
            token_address: token_address.to_string(),
            token_symbol: token_symbol.to_string(),
            event_type,
            quantity,
            usd_price_per_token: price_per_token,
            usd_value,
            timestamp,
            transaction_hash: transaction_hash.to_string(),
        })
    }

    /// Parse transaction timestamp from block_time string
    fn parse_timestamp(&self, block_time: &str) -> Result<DateTime<Utc>, String> {
        // Try parsing as Unix timestamp first
        if let Ok(unix_timestamp) = block_time.parse::<i64>() {
            return DateTime::from_timestamp(unix_timestamp, 0)
                .ok_or_else(|| format!("Invalid unix timestamp: {}", unix_timestamp));
        }
        
        // Try parsing as ISO 8601 format
        if let Ok(datetime) = DateTime::parse_from_rfc3339(block_time) {
            return Ok(datetime.with_timezone(&Utc));
        }
        
        Err(format!("Unable to parse block_time: {}", block_time))
    }

    /// Calculate UI amount from raw amount and decimals
    fn calculate_ui_amount(&self, raw_amount: i128, decimals: u32) -> Decimal {
        let raw_decimal = Decimal::try_from(raw_amount).unwrap_or(Decimal::ZERO);
        let divisor = Decimal::from(10_u64.pow(decimals));
        raw_decimal / divisor
    }

    /// Group financial events by token address for P&L processing
    /// This matches the interface from the original parser
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

/// Trait to abstract over different enriched transaction types
/// This allows the parser to work with any enriched transaction format
pub trait HistoryTransaction {
    /// Get the transaction hash
    fn get_tx_hash(&self) -> &str;
    
    /// Get the main action type (swap, send, received)
    fn get_main_action(&self) -> &str;
    
    /// Get the block time
    fn get_block_time(&self) -> &str;
    
    /// Get enriched balance changes with prices
    fn get_enriched_balance_changes(&self) -> Vec<HistoryBalanceChange>;
}

/// Abstracted balance change for the parser
#[derive(Debug, Clone)]
pub struct HistoryBalanceChange {
    pub amount: i128,
    pub symbol: String,
    pub address: String,
    pub decimals: u32,
    pub price_per_token: Option<f64>,
    pub price_resolved: bool,
}

// Note: Implementation for EnrichedTransaction will be provided in the dex_client crate
// to avoid circular dependencies. The trait is defined here for use across crates.

#[cfg(test)]
mod tests {
    use super::*;
    
    // Mock implementation for testing
    struct MockEnrichedTransaction {
        tx_hash: String,
        main_action: String,
        block_time: String,
        balance_changes: Vec<HistoryBalanceChange>,
    }
    
    impl HistoryTransaction for MockEnrichedTransaction {
        fn get_tx_hash(&self) -> &str {
            &self.tx_hash
        }
        
        fn get_main_action(&self) -> &str {
            &self.main_action
        }
        
        fn get_block_time(&self) -> &str {
            &self.block_time
        }
        
        fn get_enriched_balance_changes(&self) -> Vec<HistoryBalanceChange> {
            self.balance_changes.clone()
        }
    }
    
    // Helper function to create a mock balance change
    fn create_mock_balance_change(
        amount: i128,
        symbol: &str,
        address: &str,
        decimals: u32,
        price: Option<f64>,
        resolved: bool,
    ) -> HistoryBalanceChange {
        HistoryBalanceChange {
            amount,
            symbol: symbol.to_string(),
            address: address.to_string(),
            decimals,
            price_per_token: price,
            price_resolved: resolved,
        }
    }

    #[tokio::test]
    async fn test_parse_swap_transaction() {
        let parser = HistoryTransactionParser::new("test_wallet".to_string());
        
        // Create a mock swap: SOL -> BONK
        let transaction = MockEnrichedTransaction {
            tx_hash: "test_tx_hash".to_string(),
            main_action: "swap".to_string(),
            block_time: "1640995200".to_string(),
            balance_changes: vec![
                create_mock_balance_change(-1000000000, "SOL", "So11111111111111111111111111111111111111112", 9, Some(150.0), true), // -1 SOL
                create_mock_balance_change(100000000, "BONK", "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263", 5, Some(0.00002), true), // +1000 BONK
            ],
        };
        
        let result = parser.parse_single_history_transaction(&transaction).await.unwrap();
        
        assert_eq!(result.transaction_type, "swap");
        assert_eq!(result.events.len(), 2);
        
        // Check for buy and sell events
        let buy_events: Vec<_> = result.events.iter().filter(|e| e.event_type == NewEventType::Buy).collect();
        let sell_events: Vec<_> = result.events.iter().filter(|e| e.event_type == NewEventType::Sell).collect();
        
        assert_eq!(buy_events.len(), 1);
        assert_eq!(sell_events.len(), 1);
        
        // Verify BONK buy event
        let bonk_buy = &buy_events[0];
        assert_eq!(bonk_buy.token_symbol, "BONK");
        assert_eq!(bonk_buy.quantity, Decimal::try_from(1000.0).unwrap()); // 100000000 / 10^5
        
        // Verify SOL sell event  
        let sol_sell = &sell_events[0];
        assert_eq!(sol_sell.token_symbol, "SOL");
        assert_eq!(sol_sell.quantity, Decimal::try_from(1.0).unwrap()); // 1000000000 / 10^9
    }
    
    #[tokio::test]
    async fn test_parse_send_transaction() {
        let parser = HistoryTransactionParser::new("test_wallet".to_string());
        
        // Create a mock send transaction: sending USDC
        let transaction = MockEnrichedTransaction {
            tx_hash: "test_send_hash".to_string(),
            main_action: "send".to_string(),
            block_time: "1640995200".to_string(),
            balance_changes: vec![
                create_mock_balance_change(-1000000, "USDC", "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", 6, Some(1.0), true), // -1 USDC
            ],
        };
        
        let result = parser.parse_single_history_transaction(&transaction).await.unwrap();
        
        assert_eq!(result.transaction_type, "send");
        assert_eq!(result.events.len(), 1);
        
        // Should only create a sell event (disposal)
        let event = &result.events[0];
        assert_eq!(event.event_type, NewEventType::Sell);
        assert_eq!(event.token_symbol, "USDC");
        assert_eq!(event.quantity, Decimal::try_from(1.0).unwrap()); // 1000000 / 10^6
    }
    
    #[tokio::test]
    async fn test_skip_received_transaction() {
        let parser = HistoryTransactionParser::new("test_wallet".to_string());
        
        // Create a mock received transaction
        let transaction = MockEnrichedTransaction {
            tx_hash: "test_received_hash".to_string(),
            main_action: "received".to_string(),
            block_time: "1640995200".to_string(),
            balance_changes: vec![
                create_mock_balance_change(1000000, "USDC", "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", 6, Some(1.0), true), // +1 USDC received
            ],
        };
        
        let result = parser.parse_single_history_transaction(&transaction).await.unwrap();
        
        assert_eq!(result.transaction_type, "received");
        assert_eq!(result.events.len(), 0); // Should skip received transactions
    }

    #[test]
    fn test_calculate_ui_amount() {
        let parser = HistoryTransactionParser::new("test_wallet".to_string());
        
        // Test USDC (6 decimals)
        let ui_amount = parser.calculate_ui_amount(1000000, 6);
        assert_eq!(ui_amount, Decimal::from(1));
        
        // Test SOL (9 decimals)
        let ui_amount = parser.calculate_ui_amount(1000000000, 9);
        assert_eq!(ui_amount, Decimal::from(1));
        
        // Test negative amounts
        let ui_amount = parser.calculate_ui_amount(-1000000, 6);
        assert_eq!(ui_amount, Decimal::from(-1));
    }
    
    #[test]
    fn test_parse_timestamp() {
        let parser = HistoryTransactionParser::new("test_wallet".to_string());
        
        // Test Unix timestamp
        let result = parser.parse_timestamp("1640995200");
        assert!(result.is_ok());
        
        // Test ISO 8601
        let result = parser.parse_timestamp("2022-01-01T00:00:00Z");
        assert!(result.is_ok());
        
        // Test invalid format
        let result = parser.parse_timestamp("invalid");
        assert!(result.is_err());
    }
}