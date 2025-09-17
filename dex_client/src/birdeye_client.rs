use anyhow::Result;
use config_manager::BirdEyeConfig;
use pnl_core::{GeneralTraderTransaction, TokenTransactionSide};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, info, error, warn};

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

// BirdEye API client configuration moved to config_manager crate

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

/// Gainers-Losers response from BirdEye (/trader/gainers-losers)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GainersLosersResponse {
    pub success: bool,
    pub data: GainersLosersData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GainersLosersData {
    pub items: Vec<GainerLoser>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GainerLoser {
    pub network: String,
    pub address: String, // This is the wallet address
    pub pnl: f64,
    pub trade_count: u32,
    pub volume: f64,
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
    pub has_next: Option<bool>, // Make optional since it may not always be present
}

/// New listing token response from BirdEye (/defi/v2/tokens/new_listing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewListingTokenResponse {
    pub success: bool,
    pub data: NewListingTokenData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewListingTokenData {
    pub items: Vec<NewListingToken>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewListingToken {
    pub address: String,
    pub symbol: String,
    pub name: String,
    pub decimals: u8,
    pub source: String,
    #[serde(rename = "liquidityAddedAt")]
    pub liquidity_added_at: String,
    #[serde(rename = "logoURI")]
    pub logo_uri: Option<String>,
    pub liquidity: f64,
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
    #[serde(rename = "isScaledUiToken")]
    pub is_scaled_ui_token: bool,
    pub address: Option<String>,
    #[serde(rename = "priceChange24h")]
    pub price_change_24h: Option<f64>,
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

/// Multi-price response from BirdEye
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiPriceResponse {
    pub success: bool,
    pub data: HashMap<String, TokenPriceData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPriceData {
    pub value: f64,
    #[serde(rename = "updateUnixTime")]
    pub update_unix_time: i64,
    #[serde(rename = "updateHumanTime")]
    pub update_human_time: String,
    #[serde(rename = "priceChange24h")]
    pub price_change_24h: Option<f64>,
    #[serde(rename = "priceInNative")]
    pub price_in_native: Option<f64>,
    pub liquidity: Option<f64>,
}

/// Consolidated transaction representing the net effect of a complete transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidatedTransaction {
    /// Transaction hash
    pub tx_hash: String,
    
    /// Block timestamp
    pub block_unix_time: i64,
    
    /// Net token changes (positive = received, negative = sent)
    pub net_token_changes: HashMap<String, ConsolidatedTokenChange>,
    
    /// Total USD volume of the transaction
    pub total_volume_usd: f64,
    
    /// Transaction source/exchange
    pub source: String,
    
    /// Wallet address
    pub wallet_address: String,
}

/// Net change for a specific token within a consolidated transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidatedTokenChange {
    /// Token symbol
    pub symbol: String,
    
    /// Token address/mint
    pub address: String,
    
    /// Net change in UI amount (positive = received, negative = sent)
    pub net_ui_amount: f64,
    
    /// Net change in raw amount with decimals
    pub net_raw_amount: i128,
    
    /// Token decimals
    pub decimals: u32,
    
    /// USD value of the net change (positive = value in, negative = value out)
    pub usd_value: f64,
    
    /// Price per token at time of transaction
    pub price_per_token: f64,
}

/// Wallet transaction history response from BirdEye
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletTransactionHistoryResponse {
    pub success: bool,
    pub data: WalletTransactionData,
}

/// Transaction history data containing list of transactions per chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletTransactionData {
    #[serde(default)]
    pub solana: Vec<WalletTransaction>,
}

/// Individual wallet transaction from transaction history API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletTransaction {
    /// Transaction hash
    #[serde(rename = "txHash")]
    pub tx_hash: String,
    
    /// Block number
    #[serde(rename = "blockNumber")]
    pub block_number: u64,
    
    /// Block timestamp in ISO format
    #[serde(rename = "blockTime")]
    pub block_time: String,
    
    /// Transaction status (true = successful)
    pub status: bool,
    
    /// From address
    pub from: String,
    
    /// To address
    pub to: String,
    
    /// Transaction fee in lamports
    pub fee: u64,
    
    /// Main action type (send, swap, received, etc.)
    #[serde(rename = "mainAction")]
    pub main_action: String,
    
    /// Balance changes for all affected tokens
    #[serde(rename = "balanceChange")]
    pub balance_change: Vec<BalanceChange>,
    
    /// Detailed token transfer information
    #[serde(rename = "tokenTransfers")]
    pub token_transfers: Vec<TokenTransfer>,
    
    /// Contract label information
    #[serde(rename = "contractLabel")]
    pub contract_label: Option<ContractLabel>,
}

/// Balance change for a specific token in a transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceChange {
    /// Amount change (negative = outgoing, positive = incoming)
    pub amount: i128,
    
    /// Token symbol
    pub symbol: String,
    
    /// Token name
    pub name: String,
    
    /// Token decimals
    pub decimals: u32,
    
    /// Token mint address
    pub address: String,
    
    /// Token logo URI
    #[serde(rename = "logoURI")]
    pub logo_uri: Option<String>,
    
    /// Whether token uses scaled UI amounts
    #[serde(rename = "isScaledUiToken")]
    pub is_scaled_ui_token: bool,
    
    /// Multiplier for scaled tokens
    pub multiplier: Option<f64>,
    
    /// Token account address (for createAssociatedAccount transactions)
    #[serde(rename = "tokenAccount")]
    pub token_account: Option<String>,
    
    /// Owner wallet address (for createAssociatedAccount transactions)
    pub owner: Option<String>,
    
    /// Program ID (for createAssociatedAccount transactions) 
    #[serde(rename = "programId")]
    pub program_id: Option<String>,
}

/// Detailed token transfer within a transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTransfer {
    /// Source token account
    #[serde(rename = "fromTokenAccount")]
    pub from_token_account: String,
    
    /// Destination token account  
    #[serde(rename = "toTokenAccount")]
    pub to_token_account: String,
    
    /// Source user account
    #[serde(rename = "fromUserAccount")]
    pub from_user_account: Option<String>,
    
    /// Destination user account
    #[serde(rename = "toUserAccount")]
    pub to_user_account: Option<String>,
    
    /// Token amount transferred (UI amount)
    #[serde(rename = "tokenAmount")]
    pub token_amount: f64,
    
    /// Token mint address
    pub mint: String,
    
    /// Whether this is a native SOL transfer
    #[serde(rename = "transferNative")]
    pub transfer_native: bool,
    
    /// Whether token uses scaled UI amounts
    #[serde(rename = "isScaledUiToken")]
    pub is_scaled_ui_token: bool,
    
    /// Multiplier for scaled tokens
    pub multiplier: Option<f64>,
}

/// Contract label information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractLabel {
    /// Contract address
    pub address: String,
    
    /// Contract name
    pub name: String,
    
    /// Additional metadata
    pub metadata: Option<ContractMetadata>,
}

/// Contract metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractMetadata {
    /// Contract icon
    pub icon: String,
}

/// Wallet portfolio response from BirdEye
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletPortfolioResponse {
    pub success: bool,
    pub data: WalletPortfolioData,
}

/// Portfolio data containing list of token balances
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletPortfolioData {
    pub items: Vec<WalletTokenBalance>,
}


