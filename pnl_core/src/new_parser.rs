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
    chain_id: &'a str,
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
    /// Received tokens (airdrops, transfers from other wallets) - no cost basis
    Receive,
}

/// Standardized financial event created from transaction data
/// Following the new algorithm specification exactly
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewFinancialEvent {
    /// Wallet address that performed the transaction
    pub wallet_address: String,

    /// Token address (mint)
    pub token_address: String,

    /// Token symbol
    pub token_symbol: String,

    /// Chain ID (e.g., "solana", "ethereum", "binance-smart-chain", "base")
    /// Defaults to "solana" for backward compatibility with old data
    #[serde(default = "default_chain_id")]
    pub chain_id: String,

    /// Event type (BUY or SELL)
    pub event_type: NewEventType,

    /// Token quantity (always positive, using absolute value)
    pub quantity: Decimal,

    /// USD price per token at transaction time
    pub usd_price_per_token: Decimal,

    /// USD value (quantity × price)
    pub usd_value: Decimal,

    /// For multi-hop swap BUY events: the token that was actually spent
    /// Example: USDUC → SOL → INFINITY, this would be "USDUC" for the INFINITY BUY event
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap_input_token: Option<String>,

    /// For multi-hop swap BUY events: quantity of the input token spent
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap_input_quantity: Option<Decimal>,

    /// For multi-hop swap BUY events: USD value of what was actually spent
    /// This is the TRUE invested amount (what user paid), not the market value of tokens received
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap_input_usd_value: Option<Decimal>,

    /// Transaction timestamp
    pub timestamp: DateTime<Utc>,

    /// Transaction hash
    pub transaction_hash: String,
}

/// Default chain_id for backward compatibility with old data
fn default_chain_id() -> String {
    "solana".to_string()
}

/// Result of parsing a single transaction
#[derive(Debug, Clone)]
pub struct ParsedTransaction {
    /// The buy event from this transaction
    pub buy_event: NewFinancialEvent,

    /// The sell event from this transaction
    pub sell_event: NewFinancialEvent,
}

/// Data Preparation & Parsing Module
/// Processes raw transaction data into standardized financial events
/// Designed for parallel processing of multiple wallets
pub struct NewTransactionParser {
    wallet_address: String,
}

impl NewTransactionParser {
    /// Create a new parser for a specific wallet
    pub fn new(wallet_address: String) -> Self {
        Self { wallet_address }
    }

    /// Validates price against nearest_price and returns the safer option
    /// Uses 5% deviation threshold to detect corrupted price data
    fn validate_and_extract_price(
        &self,
        main_price: Option<f64>,
        nearest_price: Option<f64>,
        token_symbol: &str,
        side: &str, // "quote" or "base" for logging
    ) -> Result<Decimal, String> {
        const PRICE_DEVIATION_THRESHOLD: f64 = 1.25; // 25% threshold

        let main = main_price.unwrap_or(0.0);
        let nearest = nearest_price.unwrap_or(0.0);

        // Both prices available - validate deviation
        if main > 0.0 && nearest > 0.0 {
            let ratio = main.max(nearest) / main.min(nearest);
            if ratio > PRICE_DEVIATION_THRESHOLD {
                warn!(
                    "Price deviation detected for {} {}: main=${:.6}, nearest=${:.6} ({}x), using nearest_price",
                    token_symbol, side, main, nearest, ratio
                );
                return Decimal::try_from(nearest)
                    .map_err(|e| format!("Invalid nearest_price conversion: {}", e).into());
            }
            return Decimal::try_from(main)
                .map_err(|e| format!("Invalid main_price conversion: {}", e).into());
        }

        // Fallback to nearest_price if available
        if nearest > 0.0 {
            debug!(
                "Using nearest_price for {} {}: ${:.6}",
                token_symbol, side, nearest
            );
            return Ok(Decimal::try_from(nearest)
                .map_err(|e| format!("Invalid nearest_price fallback conversion: {}", e))?);
        }

        // Last resort: use main price if available
        if main > 0.0 {
            return Ok(Decimal::try_from(main)
                .map_err(|e| format!("Invalid main_price fallback conversion: {}", e))?);
        }

        Ok(Decimal::ZERO)
    }

