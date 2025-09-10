use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Configuration for GoldRush API client
#[derive(Debug, Clone)]
pub struct GoldRushConfig {
    pub api_key: String,
    pub base_url: String,
    pub timeout_seconds: u64,
}

impl Default for GoldRushConfig {
    fn default() -> Self {
        Self {
            api_key: "cqt_rQqPrvKMbyqJmTV7WVMMrGh9XKqt".to_string(),
            base_url: "https://api.covalenthq.com/v1".to_string(),
            timeout_seconds: 120, // Increased to handle large responses
        }
    }
}

/// Supported EVM chains for GoldRush API
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GoldRushChain {
    #[serde(rename = "eth-mainnet")]
    Ethereum,
    #[serde(rename = "base-mainnet")]
    Base,
    #[serde(rename = "bsc-mainnet")]
    Bsc,
}

impl Default for GoldRushChain {
    fn default() -> Self {
        GoldRushChain::Ethereum
    }
}

impl GoldRushChain {
    pub fn as_str(&self) -> &'static str {
        match self {
            GoldRushChain::Ethereum => "eth-mainnet",
            GoldRushChain::Base => "base-mainnet",
            GoldRushChain::Bsc => "bsc-mainnet",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, crate::error::GoldRushError> {
        match s {
            "eth-mainnet" | "ethereum" => Ok(GoldRushChain::Ethereum),
            "base-mainnet" | "base" => Ok(GoldRushChain::Base),
            "bsc-mainnet" | "bsc" | "binance" => Ok(GoldRushChain::Bsc),
            _ => Err(crate::error::GoldRushError::InvalidChain {
                chain: s.to_string(),
            }),
        }
    }
}

/// GoldRush API response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldRushResponse<T> {
    pub data: T,
    pub error: bool,
    pub error_message: Option<String>,
    pub error_code: Option<u32>,
}

/// Transaction response from GoldRush API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionsResponse {
    pub address: String,
    pub updated_at: DateTime<Utc>,
    pub next_update_at: DateTime<Utc>,
    pub quote_currency: String,
    pub chain_id: u32,
    pub chain_name: String,
    pub items: Vec<GoldRushTransaction>,
    pub pagination: Option<PaginationInfo>,
}

/// Pagination info for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationInfo {
    pub has_more: bool,
    pub page_number: u32,
    pub page_size: u32,
    pub total_count: Option<u32>,
}

/// Main transaction structure from GoldRush API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldRushTransaction {
    pub block_signed_at: DateTime<Utc>,
    pub block_height: u64,
    pub block_hash: String,
    pub tx_hash: String,
    pub tx_offset: u32,
    pub successful: bool,
    pub miner_address: String,
    pub from_address: String,
    pub from_address_label: Option<String>,
    pub to_address: Option<String>,
    pub to_address_label: Option<String>,
    pub value: String, // Wei amount
    pub value_quote: Option<Decimal>, // USD value
    pub pretty_value_quote: Option<String>,
    pub gas_metadata: GasMetadata,
    pub gas_offered: u64,
    pub gas_spent: u64,
    pub gas_price: u64,
    pub gas_quote: Decimal,
    pub pretty_gas_quote: String,
    pub gas_quote_rate: Decimal,
    pub fees_paid: String,
    pub explorers: Vec<Explorer>,
    pub log_events: Option<Vec<LogEvent>>, // Optional - simple transfers don't have logs
}

/// Gas information for transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasMetadata {
    pub contract_decimals: u32,
    pub contract_name: String,
    pub contract_ticker_symbol: String,
    pub contract_address: String,
    pub supports_erc: Option<Vec<String>>,
    pub logo_url: Option<String>,
}

/// Explorer link for transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Explorer {
    pub label: String,
    pub url: String,
}

/// Log event within a transaction (for DEX swaps, token transfers, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    pub block_signed_at: DateTime<Utc>,
    pub block_height: u64,
    pub tx_offset: u32,
    pub log_offset: u32,
    pub tx_hash: String,
    pub raw_log_topics: Vec<String>,
    pub sender_contract_decimals: Option<u32>,
    pub sender_name: Option<String>,
    pub sender_contract_ticker_symbol: Option<String>,
    pub sender_address: String,
    pub sender_address_label: Option<String>,
    pub sender_logo_url: Option<String>,
    pub supports_erc: Option<Vec<String>>,
    pub sender_factory_address: Option<String>,
    pub raw_log_data: Option<String>,
    pub decoded: Option<DecodedLogEvent>,
}

/// Decoded log event data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodedLogEvent {
    pub name: String,
    pub signature: String,
    pub params: Option<Vec<DecodedParam>>,
}

/// Parameter in decoded log event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodedParam {
    pub name: String,
    pub r#type: String,
    pub indexed: bool,
    pub decoded: bool,
    pub value: serde_json::Value,
}

/// Parsed transaction for P&L calculation
#[derive(Debug, Clone)]
pub struct ParsedGoldRushTransaction {
    pub tx_hash: String,
    pub block_time: DateTime<Utc>,
    pub from_address: String,
    pub to_address: Option<String>,
    pub transaction_type: TransactionType,
    pub token_changes: Vec<TokenChange>,
    pub gas_fee_usd: Option<Decimal>,
}