/// Individual token balance in a wallet portfolio
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletTokenBalance {
    /// Token mint address
    pub address: String,
    
    /// Token decimals
    pub decimals: u32,
    
    /// Raw balance (with decimals)
    pub balance: u64,
    
    /// UI amount (scaled by decimals)
    #[serde(rename = "uiAmount")]
    pub ui_amount: f64,
    
    /// Chain ID
    #[serde(rename = "chainId")]
    pub chain_id: String,
    
    /// Token name
    pub name: Option<String>,
    
    /// Token symbol
    pub symbol: Option<String>,
    
    /// Token icon URL
    pub icon: Option<String>,
    
    /// Token logo URI
    #[serde(rename = "logoURI")]
    pub logo_uri: Option<String>,
    
    /// Current USD price per token
    #[serde(rename = "priceUsd", alias = "price", default)]
    pub price_usd: f64,
    
    /// Current USD value of this balance
    #[serde(rename = "valueUsd", alias = "value", default)]
    pub value_usd: f64,
    
    /// Whether token uses scaled UI amounts
    #[serde(rename = "isScaledUiToken")]
    pub is_scaled_ui_token: bool,
    
    /// Multiplier for scaled tokens
    pub multiplier: Option<f64>,
}

/// BirdEye API client
#[derive(Debug, Clone)]
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

    /// Get the BirdEye client configuration
    pub fn config(&self) -> &BirdEyeConfig {
        &self.config
    }


    /// Get trending tokens from BirdEye using multiple sorting criteria for enhanced discovery
    pub async fn get_trending_tokens_multi_sort(&self, chain: &str) -> Result<Vec<TrendingToken>, BirdEyeError> {
        
        debug!("🔄 Starting multi-sort trending token discovery for chain: {}", chain);
        
        // Define the three sorting strategies
        let sort_strategies = [
            ("rank", "asc", "momentum/community interest"),
            ("volume24hUSD", "desc", "trading activity"),
            ("liquidity", "desc", "market depth"),
        ];
        
        let mut all_tokens = Vec::new();
        let mut unique_addresses = HashSet::new();
        
        // Execute all three API calls sequentially to avoid borrowing issues
        for (sort_by, sort_type, description) in sort_strategies.iter() {
            debug!("📊 Fetching tokens sorted by {} ({})", sort_by, description);
            
            match self.fetch_trending_tokens_by_sort(chain, sort_by, sort_type).await {
                Ok(tokens) => {
                    info!("✅ Retrieved {} tokens sorted by {} ({})", 
                          tokens.len(), sort_by, description);
                    
                    for token in tokens {
                        // Only add if we haven't seen this token address before
                        if unique_addresses.insert(token.address.clone()) {
                            all_tokens.push(token);
                        }
                    }
                }
                Err(e) => {
                    warn!("❌ Failed to fetch tokens sorted by {} ({}): {}", sort_by, description, e);
                    // Continue with other strategies - don't fail the entire operation
                }
            }
            
            // Small delay between requests to avoid rate limiting
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        
        // Sort final result by volume for consistency
        all_tokens.sort_by(|a, b| {
            b.volume_24h.unwrap_or(0.0).partial_cmp(&a.volume_24h.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        
        info!("🎯 Multi-sort discovery completed: {} unique tokens discovered across all strategies", all_tokens.len());
        
        if self.config.api_base_url.contains("localhost") || std::env::var("DEBUG").is_ok() {
            debug!("📋 Token discovery breakdown:");
            for (i, token) in all_tokens.iter().enumerate().take(10) {
                debug!("  {}. {} ({}) - Vol: ${:.0}, Liq: ${:.0}", 
                       i + 1, token.symbol, token.address, 
                       token.volume_24h.unwrap_or(0.0),
                       token.liquidity.unwrap_or(0.0));
            }
        }
        
        Ok(all_tokens)
    }

    /// Helper method to fetch trending tokens by specific sort criteria
    async fn fetch_trending_tokens_by_sort(&self, chain: &str, sort_by: &str, sort_type: &str) -> Result<Vec<TrendingToken>, BirdEyeError> {
        let url = format!("{}/defi/token_trending", self.config.api_base_url);
        
        let response = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .query(&[
                ("chain", chain),
                ("sort_by", sort_by),
                ("sort_type", sort_type),
                ("offset", "0"),
                ("limit", "20"),
            ])
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

        Ok(trending_response.data.tokens)
    }

    /// Get trending tokens with multi-sort + pagination (3 strategies × 5 offsets = 15 calls) for comprehensive discovery
    pub async fn get_trending_tokens_paginated(&self, chain: &str) -> Result<Vec<TrendingToken>, BirdEyeError> {
        debug!("🔄 Starting paginated multi-sort trending token discovery for chain: {}", chain);
        
        // Define the three sorting strategies (preserve existing multi-sort functionality)
        let sort_strategies = [
            ("rank", "asc", "momentum/community interest"),
            ("volume24hUSD", "desc", "trading activity"),
            ("liquidity", "desc", "market depth"),
        ];
        
        // Define offsets for pagination
        let offsets = [0, 100, 200, 300, 400];
        
        let mut all_tokens = Vec::new();
        let mut unique_addresses = HashSet::new();
        
        // Execute all combinations: 3 strategies × 5 offsets = 15 API calls
        for (sort_by, sort_type, description) in sort_strategies.iter() {
            debug!("📊 Processing sort strategy: {} ({})", sort_by, description);
            
            for (i, offset) in offsets.iter().enumerate() {
                debug!("📊 Fetching {} tokens page {}/{} (offset: {})", sort_by, i + 1, offsets.len(), offset);
                
                match self.fetch_trending_tokens_by_sort_paginated(chain, sort_by, sort_type, *offset).await {
                    Ok(tokens) => {
                        info!("✅ Retrieved {} tokens from {} strategy page {} (offset: {})", 
                              tokens.len(), sort_by, i + 1, offset);
                        
                        for token in tokens {
                            // Only add if we haven't seen this token address before
                            if unique_addresses.insert(token.address.clone()) {
                                all_tokens.push(token);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("❌ Failed to fetch {} tokens at offset {}: {}", sort_by, offset, e);
                        // Continue with other pages - don't fail the entire operation
                    }
                }
                
                // Add delay between paginated calls to respect rate limits
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
        
        // Sort by volume for consistency
        all_tokens.sort_by(|a, b| {
            b.volume_24h.unwrap_or(0.0).partial_cmp(&a.volume_24h.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        
        info!("🎯 Paginated multi-sort discovery completed: {} unique tokens discovered across all strategies and pages", all_tokens.len());
        
        if self.config.api_base_url.contains("localhost") || std::env::var("DEBUG").is_ok() {
            debug!("📋 Top paginated multi-sort trending tokens:");
            for (i, token) in all_tokens.iter().enumerate().take(10) {
                debug!("  {}. {} ({}) - Vol: ${:.0}, Liq: ${:.0}", 
                       i + 1, token.symbol, token.address, 
                       token.volume_24h.unwrap_or(0.0),
                       token.liquidity.unwrap_or(0.0));
            }
        }
        
        Ok(all_tokens)
    }

    /// Helper method to fetch trending tokens by sort strategy + offset for pagination
    async fn fetch_trending_tokens_by_sort_paginated(&self, chain: &str, sort_by: &str, sort_type: &str, offset: u32) -> Result<Vec<TrendingToken>, BirdEyeError> {
        let url = format!("{}/defi/token_trending", self.config.api_base_url);
        
        let response = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .header("x-chain", chain)
            .header("accept", "application/json")
            .query(&[
                ("chain", chain),
                ("sort_by", sort_by),
                ("sort_type", sort_type),
                ("offset", &offset.to_string()),
                ("limit", "20"), // API enforced maximum
            ])
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

        Ok(trending_response.data.tokens)
    }

    /// Get top traders for a specific token
    pub async fn get_top_traders(&self, token_address: &str, chain: &str, limit: Option<u32>) -> Result<Vec<TopTrader>, BirdEyeError> {
        let url = format!("{}/defi/v2/tokens/top_traders", self.config.api_base_url);
        
        debug!("Fetching top traders from BirdEye for token: {}", token_address);
        
        let mut query_params = vec![
            ("address", token_address),
            ("time_frame", "24h"),
            ("sort_type", "desc"),
            ("sort_by", "volume"),
            ("offset", "0"),
        ];
        
        let limit_string = limit.unwrap_or(20).to_string();
        query_params.push(("limit", &limit_string));

        let response = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .header("x-chain", chain)
            .header("accept", "application/json")
            .query(&query_params)
            .send()
            .await?;

        if response.status() == 429 {
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_else(|_| "Failed to read error body".to_string());
            error!("BirdEye API error for top traders {}: HTTP {} - Body: {}", token_address, status, error_body);
            return Err(BirdEyeError::Api(format!("HTTP {} - {}", status, error_body)));
        }

        let top_traders_response: TopTradersResponse = response.json().await?;
        
        if !top_traders_response.success {
            return Err(BirdEyeError::Api("API returned success=false".to_string()));
        }

        info!("Retrieved {} top traders from BirdEye for token {}", 
              top_traders_response.data.items.len(), token_address);
        Ok(top_traders_response.data.items)
    }

    /// Get top traders for a specific token with pagination (offset 0-400, limit 10) for comprehensive discovery
    pub async fn get_top_traders_paginated(&self, token_address: &str, chain: &str) -> Result<Vec<TopTrader>, BirdEyeError> {
        debug!("🔄 Starting paginated top traders discovery for token: {}", token_address);
        
        let mut all_traders = Vec::new();
        let mut unique_addresses = HashSet::new();
        
        // Make 5 API calls with different offsets to get comprehensive coverage
        let offsets = [0, 100, 200, 300, 400];
        
        for (i, offset) in offsets.iter().enumerate() {
            debug!("📊 Fetching top traders page {}/{} (offset: {})", i + 1, offsets.len(), offset);
            
            match self.fetch_top_traders_paginated(token_address, chain, *offset).await {
                Ok(traders) => {
                    info!("✅ Retrieved {} top traders from page {} (offset: {})", traders.len(), i + 1, offset);
                    
                    for trader in traders {
                        // Only add if we haven't seen this trader address before
                        if unique_addresses.insert(trader.owner.clone()) {
                            all_traders.push(trader);
                        }
                    }
                }
                Err(e) => {
                    warn!("❌ Failed to fetch top traders at offset {}: {}", offset, e);
                    // Continue with other pages - don't fail the entire operation
                }
            }
            
            // Add delay between paginated calls to respect rate limits
            if i < offsets.len() - 1 {
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
        
        // Sort by volume descending for consistency
        all_traders.sort_by(|a, b| {
            b.volume.partial_cmp(&a.volume).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        info!("🎯 Paginated top traders discovery completed: {} unique traders discovered", all_traders.len());
        
        if self.config.api_base_url.contains("localhost") || std::env::var("DEBUG").is_ok() {
            debug!("📋 Top paginated traders:");
            for (i, trader) in all_traders.iter().enumerate().take(10) {
                debug!("  {}. {} - Vol: ${:.0}, Trades: {}", 
                       i + 1, trader.owner, trader.volume, trader.trade);
            }
        }
        
        Ok(all_traders)
    }

    /// Helper method to fetch top traders by offset for pagination
    async fn fetch_top_traders_paginated(&self, token_address: &str, chain: &str, offset: u32) -> Result<Vec<TopTrader>, BirdEyeError> {
        let url = format!("{}/defi/v2/tokens/top_traders", self.config.api_base_url);
        
        let response = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .header("x-chain", chain)
            .header("accept", "application/json")
            .query(&[
                ("address", token_address),
                ("time_frame", "24h"),
                ("sort_type", "desc"),
                ("sort_by", "volume"),
                ("offset", &offset.to_string()),
                ("limit", "10"), // API enforced maximum
            ])
            .send()
            .await?;

        if response.status() == 429 {
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_else(|_| "Failed to read error body".to_string());
            error!("BirdEye API error for top traders {}: HTTP {} - Body: {}", token_address, status, error_body);
            return Err(BirdEyeError::Api(format!("HTTP {} - {}", status, error_body)));
        }

        let top_traders_response: TopTradersResponse = response.json().await?;
        
        if !top_traders_response.success {
            return Err(BirdEyeError::Api("API returned success=false".to_string()));
        }

        Ok(top_traders_response.data.items)
    }

    /// Get top gainers/losers from BirdEye (filtered to only return gainers)
    pub async fn get_gainers_losers(&self, timeframe: &str, chain: &str) -> Result<Vec<GainerLoser>, BirdEyeError> {
        let url = format!("{}/trader/gainers-losers", self.config.api_base_url);
        
        debug!("Fetching gainers/losers from BirdEye for timeframe: {}", timeframe);
        
        let query_params = vec![
            ("type", timeframe),
            ("sort_by", "PnL"),
            ("sort_type", "desc"),
            ("offset", "0"),
            ("limit", "10"),
        ];

        let response = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .header("x-chain", chain)
            .header("accept", "application/json")
            .query(&query_params)
            .send()
            .await?;

        if response.status() == 429 {
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_else(|_| "Failed to read error body".to_string());
            error!("BirdEye API error for gainers/losers {}: HTTP {} - Body: {}", timeframe, status, error_body);
            return Err(BirdEyeError::Api(format!("HTTP {} - {}", status, error_body)));
        }

        let gainers_losers_response: GainersLosersResponse = response.json().await?;
        
        if !gainers_losers_response.success {
            return Err(BirdEyeError::Api("API returned success=false".to_string()));
        }

        // Filter to only return gainers (pnl > 0)
        let total_count = gainers_losers_response.data.items.len();
        let gainers_only: Vec<GainerLoser> = gainers_losers_response.data.items
            .into_iter()
            .filter(|trader| trader.pnl > 0.0)
            .collect();

        info!("Retrieved {} gainers from BirdEye for timeframe {} (filtered from {} total)", 
              gainers_only.len(), timeframe, total_count);
        Ok(gainers_only)
    }

    /// Get gainers/losers with multi-timeframe + pagination (3 timeframes × 5 offsets = 15 calls) for comprehensive discovery
    pub async fn get_gainers_losers_paginated(&self, chain: &str) -> Result<Vec<GainerLoser>, BirdEyeError> {
        debug!("🔄 Starting paginated multi-timeframe gainers/losers discovery");
        
        // Define the three timeframes (preserve existing multi-timeframe functionality)
        let timeframes = ["1W", "yesterday", "today"];
        
        // Define offsets for pagination
        let offsets = [0, 100, 200, 300, 400];
        
        let mut all_gainers = Vec::new();
        let mut unique_addresses = HashSet::new();
        
        // Execute all combinations: 3 timeframes × 5 offsets = 15 API calls
        for timeframe in timeframes.iter() {
            debug!("📊 Processing timeframe: {}", timeframe);
            
            for (i, offset) in offsets.iter().enumerate() {
                debug!("📊 Fetching {} gainers page {}/{} (offset: {})", timeframe, i + 1, offsets.len(), offset);
                
                match self.fetch_gainers_losers_paginated(timeframe, chain, *offset).await {
                    Ok(gainers) => {
                        info!("✅ Retrieved {} gainers from {} timeframe page {} (offset: {})", 
                              gainers.len(), timeframe, i + 1, offset);
                        
                        for gainer in gainers {
                            // Only add if we haven't seen this wallet address before
                            if unique_addresses.insert(gainer.address.clone()) {
                                all_gainers.push(gainer);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("❌ Failed to fetch {} gainers at offset {}: {}", timeframe, offset, e);
                        // Continue with other pages - don't fail the entire operation
                    }
                }
                
                // Add delay between paginated calls to respect rate limits
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
        
        // Sort by PnL descending for consistency
        all_gainers.sort_by(|a, b| {
            b.pnl.partial_cmp(&a.pnl).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        info!("🎯 Paginated multi-timeframe gainers discovery completed: {} unique gainers discovered across all timeframes and pages", all_gainers.len());
        
        if self.config.api_base_url.contains("localhost") || std::env::var("DEBUG").is_ok() {
            debug!("📋 Top paginated multi-timeframe gainers:");
            for (i, gainer) in all_gainers.iter().enumerate().take(10) {
                debug!("  {}. {} - PnL: ${:.2}, Vol: ${:.0}, Trades: {}", 
                       i + 1, gainer.address, gainer.pnl, gainer.volume, gainer.trade_count);
            }
        }
        
        Ok(all_gainers)
    }

    /// Helper method to fetch gainers/losers by offset for pagination
    async fn fetch_gainers_losers_paginated(&self, timeframe: &str, chain: &str, offset: u32) -> Result<Vec<GainerLoser>, BirdEyeError> {
        let url = format!("{}/trader/gainers-losers", self.config.api_base_url);
        
        let response = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .header("x-chain", chain)
            .header("accept", "application/json")
            .query(&[
                ("type", timeframe),
                ("sort_by", "PnL"),
                ("sort_type", "desc"),
                ("offset", &offset.to_string()),
                ("limit", "10"), // API enforced maximum
            ])
            .send()
            .await?;

        if response.status() == 429 {
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_else(|_| "Failed to read error body".to_string());
            error!("BirdEye API error for gainers/losers {}: HTTP {} - Body: {}", timeframe, status, error_body);
            return Err(BirdEyeError::Api(format!("HTTP {} - {}", status, error_body)));
        }

        let gainers_losers_response: GainersLosersResponse = response.json().await?;
        
        if !gainers_losers_response.success {
            return Err(BirdEyeError::Api("API returned success=false".to_string()));
        }

        // Filter to only return gainers (pnl > 0)
        let gainers_only: Vec<GainerLoser> = gainers_losers_response.data.items
            .into_iter()
            .filter(|trader| trader.pnl > 0.0)
            .collect();

        Ok(gainers_only)
    }

    /// Get newly listed tokens from legitimate platforms only (excludes meme platforms for security)
    pub async fn get_new_listing_tokens_comprehensive(&self, chain: &str) -> Result<Vec<NewListingToken>, BirdEyeError> {
        let limit = 20;
        
        debug!("🆕 Starting secure new listing token discovery for chain: {} (legitimate platforms only)", chain);
        
        // Only fetch from non-meme platforms to avoid tokens with security issues (locked liquidity, etc.)
        let legitimate_tokens = self.get_new_listing_tokens(chain, limit, false).await?;
        
        info!("🎯 Secure new listing discovery completed: {} tokens from legitimate platforms only (meme platforms excluded for security)", 
              legitimate_tokens.len());
        
        Ok(legitimate_tokens)
    }

    /// Get newly listed tokens from BirdEye API (single call)
    async fn get_new_listing_tokens(&self, chain: &str, limit: u32, meme_platform_enabled: bool) -> Result<Vec<NewListingToken>, BirdEyeError> {
        let url = format!("{}/defi/v2/tokens/new_listing", self.config.api_base_url);
        
        debug!("📡 Fetching new listing tokens from BirdEye (meme_platform_enabled: {}, limit: {}) - secure discovery", 
               meme_platform_enabled, limit);
        
        let response = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .header("x-chain", chain)
            .query(&[
                ("limit", &limit.to_string()),
                ("meme_platform_enabled", &meme_platform_enabled.to_string()),
            ])
            .send()
            .await?;

        if response.status() == 429 {
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_else(|_| "Failed to read error body".to_string());
            error!("BirdEye API error for new listing tokens: HTTP {} - Body: {}", status, error_body);
            return Err(BirdEyeError::Api(format!("HTTP {} - {}", status, error_body)));
        }

        let new_listing_response: NewListingTokenResponse = response.json().await?;
        
        if !new_listing_response.success {
            return Err(BirdEyeError::Api("API returned success=false".to_string()));
        }

        debug!("✅ Retrieved {} new listing tokens (meme_platform_enabled: {}) - legitimate platforms only", 
               new_listing_response.data.items.len(), meme_platform_enabled);
        
        Ok(new_listing_response.data.items)
    }

    /// Filter new listing tokens based on quality criteria
    pub fn filter_new_listing_tokens(&self, tokens: Vec<NewListingToken>, filter: &NewListingTokenFilter) -> Vec<NewListingToken> {
        let original_count = tokens.len();
        let mut filtered_tokens: Vec<NewListingToken> = tokens
            .into_iter()
            .filter(|token| {
                // Liquidity filter
                if let Some(min_liquidity) = filter.min_liquidity {
                    if token.liquidity < min_liquidity {
                        debug!("⭕ Filtered out {} due to low liquidity: ${:.2}", token.symbol, token.liquidity);
                        return false;
                    }
                }
                
                // Age filter (check if token was added within max_age_hours)
                if let Some(max_age_hours) = filter.max_age_hours {
                    if let Ok(added_at) = chrono::DateTime::parse_from_rfc3339(&token.liquidity_added_at) {
                        let age_hours = chrono::Utc::now().signed_duration_since(added_at).num_hours();
                        if age_hours > max_age_hours as i64 {
                            debug!("⭕ Filtered out {} due to age: {} hours old", token.symbol, age_hours);
                            return false;
                        }
                    }
                }
                
                // Source exclusion filter
                if let Some(ref exclude_sources) = filter.exclude_sources {
                    if exclude_sources.contains(&token.source) {
                        debug!("⭕ Filtered out {} due to excluded source: {}", token.symbol, token.source);
                        return false;
                    }
                }
                
                true
            })
            .collect();
        
        // Sort by liquidity descending
        filtered_tokens.sort_by(|a, b| b.liquidity.partial_cmp(&a.liquidity).unwrap_or(std::cmp::Ordering::Equal));
        
        // Apply max tokens limit
        if let Some(max_tokens) = filter.max_tokens {
            if filtered_tokens.len() > max_tokens {
                filtered_tokens.truncate(max_tokens);
            }
        }
        
        info!("🔍 Filtered {} new listing tokens to {} quality tokens", 
              original_count, filtered_tokens.len());
        
        if self.config.api_base_url.contains("localhost") || std::env::var("DEBUG").is_ok() {
            debug!("📋 Top new listing tokens after filtering:");
            for (i, token) in filtered_tokens.iter().enumerate().take(5) {
                debug!("  {}. {} ({}) - Liquidity: ${:.2}, Source: {}", 
                       i + 1, token.symbol, token.address, token.liquidity, token.source);
            }
        }
        
        filtered_tokens
    }

    /// Get all trader transactions for a wallet (general endpoint without token filter)
    pub async fn get_all_trader_transactions(
        &self,
        wallet_address: &str,
        chain: &str,
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

        let request = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .header("x-chain", chain)
            .query(&query_params);
            
        debug!("Making BirdEye API request to: {} with params: {:?}", url, query_params);
        
        let response = request.send().await?;

        debug!("BirdEye API response status: {} for wallet: {}", response.status(), wallet_address);

        if response.status() == 429 {
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_else(|_| "Failed to read error body".to_string());
            error!("BirdEye API error for wallet {}: HTTP {} - Body: {}", wallet_address, status, error_body);
            return Err(BirdEyeError::Api(format!("HTTP {} - {}", status, error_body)));
        }

        let response_text = response.text().await?;
        debug!("Raw BirdEye API response for wallet {}: {}", wallet_address, response_text);
        
        let general_txs_response: GeneralTraderTransactionsResponse = serde_json::from_str(&response_text)
            .map_err(|e| BirdEyeError::Api(format!("JSON parsing error: {}", e)))?;
        
        if !general_txs_response.success {
            return Err(BirdEyeError::Api("API returned success=false".to_string()));
        }

        debug!("Retrieved {} general transactions from BirdEye for wallet {}", 
               general_txs_response.data.items.len(), wallet_address);
        Ok(general_txs_response.data.items)
    }

    /// Get all trader transactions for a wallet with pagination (fetches all available transactions)
    pub async fn get_all_trader_transactions_paginated(
        &self,
        wallet_address: &str,
        chain: &str,
        from_time: Option<i64>,
        to_time: Option<i64>,
        max_total_transactions: u32,
    ) -> Result<Vec<GeneralTraderTransaction>, BirdEyeError> {
        let mut all_transactions = Vec::new();
        let mut offset = 0u32;
        let limit = 100u32; // Maximum allowed by BirdEye API
        let max_offset_limit = 10000u32; // BirdEye API constraint: offset + limit <= 10000
        
        debug!("Starting paginated fetch for wallet {} with max_total_transactions: {}", 
               wallet_address, max_total_transactions);
        
        loop {
            // Check BirdEye API constraint: offset + limit <= 10000
            if offset + limit > max_offset_limit {
                warn!("Reached BirdEye API constraint (offset + limit > 10000) for wallet {} at offset {}", 
                     wallet_address, offset);
                break;
            }
            
            // Check if we've fetched enough transactions
            if all_transactions.len() >= max_total_transactions as usize {
                debug!("Reached max_total_transactions limit ({}) for wallet {}", 
                       max_total_transactions, wallet_address);
                break;
            }
            
            // Calculate how many transactions we still need
            let remaining_needed = max_total_transactions.saturating_sub(all_transactions.len() as u32);
            let current_limit = std::cmp::min(limit, remaining_needed);
            
            // Ensure we don't violate the API constraint with the current limit
            let adjusted_limit = if offset + current_limit > max_offset_limit {
                max_offset_limit - offset
            } else {
                current_limit
            };
            
            if adjusted_limit == 0 {
                debug!("Cannot make request with limit 0, stopping pagination for wallet {}", wallet_address);
                break;
            }
            
            debug!("Fetching batch for wallet {} (offset: {}, limit: {})", 
                   wallet_address, offset, adjusted_limit);
            
            // Make the API call with offset and retry logic
            let (batch_transactions, has_next) = match self.get_all_trader_transactions_with_offset_retry(
                wallet_address,
                chain,
                from_time,
                to_time,
                Some(adjusted_limit),
                Some(offset),
            ).await {
                Ok(result) => result,
                Err(e) => {
                    warn!("Failed to fetch batch for wallet {} at offset {}: {}. Continuing with next batch.", 
                          wallet_address, offset, e);
                    // Continue pagination despite this batch failure
                    (Vec::new(), true)
                }
            };
            
            let batch_size = batch_transactions.len();
            debug!("Retrieved {} transactions in batch for wallet {} (has_next: {})", 
                   batch_size, wallet_address, has_next);
            
            // If no transactions returned, we've reached the end
            if batch_size == 0 {
                debug!("No more transactions available for wallet {}", wallet_address);
                break;
            }
            
            // Add to our collection
            all_transactions.extend(batch_transactions);
            
            // Check has_next field from API response - this is the reliable indicator
            if !has_next {
                debug!("API indicates no more transactions available (has_next=false) for wallet {}", wallet_address);
                break;
            }
            
            // Move to next page by incrementing offset by the standard limit size (100)
            // This follows standard offset pagination: offset=0,100,200,300...
            // Always use the standard limit, not adjusted_limit, to maintain consistent pagination
            offset += limit;
            
            // Respect BirdEye rate limit: 10 req/sec = 100ms between requests (conservative)
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
        
        info!("Completed paginated fetch for wallet {}: {} total transactions", 
              wallet_address, all_transactions.len());
        
        Ok(all_transactions)
    }

    /// Private helper function that includes offset parameter
    /// Returns tuple of (transactions, has_next)
    async fn get_all_trader_transactions_with_offset(
        &self,
        wallet_address: &str,
        chain: &str,
        from_time: Option<i64>,
        to_time: Option<i64>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<(Vec<GeneralTraderTransaction>, bool), BirdEyeError> {
        let url = format!("{}/trader/txs/seek_by_time", self.config.api_base_url);
        
        let mut query_params = vec![
            ("address", wallet_address),
        ];
        
        let from_string;
        let to_string;
        let limit_string;
        let offset_string;
        
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
        if let Some(offset_val) = offset {
            offset_string = offset_val.to_string();
            query_params.push(("offset", &offset_string));
        }

        let request = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .header("x-chain", chain)
            .query(&query_params);
            
        info!("🔄 Making BirdEye API request to: {} with params: {:?}", url, query_params);
        
        let response = request.send().await?;

        info!("📡 BirdEye API response status: {} for wallet: {}", response.status(), wallet_address);

        if response.status() == 429 {
            error!("🚫 Rate limit hit (429) for wallet {}", wallet_address);
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_else(|_| "Failed to read error body".to_string());
            error!("❌ BirdEye API error for wallet {}: HTTP {} - Body: {}", wallet_address, status, error_body);
            return Err(BirdEyeError::Api(format!("HTTP {} - {}", status, error_body)));
        }

        let response_text = response.text().await?;
        
        info!("📄 BirdEye API response size: {} bytes for wallet: {}", response_text.len(), wallet_address);
        
        // Log first and last 100 chars to debug JSON corruption
        let preview_start = if response_text.len() > 100 {
            &response_text[..100]
        } else {
            &response_text
        };
        let preview_end = if response_text.len() > 200 {
            &response_text[response_text.len()-100..]
        } else {
            ""
        };
        
        info!("📄 Response preview - Start: {}...", preview_start.replace('\n', " "));
        if !preview_end.is_empty() {
            info!("📄 Response preview - End: ...{}", preview_end.replace('\n', " "));
        }
        
        let general_txs_response: GeneralTraderTransactionsResponse = serde_json::from_str(&response_text)
            .map_err(|e| {
                error!("💥 JSON parsing failed for wallet {}: {}", wallet_address, e);
                error!("💥 Response length: {} bytes", response_text.len());
                
                BirdEyeError::Api(format!("JSON parsing error: {}", e))
            })?;
        
        if !general_txs_response.success {
            error!("❌ BirdEye API returned success=false for wallet {}", wallet_address);
            return Err(BirdEyeError::Api("API returned success=false".to_string()));
        }

        let has_next = general_txs_response.data.has_next.unwrap_or(false);
        let items_count = general_txs_response.data.items.len();
        
        info!("✅ Successfully parsed {} transactions, has_next={} for wallet {}", 
              items_count, has_next, wallet_address);
        
        Ok((general_txs_response.data.items, has_next))
    }

    /// Retry wrapper with exponential backoff for rate limit handling
    async fn get_all_trader_transactions_with_offset_retry(
        &self,
        wallet_address: &str,
        chain: &str,
        from_time: Option<i64>,
        to_time: Option<i64>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<(Vec<GeneralTraderTransaction>, bool), BirdEyeError> {
        let max_retries = 3;
        let mut attempt: u32 = 0;
        
        loop {
            attempt += 1;
            
            match self.get_all_trader_transactions_with_offset(
                wallet_address, chain, from_time, to_time, limit, offset
            ).await {
                Ok(result) => return Ok(result),
                Err(BirdEyeError::RateLimit) if attempt <= max_retries => {
                    let delay_ms = 1000 * attempt.pow(2); // Exponential backoff: 1s, 4s, 9s
                    warn!("Rate limit hit for wallet {} (attempt {}), retrying in {}ms", 
                          wallet_address, attempt, delay_ms);
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms as u64)).await;
                    continue;
                }
                Err(BirdEyeError::Api(ref msg)) if msg.contains("JSON parsing error") && attempt <= max_retries => {
                    let delay_ms = 500 * attempt; // Linear backoff for JSON errors: 500ms, 1s, 1.5s
                    warn!("JSON parsing error for wallet {} (attempt {}): {}. Retrying in {}ms", 
                          wallet_address, attempt, msg, delay_ms);
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms as u64)).await;
                    continue;
                }
                Err(e) => {
                    if attempt > max_retries {
                        error!("Max retries exceeded for wallet {} at offset {:?}: {}", 
                               wallet_address, offset, e);
                    }
                    return Err(e);
                }
            }
        }
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
    pub async fn get_current_price(&self, token_address: &str, chain: &str) -> Result<f64, BirdEyeError> {
        let url = format!("{}/defi/price", self.config.api_base_url);
        
        debug!("Fetching current price from BirdEye for token: {}", token_address);
        
        let response = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .header("x-chain", chain)
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

    /// Get current prices for multiple tokens (with batching to avoid HTTP 414)
    pub async fn get_current_prices(&self, token_addresses: &[String], chain: &str) -> Result<HashMap<String, f64>, BirdEyeError> {
        const BATCH_SIZE: usize = 50;  // Conservative batch size to avoid HTTP 414
        
        if token_addresses.is_empty() {
            return Ok(HashMap::new());
        }
        
        // If small enough, use single request (optimization)
        if token_addresses.len() <= BATCH_SIZE {
            return self.fetch_price_batch(token_addresses, chain).await;
        }
        
        // For larger lists, batch the requests
        let mut all_prices = HashMap::new();
        let chunks: Vec<_> = token_addresses.chunks(BATCH_SIZE).collect();
        
        info!("Fetching current prices for {} tokens in {} batches of up to {} tokens each", 
              token_addresses.len(), chunks.len(), BATCH_SIZE);
        
        for (i, chunk) in chunks.iter().enumerate() {
            debug!("Fetching price batch {}/{} ({} tokens)", i + 1, chunks.len(), chunk.len());
            
            match self.fetch_price_batch(&chunk.to_vec(), chain).await {
                Ok(batch_prices) => {
                    debug!("Successfully fetched {} prices in batch {}", batch_prices.len(), i + 1);
                    all_prices.extend(batch_prices);
                }
                Err(e) => {
                    warn!("Failed to fetch prices for batch {}/{}: {}", i + 1, chunks.len(), e);
                    // Continue with other batches instead of failing entirely
                }
            }
            
            // Add delay between batches to avoid rate limiting (except for last batch)
            if i < chunks.len() - 1 {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        }
        
        info!("Completed batch price fetching: {}/{} tokens have prices", 
              all_prices.len(), token_addresses.len());
        
        Ok(all_prices)
    }
    
    /// Helper method to fetch prices for a single batch of tokens
    async fn fetch_price_batch(&self, token_addresses: &[String], chain: &str) -> Result<HashMap<String, f64>, BirdEyeError> {
        let url = format!("{}/defi/multi_price", self.config.api_base_url);
        
        debug!("Fetching price batch from BirdEye for {} tokens", token_addresses.len());
        
        let address_list = token_addresses.join(",");
        
        let response = self.http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .header("x-chain", chain)
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

        debug!("Retrieved prices from BirdEye batch for {}/{} tokens", 
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

    /// Consolidate raw Birdeye transactions by tx_hash into net effects
    /// This is the critical function that fixes the P&L calculation accuracy
    pub fn consolidate_transactions_by_hash(
        &self,
        raw_transactions: Vec<GeneralTraderTransaction>,
        wallet_address: String,
    ) -> Vec<ConsolidatedTransaction> {
        let mut consolidated_map: HashMap<String, ConsolidatedTransaction> = HashMap::new();
        let raw_tx_count = raw_transactions.len();
        
        debug!("Consolidating {} raw transactions by tx_hash for wallet {}", 
               raw_tx_count, wallet_address);
        
        for tx in raw_transactions {
            let entry = consolidated_map.entry(tx.tx_hash.clone()).or_insert_with(|| {
                ConsolidatedTransaction {
                    tx_hash: tx.tx_hash.clone(),
                    block_unix_time: tx.block_unix_time,
                    net_token_changes: HashMap::new(),
                    total_volume_usd: 0.0,
                    source: tx.source.clone(),
                    wallet_address: wallet_address.clone(),
                }
            });
            
            // Add volume to total
            entry.total_volume_usd += tx.volume_usd;
            
            // Process quote side
            self.process_token_side(
                &tx.quote,
                &mut entry.net_token_changes,
                &tx.tx_hash,
            );
            
            // Process base side
            self.process_token_side(
                &tx.base,
                &mut entry.net_token_changes,
                &tx.tx_hash,
            );
        }
        
        let mut consolidated_transactions: Vec<ConsolidatedTransaction> = consolidated_map.into_values().collect();
        
        // Sort by block time
        consolidated_transactions.sort_by_key(|tx| tx.block_unix_time);
        
        debug!("Consolidated {} raw transactions into {} net transactions for wallet {}", 
               raw_tx_count, consolidated_transactions.len(), wallet_address);
        
        consolidated_transactions
    }
    
    /// Process a single token side (quote or base) and update net changes
    fn process_token_side(
        &self,
        token_side: &TokenTransactionSide,
        net_changes: &mut HashMap<String, ConsolidatedTokenChange>,
        tx_hash: &str,
    ) {
        let token_address = token_side.address.clone();
        
        // Get or create the net change entry for this token
        let net_change = net_changes.entry(token_address.clone()).or_insert_with(|| {
            ConsolidatedTokenChange {
                symbol: token_side.symbol.clone(),
                address: token_address.clone(),
                net_ui_amount: 0.0,
                net_raw_amount: 0,
                decimals: token_side.decimals,
                usd_value: 0.0,
                price_per_token: token_side.price.unwrap_or(0.0),
            }
        });
        
        // Add the UI amount change (this is the critical net calculation)
        net_change.net_ui_amount += token_side.ui_change_amount;
        net_change.net_raw_amount += token_side.change_amount;
        
        // Calculate USD value based on the change
        let token_price = token_side.price.unwrap_or(0.0);
        let usd_change = token_side.ui_change_amount * token_price;
        net_change.usd_value += usd_change;
        
        // Update price if we have a more recent one
        if token_price > 0.0 {
            net_change.price_per_token = token_price;
        }
        
        debug!("Token {} in tx {}: ui_change={}, usd_change={}, net_ui={}, net_usd={}", 
               token_side.symbol, tx_hash, token_side.ui_change_amount, usd_change, 
               net_change.net_ui_amount, net_change.usd_value);
    }
    
    // LEGACY FUNCTION REMOVED - consolidated_to_financial_events()
    // This function converted transactions to legacy FinancialEvents
    // Now we use NewTransactionParser with GeneralTraderTransaction directly

    /// Get wallet transaction history from BirdEye transaction history API
    /// This replaces the old swap-only API and captures both swaps and send transactions
    pub async fn get_wallet_transaction_history(
        &self,
        wallet: &str,
        chain: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<WalletTransaction>, BirdEyeError> {
        let chain = chain.unwrap_or("solana");
        let limit = limit.unwrap_or(1000); // Default limit

        debug!("Fetching transaction history for wallet: {} on chain: {}", wallet, chain);

        // Use the new fallback helper method
        let transactions = self.try_fetch_transactions_with_fallback(
            wallet, chain, limit, None
        ).await?;
        
        if transactions.is_empty() {
            info!(
                "📭 No transactions found for wallet {} on chain {} (tried both ui_amount_mode options)",
                wallet,
                chain
            );
        } else {
            info!(
                "Retrieved {} transactions for wallet {} on chain {}",
                transactions.len(),
                wallet,
                chain
            );
        }

        Ok(transactions)
    }

    /// Get wallet transaction history with pagination support
    pub async fn get_wallet_transaction_history_paginated(
        &self,
        wallet: &str,
        chain: Option<&str>,
        limit: Option<u32>,
        before: Option<&str>,
    ) -> Result<Vec<WalletTransaction>, BirdEyeError> {
        let chain = chain.unwrap_or("solana");
        let limit = limit.unwrap_or(100); // Smaller default for paginated requests

        info!(
            "🔄 Fetching paginated transaction history for wallet: {} on chain: {} (limit: {}, before: {:?})",
            wallet, chain, limit, before
        );

        // Use the new fallback helper method
        let transactions = self.try_fetch_transactions_with_fallback(
            wallet, chain, limit, before
        ).await?;
        
        if transactions.is_empty() {
            info!(
                "📭 No transactions found for wallet {} on chain {} (tried both ui_amount_mode options)",
                wallet,
                chain
            );
        } else {
            info!(
                "✅ Successfully retrieved {} paginated transactions for wallet {} on chain {}",
                transactions.len(),
                wallet,
                chain
            );
        }

        Ok(transactions)
    }

    /// Private helper method to try fetching transactions with ui_amount_mode fallback
    /// Tries 'scaled' first, then 'raw' if no transactions are returned
    async fn try_fetch_transactions_with_fallback(
        &self,
        wallet: &str,
        chain: &str,
        limit: u32,
        before: Option<&str>,
    ) -> Result<Vec<WalletTransaction>, BirdEyeError> {
        let url = format!("{}/v1/wallet/tx_list", self.config.api_base_url);
        let limit_string = limit.to_string();
        
        // Try 'scaled' mode first (most common)
        let result = self.try_fetch_with_ui_mode(
            &url, wallet, chain, &limit_string, before, "scaled"
        ).await;
        
        match result {
            Ok(transactions) if !transactions.is_empty() => {
                info!("✅ Found {} transactions using ui_amount_mode=scaled for wallet {}", 
                      transactions.len(), wallet);
                Ok(transactions)
            },
            Ok(_empty_transactions) => {
                info!("📭 ui_amount_mode=scaled returned empty, trying ui_amount_mode=raw for wallet {}", wallet);
                
                // Try 'raw' mode as fallback
                let fallback_result = self.try_fetch_with_ui_mode(
                    &url, wallet, chain, &limit_string, before, "raw"
                ).await;
                
                match fallback_result {
                    Ok(transactions) if !transactions.is_empty() => {
                        info!("✅ Found {} transactions using ui_amount_mode=raw (fallback) for wallet {}", 
                              transactions.len(), wallet);
                        Ok(transactions)
                    },
                    Ok(_empty_transactions) => {
                        info!("📭 Both ui_amount_mode modes returned empty for wallet {}", wallet);
                        Ok(vec![]) // Return empty vector, not an error
                    },
                    Err(e) => {
                        warn!("❌ ui_amount_mode=raw fallback failed for wallet {}: {}", wallet, e);
                        Err(e)
                    }
                }
            },
            Err(e) => {
                warn!("❌ ui_amount_mode=scaled failed for wallet {}, trying raw mode: {}", wallet, e);
                
                // Try 'raw' mode as fallback even on error
                let fallback_result = self.try_fetch_with_ui_mode(
                    &url, wallet, chain, &limit_string, before, "raw"
                ).await;
                
                match fallback_result {
                    Ok(transactions) => {
                        info!("✅ Found {} transactions using ui_amount_mode=raw (error fallback) for wallet {}", 
                              transactions.len(), wallet);
                        Ok(transactions)
                    },
                    Err(_) => {
                        error!("💥 Both ui_amount_mode modes failed for wallet {}", wallet);
                        Err(e) // Return original error
                    }
                }
            }
        }
    }

    /// Private helper to make a single API request with specific ui_amount_mode
    async fn try_fetch_with_ui_mode(
        &self,
        url: &str,
        wallet: &str,
        chain: &str,
        limit_string: &str,
        before: Option<&str>,
        ui_amount_mode: &str,
    ) -> Result<Vec<WalletTransaction>, BirdEyeError> {
        let mut query_params = vec![
            ("wallet", wallet),
            ("ui_amount_mode", ui_amount_mode),
            ("limit", limit_string),
        ];

        let before_string;
        if let Some(before_cursor) = before {
            before_string = before_cursor.to_string();
            query_params.push(("before", &before_string));
        }

        debug!("🔍 Trying BirdEye API request with ui_amount_mode={}:", ui_amount_mode);
        debug!("  📡 URL: {}", url);
        debug!("  📋 Query params: {:?}", query_params);
        
        let request_start = std::time::Instant::now();
        let response = self
            .http_client
            .get(url)
            .header("X-API-KEY", &self.config.api_key)
            .header("x-chain", chain)
            .header("accept", "application/json")
            .query(&query_params)
            .send()
            .await?;
            
        let _request_duration = request_start.elapsed();

        if response.status() == 429 {
            error!("🚫 Rate limit hit (429) for wallet {} with ui_amount_mode={}", wallet, ui_amount_mode);
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            debug!("❌ BirdEye API error with ui_amount_mode={}: HTTP {} - {}", ui_amount_mode, status, error_text);
            return Err(BirdEyeError::Api(format!("HTTP {} - {}", status, error_text)));
        }

        let response_text = response.text().await?;
        debug!("📏 Response body length: {} bytes (ui_amount_mode={})", response_text.len(), ui_amount_mode);
        
        let history_response: WalletTransactionHistoryResponse = serde_json::from_str(&response_text)
            .map_err(|e| {
                debug!("💥 JSON deserialization failed with ui_amount_mode={}: {}", ui_amount_mode, e);
                BirdEyeError::InvalidResponse(format!("JSON parse error for wallet {} (mode={}): {}", wallet, ui_amount_mode, e))
            })?;

        if !history_response.success {
            debug!("❌ BirdEye API returned success=false for wallet {} with ui_amount_mode={}", wallet, ui_amount_mode);
            return Err(BirdEyeError::Api(format!("API returned success=false (mode={})", ui_amount_mode)));
        }

        let transactions = history_response.data.solana;
        debug!("📊 Parsed {} transactions with ui_amount_mode={} for wallet {}", transactions.len(), ui_amount_mode, wallet);
        
        Ok(transactions)
    }

    /// Get current portfolio (token balances with prices) for a wallet
    /// This provides real-time token balances and current USD values for accurate unrealized P&L calculation
    pub async fn get_wallet_portfolio(
        &self,
        wallet: &str,
        chain: Option<&str>,
    ) -> Result<Vec<WalletTokenBalance>, BirdEyeError> {
        let url = format!("{}/v1/wallet/token_list", self.config.api_base_url);
        let chain = chain.unwrap_or("solana");

        debug!("Fetching current portfolio for wallet: {} on chain: {}", wallet, chain);

        let response = self
            .http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .header("x-chain", chain)
            .query(&[
                ("wallet", wallet),
                ("ui_amount_mode", "scaled"),
            ])
            .send()
            .await?;

        if response.status() == 429 {
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(BirdEyeError::Api(format!("HTTP {}: {}", status, error_text)));
        }

        let portfolio_response: WalletPortfolioResponse = response.json().await?;

        if !portfolio_response.success {
            return Err(BirdEyeError::Api(
                "Portfolio API returned success=false".to_string(),
            ));
        }

        let portfolio_tokens = portfolio_response.data.items;
        
        info!(
            "📊 Fetched portfolio for wallet {}: {} tokens with total value ${:.2}",
            wallet,
            portfolio_tokens.len(),
            portfolio_tokens.iter().map(|t| t.value_usd).sum::<f64>()
        );

        Ok(portfolio_tokens)
    }

    /// Get historical price for a token at a specific Unix timestamp
    pub async fn get_historical_price_unix(
        &self,
        token_address: &str,
        unix_time: i64,
        chain: Option<&str>,
    ) -> Result<f64, BirdEyeError> {
        let url = format!("{}/defi/historical_price_unix", self.config.api_base_url);
        let chain = chain.unwrap_or("solana");

        debug!(
            "Fetching historical price for token: {} at unix time: {} on chain: {}",
            token_address, unix_time, chain
        );

        let response = self
            .http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .query(&[
                ("address", token_address),
                ("unixtime", &unix_time.to_string()),
                ("chain", chain),
            ])
            .send()
            .await?;

        if response.status() == 429 {
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(BirdEyeError::Api(format!(
                "Historical price API HTTP {}: {}",
                status, error_text
            )));
        }

        let price_response: HistoricalPriceResponse = response.json().await?;

        if !price_response.success {
            return Err(BirdEyeError::Api(
                "Historical price API returned success=false".to_string(),
            ));
        }

        let price = price_response.data.value;
        debug!(
            "Retrieved historical price for token {}: ${} at unix time {}",
            token_address, price, unix_time
        );

        Ok(price)
    }

    /// Get current prices for multiple tokens in a single batch request
    pub async fn get_multi_price(
        &self,
        token_addresses: &[String],
        chain: Option<&str>,
    ) -> Result<HashMap<String, f64>, BirdEyeError> {
        if token_addresses.is_empty() {
            return Ok(HashMap::new());
        }

        let url = format!("{}/defi/multi_price", self.config.api_base_url);
        let chain = chain.unwrap_or("solana");
        let addresses_param = token_addresses.join(",");

        debug!(
            "Fetching current prices for {} tokens on chain: {}",
            token_addresses.len(),
            chain
        );

        let response = self
            .http_client
            .get(&url)
            .header("X-API-KEY", &self.config.api_key)
            .query(&[("list_address", addresses_param.as_str()), ("chain", chain)])
            .send()
            .await?;

        if response.status() == 429 {
            return Err(BirdEyeError::RateLimit);
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(BirdEyeError::Api(format!(
                "Multi-price API HTTP {}: {}",
                status, error_text
            )));
        }

        let multi_price_response: MultiPriceResponse = response.json().await?;

        if !multi_price_response.success {
            return Err(BirdEyeError::Api(
                "Multi-price API returned success=false".to_string(),
            ));
        }

        let mut prices = HashMap::new();
        for (address, price_data) in multi_price_response.data {
            prices.insert(address, price_data.value);
        }

        info!(
            "Retrieved current prices for {}/{} tokens on chain {}",
            prices.len(),
            token_addresses.len(),
            chain
        );

        Ok(prices)
    }

    /// Get wallet transaction history with full pagination support
    /// Fetches complete transaction history by chaining requests using transaction hashes
    pub async fn get_wallet_transaction_history_with_full_pagination(
        &self,
        wallet: &str,
        chain: Option<&str>,
        max_transactions: u32,
    ) -> Result<Vec<WalletTransaction>, BirdEyeError> {
        let chain = chain.unwrap_or("solana");
        let page_size = 100u32; // BirdEye API limit per request
        let pagination_delay_ms = 150; // Rate limiting delay between requests
        
        info!("🔄 Starting paginated fetch for wallet {} (max: {} transactions)", wallet, max_transactions);
        
        let mut all_transactions = Vec::new();
        let mut before_hash: Option<String> = None;
        let mut page_number = 1;
        
        while all_transactions.len() < max_transactions as usize {
            // Calculate remaining transactions needed
            let remaining = max_transactions.saturating_sub(all_transactions.len() as u32);
            let current_limit = std::cmp::min(page_size, remaining);
            
            info!("📄 Fetching page {} for wallet {} (limit: {}, before: {:?})", 
                   page_number, wallet, current_limit, before_hash);
            
            // Enhanced request comparison logging
            debug!("🔄 Making paginated request comparison:");
            debug!("  🎯 Target wallet: {}", wallet);
            debug!("  📊 Requested limit: {}", current_limit);
            debug!("  ⬅️ Before cursor: {:?}", before_hash);
            debug!("  📈 Page number: {}", page_number);
            debug!("  📦 Total fetched so far: {}", all_transactions.len());
            
            // Make the API request with retry logic
            let page_transactions = match self.fetch_transaction_page_with_retry(
                wallet, 
                chain, 
                current_limit, 
                before_hash.as_deref()
            ).await {
                Ok(transactions) => transactions,
                Err(e) => {
                    warn!("❌ Failed to fetch page {} for wallet {}: {}", page_number, wallet, e);
                    // Return what we have so far instead of failing completely
                    break;
                }
            };
            
            let page_size_actual = page_transactions.len();
            info!("✅ Page {} completed: {} transactions fetched for wallet {}", 
                  page_number, page_size_actual, wallet);
            
            // If no transactions returned, we've reached the end
            if page_size_actual == 0 {
                info!("🏁 No more transactions available for wallet {} (reached end)", wallet);
                break;
            }
            
            // Get the hash from the last transaction for next page
            if let Some(last_transaction) = page_transactions.last() {
                before_hash = Some(last_transaction.tx_hash.clone());
            }
            
            // Add transactions to our collection
            all_transactions.extend(page_transactions);
            
            // If we got fewer transactions than requested, we've reached the end
            if page_size_actual < current_limit as usize {
                info!("🏁 Received fewer transactions than requested for wallet {} ({}  < {}), reached end", 
                      wallet, page_size_actual, current_limit);
                break;
            }
            
            page_number += 1;
            
            // Rate limiting delay (except for the last iteration)
            if all_transactions.len() < max_transactions as usize {
                tokio::time::sleep(Duration::from_millis(pagination_delay_ms)).await;
            }
        }
        
        // Truncate to max_transactions if we exceeded it
        if all_transactions.len() > max_transactions as usize {
            all_transactions.truncate(max_transactions as usize);
        }
        
        info!("🎯 Pagination completed for wallet {}: fetched {} transactions across {} pages", 
              wallet, all_transactions.len(), page_number - 1);
        
        Ok(all_transactions)
    }
    
    /// Helper method to fetch a single page with retry logic for rate limiting
    async fn fetch_transaction_page_with_retry(
        &self,
        wallet: &str,
        chain: &str,
        limit: u32,
        before_hash: Option<&str>,
    ) -> Result<Vec<WalletTransaction>, BirdEyeError> {
        let max_retries = 3u32;
        let mut attempt = 1u32;
        
        loop {
            match self.get_wallet_transaction_history_paginated(
                wallet,
                Some(chain),
                Some(limit),
                before_hash,
            ).await {
                Ok(transactions) => return Ok(transactions),
                Err(BirdEyeError::RateLimit) if attempt <= max_retries => {
                    let delay_ms = 1000 * attempt.pow(2); // Exponential backoff: 1s, 4s, 9s
                    warn!("🚫 Rate limit hit for wallet {} page (attempt {}), retrying in {}ms", 
                          wallet, attempt, delay_ms);
                    tokio::time::sleep(Duration::from_millis(delay_ms as u64)).await;
                    attempt += 1;
                }
                Err(e) => {
                    if attempt > max_retries {
                        error!("💥 Max retries exceeded for wallet {} page: {}", wallet, e);
                    }
                    return Err(e);
                }
            }
        }
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

/// Quality criteria for filtering new listing tokens
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewListingTokenFilter {
    /// Minimum liquidity in USD
    pub min_liquidity: Option<f64>,
    /// Maximum age in hours since listing
    pub max_age_hours: Option<u32>,
    /// Maximum number of tokens to return
    pub max_tokens: Option<usize>,
    /// Sources to exclude from results
    pub exclude_sources: Option<Vec<String>>,
}

impl Default for NewListingTokenFilter {
    fn default() -> Self {
        Self {
            min_liquidity: Some(1000.0),      // $1k minimum liquidity
            max_age_hours: Some(24),          // Last 24 hours
            max_tokens: Some(25),             // Top 25 tokens max
            exclude_sources: None,            // No exclusions by default
        }
    }
}



// LEGACY: PriceFetcher implementation removed
// BirdEyeClient now provides current prices directly via get_current_prices()
// Historical prices come from embedded transaction data via NewTransactionParser

