pub mod timeframe;
pub mod trader_filter;
pub mod new_parser;
pub mod new_pnl_engine;
pub mod comprehensive_tests;

// Re-export key trader filtering types
pub use trader_filter::{TraderFilter, TraderQuality, RiskLevel, TradingStyle, generate_trader_summary};
// New algorithm exports (primary P&L system)
pub use new_parser::{NewTransactionParser, NewFinancialEvent, NewEventType, ParsedTransaction};
pub use new_pnl_engine::{NewPnLEngine, TokenPnLResult, PortfolioPnLResult, MatchedTrade, UnmatchedSell, RemainingPosition};


use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use tracing::{debug, warn};
use uuid::Uuid;

/// Single transaction from general BirdEye trader API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralTraderTransaction {
    pub quote: TokenTransactionSide,
    pub base: TokenTransactionSide,
    #[serde(rename = "base_price")]
    pub base_price: Option<f64>,
    #[serde(rename = "quote_price")]
    #[serde(deserialize_with = "deserialize_nullable_f64")]
    pub quote_price: f64,
    #[serde(rename = "tx_hash")]
    pub tx_hash: String,
    pub source: String,
    #[serde(rename = "block_unix_time")]
    pub block_unix_time: i64,
    #[serde(rename = "tx_type")]
    #[serde(default = "default_tx_type")]
    pub tx_type: String, // "swap"
    #[serde(default)]
    pub address: String, // Program address
    #[serde(default)]
    pub owner: String,   // Wallet address
    #[serde(rename = "volume_usd")]
    pub volume_usd: f64,
}

/// Token side of a transaction (quote or base)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTransactionSide {
    #[serde(default = "default_symbol")]
    pub symbol: String, // Make resilient to missing symbol
    #[serde(default)]
    pub decimals: u32,
    #[serde(deserialize_with = "deserialize_nullable_string")]
    pub address: String, // Make resilient to null values
    #[serde(deserialize_with = "deserialize_amount")]
    pub amount: u128,
    #[serde(rename = "type")]
    pub transfer_type: Option<String>, // "transfer", "transferChecked", "split", "burn", "mintTo", etc.
    #[serde(rename = "type_swap")]
    #[serde(deserialize_with = "deserialize_nullable_string")]
    pub type_swap: String, // "from", "to" - Make resilient to null values
    #[serde(rename = "ui_amount")]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_optional_nullable_f64")]
    pub ui_amount: f64, // Make resilient to missing/null values
    pub price: Option<f64>,
    #[serde(rename = "nearest_price")]
    pub nearest_price: Option<f64>,
    #[serde(rename = "change_amount")]
    #[serde(deserialize_with = "deserialize_signed_amount")]
    pub change_amount: i128,
    #[serde(rename = "ui_change_amount")]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_optional_nullable_f64")]
    pub ui_change_amount: f64, // Make resilient to missing/null values
    #[serde(rename = "fee_info")]
    pub fee_info: Option<serde_json::Value>,
}

#[derive(Error, Debug)]
pub enum PnLError {
    #[error("Price fetching error: {0}")]
    PriceFetch(String),
    #[error("Invalid financial event: {0}")]
    InvalidEvent(String),
    #[error("Calculation error: {0}")]
    Calculation(String),
    #[error("Configuration error: {0}")]
    Configuration(String),
    #[error("Timeframe parsing error: {0}")]
    TimeframeParse(String),
}

pub type Result<T> = std::result::Result<T, PnLError>;

/// Simplified transaction record for processing external API data before enrichment
/// This serves as an intermediate format between raw API data and FinancialEvent
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransactionRecord {
    /// Transaction signature/hash
    pub signature: String,
    
    /// Wallet address that performed this action
    pub wallet_address: String,
    
    /// Transaction timestamp (Unix timestamp)
    pub timestamp: i64,
    
    /// DEX/source that executed the transaction
    pub source: String,
    
    /// Transaction fee in lamports
    pub fee: u64,
    
    /// Token changes for this wallet in this transaction
    pub token_changes: Vec<TokenChangeRecord>,
    
    /// SOL balance change for this wallet (in lamports)
    pub sol_balance_change: i64,
}

