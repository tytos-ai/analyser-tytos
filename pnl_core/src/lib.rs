// Removed timeframe module - functions were never used in actual P&L processing
pub mod history_parser;
pub mod new_parser;
pub mod new_pnl_engine;
pub mod zerion_balance_fetcher;

// New algorithm exports (primary P&L system)
pub use zerion_balance_fetcher::{TokenBalance, ZerionBalanceFetcher};
pub use history_parser::{
    HistoryBalanceChange, HistoryTransaction, HistoryTransactionParser, ParsedHistoryTransaction,
};
pub use new_parser::{NewEventType, NewFinancialEvent, NewTransactionParser, ParsedTransaction};
pub use new_pnl_engine::{
    MatchedTrade, NewPnLEngine, PortfolioPnLResult, RemainingPosition, TokenPnLResult,
    UnmatchedSell,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, warn};

/// Single transaction from general trader API
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
    pub owner: String, // Wallet address
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
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_optional_signed_amount")]
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
            value
                .parse::<u128>()
                .map_err(|_| Error::invalid_value(Unexpected::Str(value), &self))
        }
    }

    deserializer.deserialize_any(AmountVisitor)
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
            value
                .parse::<f64>()
                .map_err(|_| Error::invalid_value(Unexpected::Str(value), &self))
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

/// Deserialize an optional signed amount that might be missing
fn deserialize_optional_signed_amount<'de, D>(
    deserializer: D,
) -> std::result::Result<i128, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{Error, Unexpected, Visitor};
    use std::fmt;

    struct OptionalSignedAmountVisitor;

    impl Visitor<'_> for OptionalSignedAmountVisitor {
        type Value = i128;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string or number representing a signed amount, or missing field")
        }

        fn visit_none<E>(self) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            // Return 0 for missing values
            Ok(0)
        }

        fn visit_unit<E>(self) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            // Return 0 for null values
            Ok(0)
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
                let truncated = if value >= 0.0 {
                    value.floor()
                } else {
                    value.ceil()
                };
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
            match value.parse::<i128>() {
                Ok(parsed) => Ok(parsed),
                Err(_) => {
                    warn!("Could not parse '{}' as i128, using 0", value);
                    Ok(0)
                }
            }
        }
    }

    deserializer.deserialize_any(OptionalSignedAmountVisitor)
}
