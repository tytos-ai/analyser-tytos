use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoostedToken {
    pub url: String,
    #[serde(rename = "chainId")]
    pub chain_id: String,
    #[serde(rename = "tokenAddress")]
    pub token_address: String,
    pub icon: Option<String>,
    pub header: Option<String>,
    #[serde(rename = "openGraph")]
    pub open_graph: Option<String>,
    pub description: Option<String>,
    pub links: Option<Vec<TokenLink>>,
    pub amount: Option<u64>,
    #[serde(rename = "totalAmount")]
    pub total_amount: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenLink {
    #[serde(rename = "type")]
    pub link_type: Option<String>,
    pub label: Option<String>,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPair {
    #[serde(rename = "chainId")]
    pub chain_id: String,
    #[serde(rename = "dexId")]
    pub dex_id: String,
    pub url: String,
    #[serde(rename = "pairAddress")]
    pub pair_address: String,
    pub labels: Option<Vec<String>>,
    #[serde(rename = "baseToken")]
    pub base_token: Token,
    #[serde(rename = "quoteToken")]
    pub quote_token: Token,
    #[serde(rename = "priceNative")]
    pub price_native: Option<String>,
    #[serde(rename = "priceUsd")]
    pub price_usd: Option<String>,
    pub txns: Option<HashMap<String, TransactionCount>>,
    pub volume: Option<HashMap<String, f64>>,
    #[serde(rename = "priceChange")]
    pub price_change: Option<HashMap<String, f64>>,
    pub liquidity: Option<Liquidity>,
    pub fdv: Option<f64>,
    #[serde(rename = "marketCap")]
    pub market_cap: Option<f64>,
    #[serde(rename = "pairCreatedAt")]
    pub pair_created_at: Option<i64>,
    pub info: Option<TokenInfo>,
    pub boosts: Option<BoostInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub address: String,
    pub name: String,
    pub symbol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionCount {
    pub buys: u64,
    pub sells: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Liquidity {
    pub usd: Option<f64>,
    pub base: Option<f64>,
    pub quote: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    #[serde(rename = "imageUrl")]
    pub image_url: Option<String>,
    pub websites: Option<Vec<Website>>,
    pub socials: Option<Vec<Social>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Website {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Social {
    #[serde(rename = "type")]
    pub social_type: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoostInfo {
    pub active: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendingToken {
    pub token_address: String,
    pub chain_id: String,
    pub boost_amount: u64,
    pub description: Option<String>,
    pub top_pair: Option<TrendingPair>,
    pub discovered_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendingPair {
    pub pair_address: String,
    pub dex_id: String,
    pub base_token_symbol: String,
    pub quote_token_symbol: String,
    pub price_usd: f64,
    pub volume_24h: f64,
    pub volume_6h: f64,
    pub volume_1h: f64,
    pub txns_24h: u64,
    pub txns_6h: u64,
    pub txns_1h: u64,
    pub price_change_24h: f64,
    pub price_change_6h: f64,
    pub price_change_1h: f64,
    pub liquidity_usd: f64,
    pub market_cap: Option<f64>,
    pub created_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendingCriteria {
    pub min_volume_24h: f64,
    pub min_txns_24h: u64,
    pub min_liquidity_usd: f64,
    pub min_price_change_24h: Option<f64>,
    pub max_pair_age_hours: Option<u64>,
}

impl Default for TrendingCriteria {
    fn default() -> Self {
        Self {
            min_volume_24h: 1_270_000.0,    // $1.27M based on analysis
            min_txns_24h: 45_000,           // 45K transactions based on analysis
            min_liquidity_usd: 10_000.0,    // $10K minimum liquidity
            min_price_change_24h: Some(50.0), // 50% price change for high volatility
            max_pair_age_hours: Some(168),   // 1 week old max
        }
    }
}