/// Individual token balance change within a transaction
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenChangeRecord {
    /// Token mint address
    pub mint: String,
    
    /// Raw token amount change (can be negative)
    pub raw_amount: i64,
    
    /// Human-readable amount change
    pub ui_amount: f64,
    
    /// Token decimals
    pub decimals: u8,
    
    /// Whether this is a buy (positive) or sell (negative) from wallet's perspective
    pub is_buy: bool,
}

impl TransactionRecord {
    /// Convert a TransactionRecord to FinancialEvents with price enrichment
    pub async fn to_financial_events<P: PriceFetcher>(
        &self,
        price_fetcher: &P,
    ) -> Result<Vec<FinancialEvent>> {
        let mut events = Vec::new();
        let timestamp = DateTime::from_timestamp(self.timestamp, 0)
            .unwrap_or_else(Utc::now);
        
        // Convert SOL changes to lamports for fees
        let transaction_fee = Decimal::from(self.fee) / Decimal::from(1_000_000_000); // Convert lamports to SOL
        
        // Process each token change
        for token_change in &self.token_changes {
            let token_amount = Decimal::from(token_change.raw_amount.abs()) 
                / Decimal::from(10_i64.pow(token_change.decimals as u32));
            
            // Determine event type based on the change direction
            let event_type = if token_change.is_buy {
                EventType::Buy
            } else {
                EventType::Sell
            };
            
            // Fetch historical price for this token at this timestamp
            let price_per_token = price_fetcher
                .fetch_historical_price(&token_change.mint, timestamp, Some("USD"))
                .await?
                .unwrap_or(Decimal::ZERO);
            
            // Calculate USD value
            let usd_value = token_amount * price_per_token;
            
            let sol_price = price_fetcher
                .fetch_historical_price("So11111111111111111111111111111111111111112", timestamp, Some("USD"))
                .await?
                .ok_or_else(|| PnLError::PriceFetch(
                    "Failed to fetch SOL price for token conversion".to_string()
                ))?;

            // Calculate SOL amount (for buys it's negative cost, for sells it's positive revenue)
            let sol_amount = if token_change.is_buy {
                -usd_value / sol_price
            } else {
                usd_value / sol_price
            };
            
            let event = FinancialEvent {
                id: Uuid::new_v4(),
                transaction_id: self.signature.clone(),
                wallet_address: self.wallet_address.clone(),
                event_type,
                token_mint: token_change.mint.clone(),
                token_amount,
                sol_amount,
                usd_value,
                timestamp,
                transaction_fee,
                metadata: EventMetadata {
                    program_id: None,
                    instruction_index: None,
                    exchange: Some(self.source.clone()),
                    price_per_token: Some(price_per_token),
                    extra: HashMap::new(),
                },
            };
            
            events.push(event);
        }
        
        // Add SOL balance change as a separate event if significant
        if self.sol_balance_change.abs() > 10_000_000 { // > 0.01 SOL
            let sol_amount = Decimal::from(self.sol_balance_change) / Decimal::from(1_000_000_000);
            let sol_price = price_fetcher
                .fetch_historical_price("So11111111111111111111111111111111111111112", timestamp, Some("USD"))
                .await?
                .ok_or_else(|| PnLError::PriceFetch(
                    "Failed to fetch SOL price for balance change conversion".to_string()
                ))?;
            let usd_value = sol_amount.abs() * sol_price;
            
            let event_type = if self.sol_balance_change > 0 {
                EventType::Buy // Received SOL
            } else {
                EventType::Sell // Sent SOL
            };
            
            let event = FinancialEvent {
                id: Uuid::new_v4(),
                transaction_id: self.signature.clone(),
                wallet_address: self.wallet_address.clone(),
                event_type,
                token_mint: "So11111111111111111111111111111111111111112".to_string(), // SOL mint
                token_amount: sol_amount.abs(),
                sol_amount,
                usd_value,
                timestamp,
                transaction_fee: Decimal::ZERO, // Don't double-count fees
                metadata: EventMetadata {
                    program_id: None,
                    instruction_index: None,
                    exchange: Some(self.source.clone()),
                    price_per_token: Some(sol_price),
                    extra: HashMap::new(),
                },
            };
            
            events.push(event);
        }
        
        Ok(events)
    }
}

