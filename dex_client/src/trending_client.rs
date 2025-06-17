use crate::types::{BoostedToken, TokenPair, TrendingToken, TrendingPair, TrendingCriteria};
use crate::DexClientError;
use anyhow::Result;
use persistence_layer::RedisClient;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

pub struct TrendingClient {
    http_client: Client,
    redis_client: Arc<Mutex<Option<RedisClient>>>,
    base_url: String,
    criteria: TrendingCriteria,
    debug: bool,
}

impl TrendingClient {
    pub fn new(
        redis_client: Option<RedisClient>,
        base_url: String,
        criteria: TrendingCriteria,
        debug: bool,
    ) -> Result<Self> {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36")
            .build()?;

        Ok(Self {
            http_client,
            redis_client: Arc::new(Mutex::new(redis_client)),
            base_url,
            criteria,
            debug,
        })
    }

    /// Fetch latest boosted tokens from DexScreener
    pub async fn fetch_latest_boosted_tokens(&self) -> Result<Vec<BoostedToken>> {
        let url = format!("{}/token-boosts/latest/v1", self.base_url);
        
        if self.debug {
            debug!("Fetching latest boosted tokens from: {}", url);
        }

        let response = self.http_client
            .get(&url)
            .header("Accept", "*/*")
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            return Err(DexClientError::Parsing(
                format!("HTTP error: {}", status)
            ).into());
        }

        let tokens: Vec<BoostedToken> = response.json().await?;
        
        if self.debug {
            debug!("Fetched {} latest boosted tokens", tokens.len());
        }