    /// Core algorithm: Parse transactions into financial events
    ///
    /// For every single transaction:
    /// - Examine both `quote` and `base` sides
    /// - Check the `ui_change_amount` sign for each side
    /// - Negative amount → SELL event (token disposed of)
    /// - Positive amount → BUY event (token acquired)
    /// - Always create exactly 2 events per transaction (one buy, one sell)
    /// - Use embedded price data for USD value calculation
    /// - Use absolute values for quantities to ensure mathematical consistency
    /// - Skip transactions with unrealistic quantities to prevent data errors
    pub async fn parse_transactions(
        &self,
        transactions: Vec<GeneralTraderTransaction>,
    ) -> Result<Vec<NewFinancialEvent>, String> {
        let mut all_events = Vec::new();

        debug!(
            "Parsing {} transactions for wallet {} using new algorithm",
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

    /// Parse a single transaction into exactly 2 financial events
    async fn parse_single_transaction(
        &self,
        transaction: &GeneralTraderTransaction,
    ) -> Result<ParsedTransaction, String> {
        let timestamp = DateTime::from_timestamp(transaction.block_unix_time, 0)
            .ok_or_else(|| format!("Invalid timestamp: {}", transaction.block_unix_time))?;

        // Examine quote side
        let quote_change = Decimal::try_from(transaction.quote.ui_change_amount)
            .map_err(|e| format!("Invalid quote ui_change_amount: {}", e))?;

        // Extract quote price with validation against nearest_price
        let quote_price = self.validate_and_extract_price(
            transaction.quote.price,
            transaction.quote.nearest_price,
            &transaction.quote.symbol,
            "quote",
        )?;

        // Examine base side
        let base_change = Decimal::try_from(transaction.base.ui_change_amount)
            .map_err(|e| format!("Invalid base ui_change_amount: {}", e))?;

        // Extract base price with validation against nearest_price
        let base_price = self.validate_and_extract_price(
            transaction.base.price,
            transaction.base.nearest_price,
            &transaction.base.symbol,
            "base",
        )?;

        // Create events based on ui_change_amount signs
        let (buy_event, sell_event) = if quote_change < Decimal::ZERO && base_change > Decimal::ZERO
        {
            // Quote is negative (SELL), Base is positive (BUY)
            let sell_event = self.create_financial_event(&EventCreationParams {
                token_address: &transaction.quote.address,
                token_symbol: &transaction.quote.symbol,
                chain_id: "solana", // BirdEye parser is Solana-specific
                event_type: NewEventType::Sell,
                quantity: quote_change.abs(), // Use absolute value for quantity
                price_per_token: quote_price,
                timestamp,
                transaction_hash: &transaction.tx_hash,
            })?;

            let buy_event = self.create_financial_event(&EventCreationParams {
                token_address: &transaction.base.address,
                token_symbol: &transaction.base.symbol,
                chain_id: "solana", // BirdEye parser is Solana-specific
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
                chain_id: "solana", // BirdEye parser is Solana-specific
                event_type: NewEventType::Buy,
                quantity: quote_change.abs(), // Use absolute value for quantity
                price_per_token: quote_price,
                timestamp,
                transaction_hash: &transaction.tx_hash,
            })?;

            let sell_event = self.create_financial_event(&EventCreationParams {
                token_address: &transaction.base.address,
                token_symbol: &transaction.base.symbol,
                chain_id: "solana", // BirdEye parser is Solana-specific
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
            chain_id: params.chain_id.to_string(),
            event_type: params.event_type.clone(),
            quantity: params.quantity,
            usd_price_per_token: params.price_per_token,
            usd_value,
            swap_input_token: None,
            swap_input_quantity: None,
            swap_input_usd_value: None,
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