/// Core data structure representing a financial event from a parsed transaction
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FinancialEvent {
    /// Unique identifier for the event
    pub id: Uuid,
    
    /// Transaction signature/hash
    pub transaction_id: String,
    
    /// Wallet address that performed this action
    pub wallet_address: String,
    
    /// Type of financial event
    pub event_type: EventType,
    
    /// Token mint address
    pub token_mint: String,
    
    /// Amount of tokens involved
    pub token_amount: Decimal,
    
    /// SOL amount (for fees, or if SOL is the token) - ACTUAL SOL quantities only
    pub sol_amount: Decimal,
    
    /// USD value of the transaction (calculated from token_amount × embedded_price)
    pub usd_value: Decimal,
    
    /// Timestamp of the event
    pub timestamp: DateTime<Utc>,
    
    /// Transaction fees paid in SOL
    pub transaction_fee: Decimal,
    
    /// Additional metadata
    pub metadata: EventMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EventType {
    /// Token purchase (swap SOL/other token for target token)
    Buy,
    /// Token sale (swap target token for SOL/other token) 
    Sell,
    /// Token transfer in (received tokens)
    TransferIn,
    /// Token transfer out (sent tokens)
    TransferOut,
    /// Transaction fee payment
    Fee,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[derive(Default)]
pub struct EventMetadata {
    /// Program that executed this transaction
    pub program_id: Option<String>,
    
    /// Instruction index within the transaction
    pub instruction_index: Option<u32>,
    
    /// Exchange/DEX used (if applicable)
    pub exchange: Option<String>,
    
    /// Price per token at time of transaction (if available)
    pub price_per_token: Option<Decimal>,
    
    /// Additional custom fields
    pub extra: HashMap<String, String>,
}


impl Default for FinancialEvent {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            transaction_id: String::new(),
            wallet_address: String::new(),
            event_type: EventType::Buy,
            token_mint: String::new(),
            token_amount: Decimal::ZERO,
            sol_amount: Decimal::ZERO,
            usd_value: Decimal::ZERO,
            timestamp: Utc::now(),
            transaction_fee: Decimal::ZERO,
            metadata: Default::default(),
        }
    }
}

