use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, info};

#[derive(Error, Debug)]
pub enum BirdEyeError {
    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error: {0}")]
    Api(String),
    #[error("Rate limit exceeded")]
    RateLimit,
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    #[error("Authentication error")]
    Auth,
}

/// BirdEye API client configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BirdEyeConfig {
    /// API base URL
    pub api_base_url: String,
    /// API key for authentication
    pub api_key: String,
    /// Request timeout in seconds
    pub request_timeout_seconds: u64,
    /// Rate limit per second
    pub rate_limit_per_second: u32,
}

impl Default for BirdEyeConfig {
    fn default() -> Self {
        Self {
            api_base_url: "https://public-api.birdeye.so".to_string(),
            api_key: "".to_string(),
            request_timeout_seconds: 30,
            rate_limit_per_second: 100, // Conservative default
        }
    }
}

/// Trending token response from BirdEye
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendingTokenResponse {
    pub success: bool,
    pub data: TrendingTokenData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendingTokenData {
    pub tokens: Vec<TrendingToken>,
    pub total: Option<u32>,
    #[serde(rename = "updateUnixTime")]
    pub update_unix_time: Option<i64>,
    #[serde(rename = "updateHumanTime")]
    pub update_human_time: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendingToken {
    pub address: String,
    pub symbol: String,
    pub name: String,
    pub decimals: Option<u8>,
    pub price: f64,
    #[serde(rename = "price24hChangePercent")]
    pub price_change_24h: Option<f64>,
    #[serde(rename = "volume24hUSD")]
    pub volume_24h: Option<f64>,
    #[serde(rename = "volume24hChangePercent")]
    pub volume_change_24h: Option<f64>,
    pub liquidity: Option<f64>,
    pub fdv: Option<f64>,
    pub marketcap: Option<f64>,
    pub rank: Option<u32>,
    #[serde(rename = "logoURI")]
    pub logo_uri: Option<String>,
    #[serde(rename = "txns24h")]
    pub txns_24h: Option<u32>,
    #[serde(rename = "lastTradeUnixTime")]
    pub last_trade_unix_time: Option<i64>,
}

/// Top traders response from BirdEye
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopTradersResponse {
    pub success: bool,
    pub data: TopTradersData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopTradersData {
    pub items: Vec<TopTrader>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopTrader {
    #[serde(rename = "tokenAddress")]
    pub token_address: String,
    pub owner: String, // This is the wallet address
    pub tags: Vec<String>,
    #[serde(rename = "type")]
    pub trader_type: String, // "24h"
    pub volume: f64,
    pub trade: u32,
    #[serde(rename = "tradeBuy")]
    pub trade_buy: u32,
    #[serde(rename = "tradeSell")]
    pub trade_sell: u32,
    #[serde(rename = "volumeBuy")]
    pub volume_buy: f64,
    #[serde(rename = "volumeSell")]
    pub volume_sell: f64,
}

/// Trader transactions response from BirdEye
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraderTxsResponse {
    pub success: bool,
    pub data: TraderTxsData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraderTxsData {
    pub items: Vec<TraderTransaction>,
    #[serde(rename = "hasNext")]
    pub has_next: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraderTransaction {
    #[serde(rename = "txHash")]
    pub tx_hash: String,
    #[serde(rename = "blockUnixTime")]
    pub block_unix_time: i64,
    #[serde(rename = "blockHumanTime")]
    pub block_human_time: String,
    pub side: String, // "buy" or "sell"
    #[serde(rename = "tokenAddress")]
    pub token_address: String,
    #[serde(rename = "tokenSymbol")]
    pub token_symbol: Option<String>,
    #[serde(rename = "tokenAmount")]
    pub token_amount: f64,
    #[serde(rename = "tokenPrice")]
    pub token_price: f64,
    #[serde(rename = "volumeUsd")]
    pub volume_usd: f64,
    pub source: Option<String>,
    #[serde(rename = "poolAddress")]
    pub pool_address: Option<String>,
}

/// General trader transactions response from BirdEye (/trader/txs/seek_by_time)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralTraderTransactionsResponse {
    pub success: bool,
    pub data: GeneralTraderTransactionsData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralTraderTransactionsData {
    pub items: Vec<GeneralTraderTransaction>,
    #[serde(rename = "hasNext")]
    pub has_next: Option<bool>, // Make optional since it may not always be present
}

/// Single transaction from general BirdEye trader API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralTraderTransaction {
    pub quote: TokenTransactionSide,
    pub base: TokenTransactionSide,
    #[serde(rename = "base_price")]
    pub base_price: Option<f64>,
    #[serde(rename = "quote_price")]
    pub quote_price: f64,
    #[serde(rename = "tx_hash")]
    pub tx_hash: String,
    pub source: String,
    #[serde(rename = "block_unix_time")]
    pub block_unix_time: i64,
    #[serde(rename = "tx_type")]
    pub tx_type: String, // "swap"
    pub address: String, // Program address
    pub owner: String,   // Wallet address
}

/// Token side of a transaction (quote or base)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTransactionSide {
    pub symbol: String,
    pub decimals: u32,
    pub address: String,
    #[serde(deserialize_with = "deserialize_amount")]
    pub amount: u64,
    #[serde(rename = "type")]
    pub transfer_type: String, // "transfer", "transferChecked"
    #[serde(rename = "type_swap")]
    pub type_swap: String, // "from", "to"
    #[serde(rename = "ui_amount")]
    pub ui_amount: f64,
    pub price: Option<f64>,
    #[serde(rename = "nearest_price")]
    pub nearest_price: f64,
    #[serde(rename = "change_amount")]
    pub change_amount: i64,
    #[serde(rename = "ui_change_amount")]
    pub ui_change_amount: f64,
    #[serde(rename = "fee_info")]
    pub fee_info: Option<serde_json::Value>,
}

/// Historical price response from BirdEye
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalPriceResponse {
    pub success: bool,
    pub data: HistoricalPriceData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalPriceData {
    pub value: f64,
    #[serde(rename = "updateUnixTime")]
    pub update_unix_time: i64,
    #[serde(rename = "updateHumanTime")]
    pub update_human_time: String,
}

/// Current price response from BirdEye
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceResponse {
    pub success: bool,
    pub data: PriceData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceData {
    pub value: f64,
    #[serde(rename = "updateUnixTime")]
    pub update_unix_time: i64,
    #[serde(rename = "updateHumanTime")]
    pub update_human_time: String,
}

/// BirdEye API client
#[derive(Clone)]
pub struct BirdEyeClient {
    config: BirdEyeConfig,
    http_client: Client,
}

impl BirdEyeClient {
    pub fn new(config: BirdEyeConfig) -> Result<Self> {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(config.request_timeout_seconds))
            .build()?;

        Ok(Self {
            config,
            http_client,
        })
    }

    /// Get trending tokens from BirdEye
    pub async fn get_trending_tokens(&self, chain: &str) -> Result<Vec<TrendingToken>, BirdEyeError> {
        let url = format!("{}/defi/token_trending", self.config.api_base_url);
        
        debug!("Fetching trending tokens from BirdEye for chain: {}", chain);
        
        let response = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .query(&[("chain", chain)])
            .send()
            .await?;

        if response.status() == 429 {
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            return Err(BirdEyeError::Api(format!("HTTP {}", response.status())));
        }

        let trending_response: TrendingTokenResponse = response.json().await?;
        
        if !trending_response.success {
            return Err(BirdEyeError::Api("API returned success=false".to_string()));
        }

        info!("Retrieved {} trending tokens from BirdEye", trending_response.data.tokens.len());
        Ok(trending_response.data.tokens)
    }

    /// Get top traders for a specific token
    pub async fn get_top_traders(&self, token_address: &str, limit: Option<u32>) -> Result<Vec<TopTrader>, BirdEyeError> {
        let url = format!("{}/defi/v2/tokens/top_traders", self.config.api_base_url);
        
        debug!("Fetching top traders from BirdEye for token: {}", token_address);
        
        let mut query_params = vec![
            ("address", token_address),
            ("time_frame", "24h"),
            ("sort_type", "desc"),
            ("sort_by", "volume"),
            ("offset", "0"),
        ];
        
        let limit_string = limit.unwrap_or(10).to_string();
        query_params.push(("limit", &limit_string));

        let response = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .header("x-chain", "solana")
            .header("accept", "application/json")
            .query(&query_params)
            .send()
            .await?;

        if response.status() == 429 {
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            return Err(BirdEyeError::Api(format!("HTTP {}", response.status())));
        }

        let top_traders_response: TopTradersResponse = response.json().await?;
        
        if !top_traders_response.success {
            return Err(BirdEyeError::Api("API returned success=false".to_string()));
        }

        info!("Retrieved {} top traders from BirdEye for token {}", 
              top_traders_response.data.items.len(), token_address);
        Ok(top_traders_response.data.items)
    }

    /// Get transaction history for a specific trader and token
    pub async fn get_trader_transactions(
        &self,
        wallet_address: &str,
        token_address: &str,
        from_time: Option<i64>,
        to_time: Option<i64>,
        limit: Option<u32>,
    ) -> Result<Vec<TraderTransaction>, BirdEyeError> {
        let url = format!("{}/trader/txs/seek_by_time", self.config.api_base_url);
        
        debug!("Fetching trader transactions from BirdEye for wallet: {} token: {}", 
               wallet_address, token_address);
        
        let mut query_params = vec![
            ("wallet", wallet_address),
            ("token_address", token_address),
        ];
        
        let from_string;
        let to_string;
        let limit_string;
        
        if let Some(from) = from_time {
            from_string = from.to_string();
            query_params.push(("from", &from_string));
        }
        if let Some(to) = to_time {
            to_string = to.to_string();
            query_params.push(("to", &to_string));
        }
        if let Some(limit_val) = limit {
            limit_string = limit_val.to_string();
            query_params.push(("limit", &limit_string));
        }

        let response = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .query(&query_params)
            .send()
            .await?;

        if response.status() == 429 {
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            return Err(BirdEyeError::Api(format!("HTTP {}", response.status())));
        }

        let trader_txs_response: TraderTxsResponse = response.json().await?;
        
        if !trader_txs_response.success {
            return Err(BirdEyeError::Api("API returned success=false".to_string()));
        }

        debug!("Retrieved {} transactions from BirdEye for wallet {} token {}", 
               trader_txs_response.data.items.len(), wallet_address, token_address);
        Ok(trader_txs_response.data.items)
    }

    /// Get all trader transactions for a wallet (general endpoint without token filter)
    pub async fn get_all_trader_transactions(
        &self,
        wallet_address: &str,
        from_time: Option<i64>,
        to_time: Option<i64>,
        limit: Option<u32>,
    ) -> Result<Vec<GeneralTraderTransaction>, BirdEyeError> {
        let url = format!("{}/trader/txs/seek_by_time", self.config.api_base_url);
        
        debug!("Fetching all trader transactions from BirdEye for wallet: {}", wallet_address);
        
        let mut query_params = vec![
            ("address", wallet_address),
        ];
        
        let from_string;
        let to_string;
        let limit_string;
        
        if let Some(from) = from_time {
            from_string = from.to_string();
            query_params.push(("after_time", &from_string));
        }
        if let Some(to) = to_time {
            to_string = to.to_string();
            query_params.push(("before_time", &to_string));
        }
        if let Some(limit_val) = limit {
            limit_string = limit_val.to_string();
            query_params.push(("limit", &limit_string));
        }

        let response = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .header("x-chain", "solana")
            .query(&query_params)
            .send()
            .await?;

        if response.status() == 429 {
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            return Err(BirdEyeError::Api(format!("HTTP {}", response.status())));
        }

        let general_txs_response: GeneralTraderTransactionsResponse = response.json().await?;
        
        if !general_txs_response.success {
            return Err(BirdEyeError::Api("API returned success=false".to_string()));
        }

        debug!("Retrieved {} general transactions from BirdEye for wallet {}", 
               general_txs_response.data.items.len(), wallet_address);
        Ok(general_txs_response.data.items)
    }

    /// Get historical price for a token at a specific timestamp
    pub async fn get_historical_price(
        &self,
        token_address: &str,
        unix_timestamp: i64,
    ) -> Result<f64, BirdEyeError> {
        let url = format!("{}/defi/historical_price_unix", self.config.api_base_url);
        
        debug!("Fetching historical price from BirdEye for token: {} at timestamp: {}", 
               token_address, unix_timestamp);
        
        let response = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .query(&[
                ("address", token_address),
                ("timestamp", &unix_timestamp.to_string()),
            ])
            .send()
            .await?;

        if response.status() == 429 {
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            return Err(BirdEyeError::Api(format!("HTTP {}", response.status())));
        }

        let price_response: HistoricalPriceResponse = response.json().await?;
        
        if !price_response.success {
            return Err(BirdEyeError::Api("API returned success=false".to_string()));
        }

        debug!("Retrieved historical price from BirdEye for token {}: ${}", 
               token_address, price_response.data.value);
        Ok(price_response.data.value)
    }

    /// Get current price for a token
    pub async fn get_current_price(&self, token_address: &str) -> Result<f64, BirdEyeError> {
        let url = format!("{}/defi/price", self.config.api_base_url);
        
        debug!("Fetching current price from BirdEye for token: {}", token_address);
        
        let response = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .query(&[("address", token_address)])
            .send()
            .await?;

        if response.status() == 429 {
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            return Err(BirdEyeError::Api(format!("HTTP {}", response.status())));
        }

        let price_response: PriceResponse = response.json().await?;
        
        if !price_response.success {
            return Err(BirdEyeError::Api("API returned success=false".to_string()));
        }

        debug!("Retrieved current price from BirdEye for token {}: ${}", 
               token_address, price_response.data.value);
        Ok(price_response.data.value)
    }

    /// Get current prices for multiple tokens
    pub async fn get_current_prices(&self, token_addresses: &[String]) -> Result<HashMap<String, f64>, BirdEyeError> {
        let url = format!("{}/defi/multi_price", self.config.api_base_url);
        
        debug!("Fetching current prices from BirdEye for {} tokens", token_addresses.len());
        
        let address_list = token_addresses.join(",");
        
        let response = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .query(&[("list_address", &address_list)])
            .send()
            .await?;

        if response.status() == 429 {
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            return Err(BirdEyeError::Api(format!("HTTP {}", response.status())));
        }

        // BirdEye multi_price returns a different format
        let response_text = response.text().await?;
        let response_data: serde_json::Value = serde_json::from_str(&response_text)
            .map_err(|e| BirdEyeError::InvalidResponse(format!("JSON parse error: {}", e)))?;
        
        if !response_data.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
            return Err(BirdEyeError::Api("API returned success=false".to_string()));
        }

        let mut prices = HashMap::new();
        
        if let Some(data) = response_data.get("data") {
            for token_address in token_addresses {
                if let Some(price_value) = data.get(token_address)
                    .and_then(|v| v.get("value"))
                    .and_then(|v| v.as_f64()) 
                {
                    prices.insert(token_address.clone(), price_value);
                }
            }
        }

        debug!("Retrieved current prices from BirdEye for {}/{} tokens", 
               prices.len(), token_addresses.len());
        Ok(prices)
    }

    /// Filter trending tokens based on quality criteria
    pub fn filter_trending_tokens(
        &self,
        tokens: Vec<TrendingToken>,
        min_volume_usd: Option<f64>,
        min_price_change_24h: Option<f64>,
        min_liquidity: Option<f64>,
        min_market_cap: Option<f64>,
        max_rank: Option<u32>,
    ) -> Vec<TrendingToken> {
        tokens
            .into_iter()
            .filter(|token| {
                // Volume filter
                if let Some(min_vol) = min_volume_usd {
                    if token.volume_24h.unwrap_or(0.0) < min_vol {
                        return false;
                    }
                }
                
                // Price change filter
                if let Some(min_change) = min_price_change_24h {
                    if token.price_change_24h.unwrap_or(0.0) < min_change {
                        return false;
                    }
                }
                
                // Liquidity filter
                if let Some(min_liq) = min_liquidity {
                    if token.liquidity.unwrap_or(0.0) < min_liq {
                        return false;
                    }
                }
                
                // Market cap filter
                if let Some(min_cap) = min_market_cap {
                    if token.marketcap.unwrap_or(0.0) < min_cap {
                        return false;
                    }
                }
                
                // Rank filter (lower rank numbers are better)
                if let Some(max_r) = max_rank {
                    if token.rank.unwrap_or(u32::MAX) > max_r {
                        return false;
                    }
                }
                
                true
            })
            .collect()
    }

    /// Filter top traders based on quality criteria
    pub fn filter_top_traders(
        &self,
        traders: Vec<TopTrader>,
        min_volume_usd: f64,
        min_trades: u32,
        _min_win_rate: Option<f64>, // Not available in BirdEye response
        _max_last_trade_hours: Option<u32>, // Not available in BirdEye response
    ) -> Vec<TopTrader> {
        traders
            .into_iter()
            .filter(|trader| {
                // Volume filter
                if trader.volume < min_volume_usd {
                    return false;
                }
                
                // Trades filter
                if trader.trade < min_trades {
                    return false;
                }
                
                // Note: Win rate and last trade time filters are not available
                // in the BirdEye top traders API response structure
                
                true
            })
            .collect()
    }
}

/// Quality criteria for filtering trending tokens
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendingTokenFilter {
    /// Minimum 24h volume in USD
    pub min_volume_usd: Option<f64>,
    /// Minimum 24h price change percentage
    pub min_price_change_24h: Option<f64>,
    /// Minimum liquidity in USD
    pub min_liquidity: Option<f64>,
    /// Minimum market cap in USD
    pub min_market_cap: Option<f64>,
    /// Maximum rank (lower is better)
    pub max_rank: Option<u32>,
    /// Maximum number of tokens to return
    pub max_tokens: Option<usize>,
}

impl Default for TrendingTokenFilter {
    fn default() -> Self {
        Self {
            min_volume_usd: Some(10000.0),     // $10k minimum volume
            min_price_change_24h: Some(5.0),   // 5% minimum price change
            min_liquidity: Some(50000.0),      // $50k minimum liquidity
            min_market_cap: Some(100000.0),    // $100k minimum market cap
            max_rank: Some(1000),              // Top 1000 ranked tokens
            max_tokens: Some(50),              // Top 50 tokens max
        }
    }
}

/// Quality criteria for filtering top traders
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopTraderFilter {
    /// Minimum volume in USD
    pub min_volume_usd: f64,
    /// Minimum number of trades
    pub min_trades: u32,
    /// Minimum win rate percentage (0-100)
    pub min_win_rate: Option<f64>,
    /// Maximum hours since last trade
    pub max_last_trade_hours: Option<u32>,
    /// Maximum number of traders to return
    pub max_traders: Option<usize>,
}

impl Default for TopTraderFilter {
    fn default() -> Self {
        Self {
            min_volume_usd: 1000.0,    // $1k minimum volume
            min_trades: 5,             // At least 5 trades
            min_win_rate: Some(60.0),  // 60% win rate
            max_last_trade_hours: Some(48), // Last trade within 48 hours
            max_traders: Some(50),     // Top 50 traders max
        }
    }
}

/// Custom deserializer for amount fields that can be either string or number
fn deserialize_amount<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{Error, Unexpected, Visitor};
    use std::fmt;

    struct AmountVisitor;

    impl<'de> Visitor<'de> for AmountVisitor {
        type Value = u64;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string or number representing an amount")
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(value)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            if value >= 0 {
                Ok(value as u64)
            } else {
                Err(Error::invalid_value(Unexpected::Signed(value), &self))
            }
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            if value >= 0.0 && value.fract() == 0.0 {
                Ok(value as u64)
            } else {
                Err(Error::invalid_value(Unexpected::Float(value), &self))
            }
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: Error,
        {
            value.parse::<u64>().map_err(|_| {
                Error::invalid_value(Unexpected::Str(value), &self)
            })
        }
    }

    deserializer.deserialize_any(AmountVisitor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_creation() {
        let config = BirdEyeConfig::default();
        assert_eq!(config.api_base_url, "https://public-api.birdeye.so");
        assert_eq!(config.request_timeout_seconds, 30);
    }

    #[test]
    fn test_filter_creation() {
        let filter = TopTraderFilter::default();
        assert_eq!(filter.min_volume_usd, 1000.0);
        assert_eq!(filter.min_trades, 5);
        assert_eq!(filter.min_win_rate, Some(60.0));
    }
}