        Ok(tokens)
    }

    /// Fetch top boosted tokens from DexScreener
    pub async fn fetch_top_boosted_tokens(&self) -> Result<Vec<BoostedToken>> {
        let url = format!("{}/token-boosts/top/v1", self.base_url);
        
        if self.debug {
            debug!("Fetching top boosted tokens from: {}", url);
        }

        let response = self.http_client
            .get(&url)
            .header("Accept", "*/*")
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            return Err(DexClientError::Parsing(
                format!("HTTP error: {}", status)
            ).into());
        }

        let tokens: Vec<BoostedToken> = response.json().await?;
        
        if self.debug {
            debug!("Fetched {} top boosted tokens", tokens.len());
        }

        Ok(tokens)
    }

    /// Fetch token pairs for a specific token address
    pub async fn fetch_token_pairs(&self, token_address: &str) -> Result<Vec<TokenPair>> {
        let url = format!("{}/token-pairs/v1/solana/{}", self.base_url, token_address);
        
        if self.debug {
            debug!("Fetching token pairs for: {}", token_address);
        }

        // Add delay to respect rate limits (300 req/min = 200ms between requests)
        tokio::time::sleep(Duration::from_millis(200)).await;

        let response = self.http_client
            .get(&url)
            .header("Accept", "*/*")
            .send()
            .await?;

        if !response.status().is_success() {
            if self.debug {
                debug!("Failed to fetch pairs for token {}: HTTP {}", token_address, response.status());
            }
            return Ok(vec![]);
        }

        let pairs: Vec<TokenPair> = response.json().await?;
        
        if self.debug {
            debug!("Fetched {} pairs for token {}", pairs.len(), token_address);
        }

        Ok(pairs)
    }

    /// Combine and deduplicate boosted tokens
    pub fn combine_boosted_tokens(&self, latest: Vec<BoostedToken>, top: Vec<BoostedToken>) -> Vec<BoostedToken> {
        let mut token_map: HashMap<String, BoostedToken> = HashMap::new();

        // Process latest tokens
        for token in latest {
            if token.chain_id == "solana" {
                token_map.insert(token.token_address.clone(), token);
            }
        }

        // Process top tokens, updating with higher boost amounts
        for token in top {
            if token.chain_id == "solana" {
                let key = token.token_address.clone();
                match token_map.get(&key) {
                    Some(existing) => {
                        let existing_amount = existing.total_amount.or(existing.amount).unwrap_or(0);
                        let new_amount = token.total_amount.or(token.amount).unwrap_or(0);
                        
                        if new_amount > existing_amount {
                            token_map.insert(key, token);
                        }
                    }
                    None => {
                        token_map.insert(key, token);
                    }
                }
            }
        }

        let mut tokens: Vec<BoostedToken> = token_map.into_values().collect();
        
        // Sort by boost amount (highest first)
        tokens.sort_by(|a, b| {
            let a_amount = a.total_amount.or(a.amount).unwrap_or(0);
            let b_amount = b.total_amount.or(b.amount).unwrap_or(0);
            b_amount.cmp(&a_amount)
        });

        if self.debug {
            debug!("Combined {} unique Solana boosted tokens", tokens.len());
        }

        tokens
    }

    /// Analyze token for trending criteria
    pub async fn analyze_token_for_trending(&self, token: &BoostedToken) -> Result<Option<TrendingToken>> {
        let pairs = self.fetch_token_pairs(&token.token_address).await?;
        
        if pairs.is_empty() {
            return Ok(None);
        }

        // Find the pair with the highest volume
        let top_pair = pairs.iter()
            .filter_map(|pair| {
                let volume_24h = pair.volume.as_ref()?.get("h24").copied().unwrap_or(0.0);
                let volume_6h = pair.volume.as_ref()?.get("h6").copied().unwrap_or(0.0);
                let volume_1h = pair.volume.as_ref()?.get("h1").copied().unwrap_or(0.0);
                
                let txns_24h = pair.txns.as_ref()
                    .and_then(|t| t.get("h24"))
                    .map(|t| t.buys + t.sells)
                    .unwrap_or(0);
                let txns_6h = pair.txns.as_ref()
                    .and_then(|t| t.get("h6"))
                    .map(|t| t.buys + t.sells)
                    .unwrap_or(0);
                let txns_1h = pair.txns.as_ref()
                    .and_then(|t| t.get("h1"))
                    .map(|t| t.buys + t.sells)
                    .unwrap_or(0);

                let price_change_24h = pair.price_change.as_ref()?.get("h24").copied().unwrap_or(0.0);
                let price_change_6h = pair.price_change.as_ref()?.get("h6").copied().unwrap_or(0.0);
                let price_change_1h = pair.price_change.as_ref()?.get("h1").copied().unwrap_or(0.0);
                
                let liquidity_usd = pair.liquidity.as_ref()?.usd.unwrap_or(0.0);
                let price_usd = pair.price_usd.as_ref()?.parse::<f64>().ok().unwrap_or(0.0);

                Some(TrendingPair {
                    pair_address: pair.pair_address.clone(),
                    dex_id: pair.dex_id.clone(),
                    base_token_symbol: pair.base_token.symbol.clone(),
                    quote_token_symbol: pair.quote_token.symbol.clone(),
                    price_usd,
                    volume_24h,
                    volume_6h,
                    volume_1h,
                    txns_24h,
                    txns_6h,
                    txns_1h,
                    price_change_24h,
                    price_change_6h,
                    price_change_1h,
                    liquidity_usd,
                    market_cap: pair.market_cap,
                    created_at: pair.pair_created_at,
                })
            })
            .max_by(|a, b| a.volume_24h.partial_cmp(&b.volume_24h).unwrap_or(std::cmp::Ordering::Equal));

        if let Some(ref pair) = top_pair {
            // Check trending criteria
            if !self.meets_trending_criteria(pair) {
                if self.debug {
                    debug!("Token {} does not meet trending criteria", token.token_address);
                }
                return Ok(None);
            }
        } else {
            return Ok(None);
        }

        let boost_amount = token.total_amount.or(token.amount).unwrap_or(0);
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;

        Ok(Some(TrendingToken {
            token_address: token.token_address.clone(),
            chain_id: token.chain_id.clone(),
            boost_amount,
            description: token.description.clone(),
            top_pair,
            discovered_at: now,
        }))
    }

    /// Check if a pair meets trending criteria
    fn meets_trending_criteria(&self, pair: &TrendingPair) -> bool {
        // Volume check
        if pair.volume_24h < self.criteria.min_volume_24h {
            return false;
        }

        // Transaction count check
        if pair.txns_24h < self.criteria.min_txns_24h {
            return false;
        }

        // Liquidity check
        if pair.liquidity_usd < self.criteria.min_liquidity_usd {
            return false;
        }

        // Price change check (optional)
        if let Some(min_change) = self.criteria.min_price_change_24h {
            if pair.price_change_24h.abs() < min_change {
                return false;
            }
        }

        // Age check (optional)
        if let Some(max_age_hours) = self.criteria.max_pair_age_hours {
            if let Some(created_at) = pair.created_at {
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
                let age_hours = (now - created_at) / 3600;
                if age_hours > max_age_hours as i64 {
                    return false;
                }
            }
        }

        true
    }

    /// Discover trending tokens through boosted token analysis
    pub async fn discover_trending_tokens(&self) -> Result<Vec<TrendingToken>> {
        info!("Starting trending token discovery...");

        // Step 1: Fetch boosted tokens
        let (latest_result, top_result) = tokio::join!(
            self.fetch_latest_boosted_tokens(),
            self.fetch_top_boosted_tokens()
        );

        let latest_tokens = latest_result?;
        let top_tokens = top_result?;

        info!("Fetched {} latest and {} top boosted tokens", latest_tokens.len(), top_tokens.len());

        // Step 2: Combine and deduplicate
        let combined_tokens = self.combine_boosted_tokens(latest_tokens, top_tokens);
        
        // Step 3: Analyze top tokens (limit to avoid rate limits)
        let tokens_to_analyze = combined_tokens.into_iter().take(20).collect::<Vec<_>>();
        let mut trending_tokens = Vec::new();

        for token in tokens_to_analyze {
            match self.analyze_token_for_trending(&token).await {
                Ok(Some(trending_token)) => {
                    if self.debug {
                        debug!("Found trending token: {} ({})", 
                               trending_token.token_address, 
                               trending_token.top_pair.as_ref().map(|p| format!("{}/{}", p.base_token_symbol, p.quote_token_symbol)).unwrap_or_default());
                    }
                    trending_tokens.push(trending_token);
                }
                Ok(None) => {
                    // Token doesn't meet criteria, continue
                }
                Err(e) => {
                    warn!("Failed to analyze token {}: {}", token.token_address, e);
                }
            }
        }

        info!("Discovered {} trending tokens", trending_tokens.len());

        // Step 4: Save to Redis
        if !trending_tokens.is_empty() {
            if let Err(e) = self.save_trending_tokens(&trending_tokens).await {
                error!("Failed to save trending tokens to Redis: {}", e);
            }
        }

        Ok(trending_tokens)
    }

    /// Save trending tokens to Redis
    async fn save_trending_tokens(&self, tokens: &[TrendingToken]) -> Result<()> {
        let redis = self.redis_client.lock().await;
        if let Some(ref redis_client) = *redis {
            for token in tokens {
                if let Some(ref pair) = token.top_pair {
                    // Save trending pair for wallet discovery
                    if let Err(e) = redis_client.set_trending_pair(&pair.pair_address).await {
                        warn!("Failed to save trending pair {}: {}", pair.pair_address, e);
                    }
                }
            }
            info!("Saved {} trending tokens to Redis", tokens.len());
        } else {
            warn!("Redis client not available, cannot save trending tokens");
        }
        
        Ok(())
    }

    /// Get current trending criteria
    pub fn get_criteria(&self) -> &TrendingCriteria {
        &self.criteria
    }

    /// Update trending criteria
    pub fn update_criteria(&mut self, criteria: TrendingCriteria) {
        self.criteria = criteria;
        info!("Updated trending criteria: min_volume_24h=${:.0}, min_txns_24h={}, min_liquidity_usd=${:.0}", 
              self.criteria.min_volume_24h, 
              self.criteria.min_txns_24h, 
              self.criteria.min_liquidity_usd);
    }
}