/// P&L calculation result for a wallet
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PnLReport {
    /// Wallet address analyzed
    pub wallet_address: String,
    
    /// Analysis timeframe
    pub timeframe: AnalysisTimeframe,
    
    /// Overall P&L summary
    pub summary: PnLSummary,
    
    /// Per-token P&L breakdown
    pub token_breakdown: Vec<TokenPnL>,
    
    /// Current holdings (tokens still held)
    pub current_holdings: Vec<Holding>,
    
    /// Analysis metadata
    pub metadata: ReportMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnalysisTimeframe {
    /// Start time of analysis (None = from beginning)
    pub start_time: Option<DateTime<Utc>>,
    
    /// End time of analysis (None = until now)
    pub end_time: Option<DateTime<Utc>>,
    
    /// Timeframe mode used
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PnLSummary {
    /// Total realized profit/loss in USD
    pub realized_pnl_usd: Decimal,
    
    /// Total unrealized profit/loss in USD (from current holdings)
    pub unrealized_pnl_usd: Decimal,
    
    /// Total P&L (realized + unrealized) in USD
    pub total_pnl_usd: Decimal,
    
    /// Total fees paid in SOL
    pub total_fees_sol: Decimal,
    
    /// Total fees paid in USD equivalent
    pub total_fees_usd: Decimal,
    
    /// Number of profitable trades
    pub winning_trades: u32,
    
    /// Number of losing trades
    pub losing_trades: u32,
    
    /// Total number of trades
    pub total_trades: u32,
    
    /// Win rate percentage
    pub win_rate: Decimal,
    
    /// Average hold time in minutes
    pub avg_hold_time_minutes: Decimal,
    
    /// Total capital deployed (max SOL value at any point)
    pub total_capital_deployed_sol: Decimal,
    
    /// ROI percentage
    pub roi_percentage: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenPnL {
    /// Token mint address
    pub token_mint: String,
    
    /// Token symbol (if known)
    pub token_symbol: Option<String>,
    
    /// Realized P&L for this token in USD
    pub realized_pnl_usd: Decimal,
    
    /// Unrealized P&L for this token in USD
    pub unrealized_pnl_usd: Decimal,
    
    /// Total P&L for this token
    pub total_pnl_usd: Decimal,
    
    /// Number of buy transactions
    pub buy_count: u32,
    
    /// Number of sell transactions
    pub sell_count: u32,
    
    /// Total tokens bought
    pub total_bought: Decimal,
    
    /// Total tokens sold
    pub total_sold: Decimal,
    
    /// Average buy price in USD
    pub avg_buy_price_usd: Decimal,
    
    /// Average sell price in USD
    pub avg_sell_price_usd: Decimal,
    
    /// First buy timestamp
    pub first_buy_time: Option<DateTime<Utc>>,
    
    /// Last sell timestamp
    pub last_sell_time: Option<DateTime<Utc>>,
    
    /// Hold time for this token in minutes
    pub hold_time_minutes: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Holding {
    /// Token mint address
    pub token_mint: String,
    
    /// Token symbol (if known)
    pub token_symbol: Option<String>,
    
    /// Amount currently held
    pub amount: Decimal,
    
    /// Average cost basis in USD per token
    pub avg_cost_basis_usd: Decimal,
    
    /// Current price in USD per token
    pub current_price_usd: Decimal,
    
    /// Total cost basis in USD
    pub total_cost_basis_usd: Decimal,
    
    /// Current value in USD
    pub current_value_usd: Decimal,
    
    /// Unrealized P&L in USD
    pub unrealized_pnl_usd: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReportMetadata {
    /// When this report was generated
    pub generated_at: DateTime<Utc>,
    
    /// Total events processed
    pub events_processed: u32,
    
    /// Events filtered out
    pub events_filtered: u32,
    
    /// Analysis duration in seconds
    pub analysis_duration_seconds: f64,
    
    /// Filters applied
    pub filters_applied: PnLFilters,
    
    /// Any warnings or issues during analysis
    pub warnings: Vec<String>,
}

/// Custom deserializer for amount fields that can be either string or number
fn deserialize_amount<'de, D>(deserializer: D) -> std::result::Result<u128, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{Error, Unexpected, Visitor};
    use std::fmt;

    struct AmountVisitor;

    impl Visitor<'_> for AmountVisitor {
        type Value = u128;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string or number representing an amount")
        }

        fn visit_u64<E>(self, value: u64) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(value as u128)
        }

        fn visit_i64<E>(self, value: i64) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            if value >= 0 {
                Ok(value as u128)
            } else {
                Err(Error::invalid_value(Unexpected::Signed(value), &self))
            }
        }

        fn visit_f64<E>(self, value: f64) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            // Handle large floating point numbers more gracefully
            if value >= 0.0 && value.is_finite() {
                // For very large numbers, truncate the fractional part
                let truncated = value.floor();
                if truncated <= (u128::MAX as f64) {
                    Ok(truncated as u128)
                } else {
                    // If the number is too large for u128, use u128::MAX
                    debug!("Large amount {} truncated to u128::MAX", value);
                    Ok(u128::MAX)
                }
            } else {
                Err(Error::invalid_value(Unexpected::Float(value), &self))
            }
        }

        fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            value.parse::<u128>().map_err(|_| {
                Error::invalid_value(Unexpected::Str(value), &self)
            })
        }
    }

    deserializer.deserialize_any(AmountVisitor)
}

/// Custom deserializer for signed amount fields that can be either string or number
fn deserialize_signed_amount<'de, D>(deserializer: D) -> std::result::Result<i128, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{Error, Unexpected, Visitor};
    use std::fmt;

    struct SignedAmountVisitor;

    impl Visitor<'_> for SignedAmountVisitor {
        type Value = i128;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string or number representing a signed amount")
        }

        fn visit_u64<E>(self, value: u64) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(value as i128)
        }

        fn visit_i64<E>(self, value: i64) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(value as i128)
        }

        fn visit_f64<E>(self, value: f64) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            if value.is_finite() {
                let truncated = if value >= 0.0 { value.floor() } else { value.ceil() };
                if truncated >= (i128::MIN as f64) && truncated <= (i128::MAX as f64) {
                    Ok(truncated as i128)
                } else {
                    // If the number is too large for i128, use appropriate limit
                    if value > 0.0 {
                        debug!("Large positive amount {} truncated to i128::MAX", value);
                        Ok(i128::MAX)
                    } else {
                        debug!("Large negative amount {} truncated to i128::MIN", value);
                        Ok(i128::MIN)
                    }
                }
            } else {
                Err(Error::invalid_value(Unexpected::Float(value), &self))
            }
        }

        fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            value.parse::<i128>().map_err(|_| {
                Error::invalid_value(Unexpected::Str(value), &self)
            })
        }
    }

    deserializer.deserialize_any(SignedAmountVisitor)
}

