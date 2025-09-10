use crate::{ParsedGoldRushTransaction, TokenChange, TokenChangeType, TokenTransfer, TransactionType};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::str::FromStr;

/// Unified financial event for P&L calculation
/// Matches the NewFinancialEvent structure from pnl_core
#[derive(Debug, Clone)]
pub struct UnifiedFinancialEvent {
    /// Wallet address that performed the transaction
    pub wallet_address: String,
    
    /// Token address (mint for Solana, contract address for EVM)
    pub token_address: String,
    
    /// Token symbol
    pub token_symbol: String,
    
    /// Event type (BUY or SELL)
    pub event_type: UnifiedEventType,
    
    /// Token quantity (always positive)
    pub quantity: Decimal,
    
    /// USD price per token at transaction time
    pub usd_price_per_token: Decimal,
    
    /// USD value (quantity × price)
    pub usd_value: Decimal,
    
    /// Transaction timestamp
    pub timestamp: DateTime<Utc>,
    
    /// Transaction hash
    pub transaction_hash: String,
    
    /// Blockchain identifier (e.g., "ethereum", "base", "bsc")
    pub chain: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnifiedEventType {
    Buy,
    Sell,
}

/// Converter for GoldRush transactions to unified financial events
pub struct GoldRushEventConverter {
    wallet_address: String,
    chain: String,
}

impl GoldRushEventConverter {
    pub fn new(wallet_address: String, chain: String) -> Self {
        Self {
            wallet_address,
            chain,
        }
    }

    /// Convert parsed GoldRush transactions to unified financial events
    pub fn convert_transactions(
        &self,
        transactions: Vec<ParsedGoldRushTransaction>,
    ) -> Vec<UnifiedFinancialEvent> {
        let mut events = Vec::new();

        for tx in transactions {
            if let Some(tx_events) = self.convert_transaction(&tx) {
                events.extend(tx_events);
            }
        }

        events
    }

    /// Convert a single transaction to financial events
    fn convert_transaction(
        &self,
        tx: &ParsedGoldRushTransaction,
    ) -> Option<Vec<UnifiedFinancialEvent>> {
        match tx.transaction_type {
            TransactionType::Swap => self.convert_swap_transaction(tx),
            TransactionType::Send => self.convert_send_transaction(tx),
            TransactionType::Receive => None, // Receives are not disposal events
            TransactionType::ContractInteraction => self.convert_contract_transaction(tx),
        }
    }

    /// Convert a DEX swap transaction (both buy and sell events)
    fn convert_swap_transaction(
        &self,
        tx: &ParsedGoldRushTransaction,
    ) -> Option<Vec<UnifiedFinancialEvent>> {
        let mut events = Vec::new();
        
        // Separate increases (buys) and decreases (sells)
        let increases: Vec<_> = tx.token_changes.iter()
            .filter(|c| c.change_type == TokenChangeType::Increase)
            .collect();
        let decreases: Vec<_> = tx.token_changes.iter()
            .filter(|c| c.change_type == TokenChangeType::Decrease)
            .collect();

        // Convert increases to buy events
        for increase in increases {
            if let Some(event) = self.create_buy_event(tx, increase) {
                events.push(event);
            }
        }

        // Convert decreases to sell events
        for decrease in decreases {
            if let Some(event) = self.create_sell_event(tx, decrease) {
                events.push(event);
            }
        }

        if events.is_empty() {
            None
        } else {
            Some(events)
        }
    }

    /// Convert a send transaction (disposal event - treat as sell)
    fn convert_send_transaction(
        &self,
        tx: &ParsedGoldRushTransaction,
    ) -> Option<Vec<UnifiedFinancialEvent>> {
        let mut events = Vec::new();

        // All token decreases in sends should be treated as sells
        let decreases: Vec<_> = tx.token_changes.iter()
            .filter(|c| c.change_type == TokenChangeType::Decrease)
            .collect();

        for decrease in decreases {
            if let Some(event) = self.create_sell_event(tx, decrease) {
                events.push(event);
            }
        }

        if events.is_empty() {
            None
        } else {
            Some(events)
        }
    }

    /// Convert a contract interaction (may include token changes)
    fn convert_contract_transaction(
        &self,
        tx: &ParsedGoldRushTransaction,
    ) -> Option<Vec<UnifiedFinancialEvent>> {
        // Treat similar to swaps - any token changes are relevant
        self.convert_swap_transaction(tx)
    }