/// Type of transaction for P&L classification
#[derive(Debug, Clone, PartialEq)]
pub enum TransactionType {
    /// DEX swap (buy/sell)
    Swap,
    /// Token transfer (send - treated as sell)
    Send,
    /// Token received (not a disposal event)
    Receive,
    /// Contract interaction (may contain token changes)
    ContractInteraction,
}

/// Token balance change in a transaction
#[derive(Debug, Clone)]
pub struct TokenChange {
    pub token_address: String,
    pub token_symbol: String,
    pub token_decimals: u32,
    pub amount_raw: String, // Raw amount (with decimals)
    pub amount_formatted: Decimal, // Human readable amount
    pub usd_value: Option<Decimal>, // USD value at transaction time
    pub change_type: TokenChangeType,
}

/// Type of token balance change
#[derive(Debug, Clone, PartialEq)]
pub enum TokenChangeType {
    /// Token balance increased
    Increase,
    /// Token balance decreased
    Decrease,
}

/// Request parameters for fetching transactions
#[derive(Debug, Clone, Default)]
pub struct TransactionRequest {
    pub wallet_address: String,
    pub chain: GoldRushChain,
    pub page_number: Option<u32>,
    pub page_size: Option<u32>,
    pub block_signed_at_asc: Option<bool>,
    pub no_logs: Option<bool>,
}

/// Response from balances_v2 API endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalancesResponse {
    pub address: String,
    pub chain_id: u32,
    pub chain_name: String,
    pub chain_tip_height: u64,
    pub chain_tip_signed_at: DateTime<Utc>,
    pub quote_currency: String,
    pub updated_at: DateTime<Utc>,
    pub items: Vec<TokenBalance>,
}

/// Token balance information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBalance {
    pub contract_decimals: Option<u32>,
    pub contract_name: Option<String>,
    pub contract_ticker_symbol: Option<String>,
    pub contract_address: String,
    pub contract_display_name: Option<String>,
    pub supports_erc: Option<Vec<String>>,
    pub logo_url: Option<String>,
    pub last_transferred_at: Option<DateTime<Utc>>,
    pub block_height: Option<u64>,
    pub native_token: Option<bool>,
    pub r#type: Option<String>,
    pub is_spam: Option<bool>,
    pub balance: Option<String>,
    pub balance_24h: Option<String>,
    pub quote_rate: Option<f64>,
    pub quote_rate_24h: Option<f64>,
    pub quote: Option<f64>,
    pub quote_24h: Option<f64>,
    pub pretty_quote: Option<String>,
    pub pretty_quote_24h: Option<String>,
}

/// Response from transfers_v2 API endpoint  
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransfersResponse {
    pub address: String,
    pub updated_at: DateTime<Utc>,
    pub next_update_at: DateTime<Utc>,
    pub quote_currency: String,
    pub chain_id: u32,
    pub chain_name: String,
    pub chain_tip_height: u64,
    pub chain_tip_signed_at: DateTime<Utc>,
    pub items: Vec<TransferTransaction>,
    pub pagination: Option<PaginationInfo>,
}

/// Response from allchains/transactions API endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllChainsTransactionsResponse {
    pub updated_at: DateTime<Utc>,
    pub cursor_before: Option<String>,
    pub cursor_after: Option<String>, 
    pub quote_currency: String,
    pub items: Vec<GoldRushTransaction>,
}

/// Transaction containing transfers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferTransaction {
    pub block_signed_at: DateTime<Utc>,
    pub block_height: u64,
    pub block_hash: String,
    pub tx_hash: String,
    pub tx_offset: u32,
    pub successful: bool,
    pub miner_address: String,
    pub from_address: String,
    pub from_address_label: Option<String>,
    pub to_address: String,
    pub to_address_label: Option<String>,
    pub value: String,
    pub value_quote: Option<f64>,
    pub pretty_value_quote: Option<String>,
    pub gas_metadata: GasMetadata,
    pub gas_offered: u64,
    pub gas_spent: u64,
    pub gas_price: u64,
    pub fees_paid: String,
    pub gas_quote: Option<f64>,
    pub pretty_gas_quote: Option<String>,
    pub gas_quote_rate: Option<f64>,
    pub transfers: Vec<TokenTransfer>,
}

/// Individual token transfer within a transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTransfer {
    pub block_signed_at: DateTime<Utc>,
    pub tx_hash: String,
    pub from_address: String,
    pub from_address_label: Option<String>,
    pub to_address: String,
    pub to_address_label: Option<String>,
    pub contract_decimals: Option<u32>,
    pub contract_name: Option<String>,
    pub contract_ticker_symbol: Option<String>,
    pub contract_address: String,
    pub logo_url: Option<String>,
    pub transfer_type: String, // "IN" or "OUT"
    pub delta: Option<String>,
    pub balance: Option<String>,
    pub quote_rate: Option<f64>,
    pub delta_quote: Option<f64>,
    pub pretty_delta_quote: Option<String>,
    pub balance_quote: Option<f64>,
    pub method_calls: Option<Vec<MethodCall>>,
    pub explorers: Option<Vec<Explorer>>,
}

/// Method call information  
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodCall {
    pub sender_address: String,
    pub method: String,
}