/// Default value for missing symbol field
fn default_symbol() -> String {
    "UNKNOWN".to_string()
}

/// Default value for missing tx_type field
fn default_tx_type() -> String {
    "unknown".to_string()
}

/// Custom deserializer for nullable f64 fields
fn deserialize_nullable_f64<'de, D>(deserializer: D) -> std::result::Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{Error, Unexpected, Visitor};
    use std::fmt;

    struct NullableF64Visitor;

    impl Visitor<'_> for NullableF64Visitor {
        type Value = f64;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a number or null")
        }

        fn visit_none<E>(self) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            // Return 0.0 for null values
            Ok(0.0)
        }

        fn visit_unit<E>(self) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            // Return 0.0 for null values
            Ok(0.0)
        }

        fn visit_f64<E>(self, value: f64) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(value)
        }

        fn visit_i64<E>(self, value: i64) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(value as f64)
        }

        fn visit_u64<E>(self, value: u64) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(value as f64)
        }

        fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            value.parse::<f64>().map_err(|_| {
                Error::invalid_value(Unexpected::Str(value), &self)
            })
        }
    }

    deserializer.deserialize_any(NullableF64Visitor)
}

/// Deserialize a string that might be null
fn deserialize_nullable_string<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{Error, Visitor};
    use std::fmt;

    struct NullableStringVisitor;

    impl Visitor<'_> for NullableStringVisitor {
        type Value = String;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string or null")
        }

        fn visit_none<E>(self) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            // Return empty string for null values
            Ok(String::new())
        }

        fn visit_unit<E>(self) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            // Return empty string for null values
            Ok(String::new())
        }

        fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(value.to_string())
        }

        fn visit_string<E>(self, value: String) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(value)
        }
    }

    deserializer.deserialize_any(NullableStringVisitor)
}

/// Deserialize an optional f64 that might be missing or null
fn deserialize_optional_nullable_f64<'de, D>(deserializer: D) -> std::result::Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{Error, Visitor};
    use std::fmt;

    struct OptionalNullableF64Visitor;

    impl Visitor<'_> for OptionalNullableF64Visitor {
        type Value = f64;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a number, null, or missing field")
        }

        fn visit_none<E>(self) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            // Return 0.0 for null values
            Ok(0.0)
        }

        fn visit_unit<E>(self) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            // Return 0.0 for null values
            Ok(0.0)
        }

        fn visit_f64<E>(self, value: f64) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(value)
        }

        fn visit_i64<E>(self, value: i64) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(value as f64)
        }

        fn visit_u64<E>(self, value: u64) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(value as f64)
        }

        fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            match value.parse::<f64>() {
                Ok(parsed) => Ok(parsed),
                Err(_) => {
                    warn!("Could not parse '{}' as f64, using 0.0", value);
                    Ok(0.0)
                }
            }
        }
    }

    deserializer.deserialize_any(OptionalNullableF64Visitor)
}


#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PnLFilters {
    /// Minimum wallet capital in SOL
    pub min_capital_sol: Decimal,
    
    /// Minimum hold time in minutes
    pub min_hold_minutes: Decimal,
    
    /// Minimum number of trades
    pub min_trades: u32,
    
    /// Minimum win rate percentage
    pub min_win_rate: Decimal,
    
    /// Maximum signatures processed
    pub max_signatures: Option<u32>,
    
    /// Maximum transactions to fetch from external APIs (BirdEye, etc.)
    /// If None, uses system default from config
    pub max_transactions_to_fetch: Option<u32>,
    
    /// Timeframe filter
    pub timeframe_filter: Option<AnalysisTimeframe>,
}

/// Trait for fetching token prices
#[async_trait]
pub trait PriceFetcher: Send + Sync {
    /// Fetch current prices for multiple tokens
    async fn fetch_prices(
        &self,
        token_mints: &[String],
        vs_token: Option<&str>,
    ) -> Result<HashMap<String, Decimal>>;
    
    /// Fetch historical price for a token at a specific time
    async fn fetch_historical_price(
        &self,
        token_mint: &str,
        timestamp: DateTime<Utc>,
        vs_token: Option<&str>,
    ) -> Result<Option<Decimal>>;
}