    /// Create a buy event from token increase
    fn create_buy_event(
        &self,
        tx: &ParsedGoldRushTransaction,
        token_change: &TokenChange,
    ) -> Option<UnifiedFinancialEvent> {
        if token_change.amount_formatted <= Decimal::ZERO {
            return None;
        }

        let usd_price = if let Some(usd_value) = token_change.usd_value {
            if token_change.amount_formatted > Decimal::ZERO {
                usd_value / token_change.amount_formatted
            } else {
                Decimal::ZERO
            }
        } else {
            Decimal::ZERO
        };

        let usd_value = token_change.usd_value.unwrap_or(Decimal::ZERO);

        Some(UnifiedFinancialEvent {
            wallet_address: self.wallet_address.clone(),
            token_address: token_change.token_address.clone(),
            token_symbol: token_change.token_symbol.clone(),
            event_type: UnifiedEventType::Buy,
            quantity: token_change.amount_formatted,
            usd_price_per_token: usd_price,
            usd_value,
            timestamp: tx.block_time,
            transaction_hash: tx.tx_hash.clone(),
            chain: self.chain.clone(),
        })
    }

    /// Create a sell event from token decrease
    fn create_sell_event(
        &self,
        tx: &ParsedGoldRushTransaction,
        token_change: &TokenChange,
    ) -> Option<UnifiedFinancialEvent> {
        if token_change.amount_formatted <= Decimal::ZERO {
            return None;
        }

        let usd_price = if let Some(usd_value) = token_change.usd_value {
            if token_change.amount_formatted > Decimal::ZERO {
                usd_value / token_change.amount_formatted
            } else {
                Decimal::ZERO
            }
        } else {
            Decimal::ZERO
        };

        let usd_value = token_change.usd_value.unwrap_or(Decimal::ZERO);

        Some(UnifiedFinancialEvent {
            wallet_address: self.wallet_address.clone(),
            token_address: token_change.token_address.clone(),
            token_symbol: token_change.token_symbol.clone(),
            event_type: UnifiedEventType::Sell,
            quantity: token_change.amount_formatted,
            usd_price_per_token: usd_price,
            usd_value,
            timestamp: tx.block_time,
            transaction_hash: tx.tx_hash.clone(),
            chain: self.chain.clone(),
        })
    }

    /// Convert GoldRush TokenTransfer objects directly to unified financial events
    /// This bypasses transaction parsing and uses the transfers_v2 API data
    pub fn convert_token_transfers(
        &self,
        transfers: Vec<TokenTransfer>,
    ) -> Vec<UnifiedFinancialEvent> {
        use tracing::info;
        
        info!("🔄 Converting {} transfers to financial events", transfers.len());
        let start_time = std::time::Instant::now();
        
        let mut events = Vec::new();
        let mut skipped_transfers = 0;
        let mut in_transfers = 0;
        let mut out_transfers = 0;
        let mut usd_value_transfers = 0;

        for (i, transfer) in transfers.iter().enumerate() {
            // Track transfer types
            match transfer.transfer_type.as_str() {
                "IN" => in_transfers += 1,
                "OUT" => out_transfers += 1,
                _ => {
                    skipped_transfers += 1;
                    continue;
                }
            }
            
            if transfer.delta_quote.is_some() {
                usd_value_transfers += 1;
            }
            
            if let Some(event) = self.create_event_from_transfer(transfer) {
                events.push(event);
            } else {
                skipped_transfers += 1;
            }
            
            // Log progress for large datasets
            if transfers.len() > 100 && i % 50 == 0 {
                info!("  📊 Progress: {}/{} transfers processed", i, transfers.len());
            }
        }

        let elapsed = start_time.elapsed();
        info!("✅ Conversion complete in {:.3}s:", elapsed.as_secs_f64());
        info!("  📈 {} IN transfers (BUY events)", in_transfers);
        info!("  📉 {} OUT transfers (SELL events)", out_transfers);
        info!("  💵 {} transfers have USD values", usd_value_transfers);
        info!("  ✅ {} events created, {} skipped", events.len(), skipped_transfers);
        
        events
    }

    /// Create a financial event from a token transfer
    fn create_event_from_transfer(
        &self,
        transfer: &TokenTransfer,
    ) -> Option<UnifiedFinancialEvent> {
        // Determine if this is a buy (IN) or sell (OUT)
        let event_type = match transfer.transfer_type.as_str() {
            "IN" => UnifiedEventType::Buy,
            "OUT" => UnifiedEventType::Sell, // Sends treated as sells
            _ => {
                // Unknown transfer type, skip
                return None;
            }
        };

        // Parse the delta amount
        let delta_str = transfer.delta.as_ref()?;
        let quantity = Decimal::from_str(delta_str).ok()?;
        if quantity <= Decimal::ZERO {
            return None;
        }

        // Format quantity using contract decimals (default to 18 if not available)
        let decimals = transfer.contract_decimals.unwrap_or(18);
        let formatted_quantity = quantity / Decimal::from(10_u64.pow(decimals));

        // Get USD price and value
        let usd_price_per_token = if let Some(quote_rate) = transfer.quote_rate {
            Decimal::from_f64_retain(quote_rate).unwrap_or_default()
        } else {
            Decimal::ZERO
        };

        let usd_value = if let Some(delta_quote) = transfer.delta_quote {
            Decimal::from_f64_retain(delta_quote.abs()).unwrap_or_default()
        } else {
            // Fallback calculation if delta_quote not available
            formatted_quantity * usd_price_per_token
        };

        Some(UnifiedFinancialEvent {
            wallet_address: self.wallet_address.clone(),
            token_address: transfer.contract_address.clone(),
            token_symbol: transfer.contract_ticker_symbol.as_ref()?.clone(),
            event_type,
            quantity: formatted_quantity,
            usd_price_per_token,
            usd_value,
            timestamp: transfer.block_signed_at,
            transaction_hash: transfer.tx_hash.clone(),
            chain: self.chain.clone(),
        })
    }
}

/// Convert UnifiedFinancialEvent to pnl_core's NewFinancialEvent
impl From<UnifiedFinancialEvent> for pnl_core::NewFinancialEvent {
    fn from(event: UnifiedFinancialEvent) -> Self {
        pnl_core::NewFinancialEvent {
            wallet_address: event.wallet_address,
            token_address: event.token_address,
            token_symbol: event.token_symbol,
            event_type: match event.event_type {
                UnifiedEventType::Buy => pnl_core::NewEventType::Buy,
                UnifiedEventType::Sell => pnl_core::NewEventType::Sell,
            },
            quantity: event.quantity,
            usd_price_per_token: event.usd_price_per_token,
            usd_value: event.usd_value,
            timestamp: event.timestamp,
            transaction_hash: event.transaction_hash,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_test_token_change(
        address: &str,
        symbol: &str,
        amount: Decimal,
        usd_value: Option<Decimal>,
        change_type: TokenChangeType,
    ) -> TokenChange {
        TokenChange {
            token_address: address.to_string(),
            token_symbol: symbol.to_string(),
            token_decimals: 18,
            amount_raw: amount.to_string(),
            amount_formatted: amount,
            usd_value,
            change_type,
        }
    }

    #[test]
    fn test_swap_conversion() {
        let converter = GoldRushEventConverter::new(
            "0x1234567890abcdef1234567890abcdef12345678".to_string(),
            "ethereum".to_string(),
        );

        let tx = ParsedGoldRushTransaction {
            tx_hash: "0xabcd".to_string(),
            block_time: Utc::now(),
            from_address: "0x1234567890abcdef1234567890abcdef12345678".to_string(),
            to_address: Some("0xdex_contract".to_string()),
            transaction_type: TransactionType::Swap,
            token_changes: vec![
                // Bought USDC
                create_test_token_change(
                    "0xusdc",
                    "USDC",
                    Decimal::from(100),
                    Some(Decimal::from(100)),
                    TokenChangeType::Increase,
                ),
                // Sold ETH
                create_test_token_change(
                    "0x0000000000000000000000000000000000000000",
                    "ETH",
                    Decimal::from_str("0.05").unwrap(),
                    Some(Decimal::from(100)),
                    TokenChangeType::Decrease,
                ),
            ],
            gas_fee_usd: Some(Decimal::from(5)),
        };

        let events = converter.convert_transaction(&tx).unwrap();
        assert_eq!(events.len(), 2);

        // Check buy event
        let buy_event = events.iter().find(|e| e.event_type == UnifiedEventType::Buy).unwrap();
        assert_eq!(buy_event.token_symbol, "USDC");
        assert_eq!(buy_event.quantity, Decimal::from(100));
        assert_eq!(buy_event.usd_price_per_token, Decimal::from(1));

        // Check sell event
        let sell_event = events.iter().find(|e| e.event_type == UnifiedEventType::Sell).unwrap();
        assert_eq!(sell_event.token_symbol, "ETH");
        assert_eq!(sell_event.quantity, Decimal::from_str("0.05").unwrap());
    }

    #[test]
    fn test_send_conversion() {
        let converter = GoldRushEventConverter::new(
            "0x1234567890abcdef1234567890abcdef12345678".to_string(),
            "ethereum".to_string(),
        );

        let tx = ParsedGoldRushTransaction {
            tx_hash: "0xabcd".to_string(),
            block_time: Utc::now(),
            from_address: "0x1234567890abcdef1234567890abcdef12345678".to_string(),
            to_address: Some("0xrecipient".to_string()),
            transaction_type: TransactionType::Send,
            token_changes: vec![
                // Sent USDC (should be treated as sell)
                create_test_token_change(
                    "0xusdc",
                    "USDC",
                    Decimal::from(50),
                    Some(Decimal::from(50)),
                    TokenChangeType::Decrease,
                ),
            ],
            gas_fee_usd: Some(Decimal::from(2)),
        };

        let events = converter.convert_transaction(&tx).unwrap();
        assert_eq!(events.len(), 1);

        let sell_event = &events[0];
        assert_eq!(sell_event.event_type, UnifiedEventType::Sell);
        assert_eq!(sell_event.token_symbol, "USDC");
        assert_eq!(sell_event.quantity, Decimal::from(50));
        assert_eq!(sell_event.usd_value, Decimal::from(50));
    }
}