use anyhow::Result;
use config_manager::SystemConfig;
use dex_client::{BirdEyeClient, TopTrader, TrendingToken as BirdEyeTrendingToken, GeneralTraderTransaction, DexScreenerClient, DexScreenerTrendingToken};
use persistence_layer::{RedisClient, DiscoveredWalletToken};
// NewFinancialEvent/NewEventType imports removed - using GeneralTraderTransaction directly
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};
use chrono::{DateTime, Utc, Duration as ChronoDuration};

// BirdEyeTrendingConfig removed - now uses SystemConfig directly

/// Token cache entry with timestamp for deduplication
#[derive(Serialize, Deserialize, Debug, Clone)]
struct TokenCacheEntry {
    token_address: String,
    chain_id: String,
    last_processed: DateTime<Utc>,
}

/// Redis-based token cache for time-based deduplication
struct TokenCache {
    redis_client: Arc<Mutex<Option<RedisClient>>>,
    cache_duration_hours: i64,
    key_prefix: String,
}

impl TokenCache {
    fn new(redis_client: Arc<Mutex<Option<RedisClient>>>, cache_duration_hours: i64) -> Self {
        Self {
            redis_client,
            cache_duration_hours,
            key_prefix: "token_cache".to_string(),
        }
    }

    /// Check if a token was processed recently (within cache duration)
    async fn is_token_cached(&self, token_address: &str, chain_id: &str) -> bool {
        let redis_guard = self.redis_client.lock().await;
        if let Some(ref redis) = *redis_guard {
            let cache_key = format!("{}:{}:{}", self.key_prefix, chain_id, token_address);
            
            match redis.get_cached_data(&cache_key).await {
                Ok(Some(cached_data)) => {
                    if let Ok(entry) = serde_json::from_str::<TokenCacheEntry>(&cached_data) {
                        let now = Utc::now();
                        let time_diff = now.signed_duration_since(entry.last_processed);
                        
                        // Check if token is still within cache window
                        if time_diff < ChronoDuration::hours(self.cache_duration_hours) {
                            debug!("🎯 Token {} on {} cached (processed {} ago)", 
                                   token_address, chain_id, time_diff);
                            return true;
                        } else {
                            debug!("⏰ Token {} on {} cache expired ({} ago)", 
                                   token_address, chain_id, time_diff);
                        }
                    }
                }
                Ok(None) => {
                    debug!("🆕 Token {} on {} not in cache", token_address, chain_id);
                }
                Err(e) => {
                    debug!("⚠️ Failed to check cache for token {} on {}: {}", token_address, chain_id, e);
                }
            }
        } else {
            warn!("⚠️ Redis client not available for token cache check");
        }
        false
    }

    /// Cache a token with current timestamp
    async fn cache_token(&self, token_address: &str, chain_id: &str) -> Result<()> {
        let redis_guard = self.redis_client.lock().await;
        if let Some(ref redis) = *redis_guard {
            let cache_key = format!("{}:{}:{}", self.key_prefix, chain_id, token_address);
            let entry = TokenCacheEntry {
                token_address: token_address.to_string(),
                chain_id: chain_id.to_string(),
                last_processed: Utc::now(),
            };

            let entry_json = serde_json::to_string(&entry)?;
            
            // Set with expiration (cache_duration_hours + 1 hour buffer)
            let expiry_seconds = ((self.cache_duration_hours + 1) * 3600) as u64;
            redis.set_with_expiry(&cache_key, &entry_json, expiry_seconds).await?;
            
            debug!("💾 Cached token {} on {} for {} hours", 
                   token_address, chain_id, self.cache_duration_hours);
        } else {
            warn!("⚠️ Redis client not available for token caching");
        }
        Ok(())
    }
}

/// Orchestrates trending token discovery using DexScreener scraping (replaces BirdEye trending)
pub struct BirdEyeTrendingOrchestrator {
    config: SystemConfig,
    birdeye_client: BirdEyeClient,
    dexscreener_client: Option<Arc<Mutex<DexScreenerClient>>>,
    redis_client: Arc<Mutex<Option<RedisClient>>>,
    is_running: Arc<Mutex<bool>>,
    token_cache: TokenCache,
}

impl BirdEyeTrendingOrchestrator {
    /// Create a new BirdEye trending orchestrator
    pub fn new(
        config: SystemConfig,
        redis_client: Option<RedisClient>,
    ) -> Result<Self> {
        // Use BirdEye config from SystemConfig
        let birdeye_config = config.birdeye.clone();
        let birdeye_client = BirdEyeClient::new(birdeye_config)?;
        
        // Initialize DexScreener client if enabled
        let dexscreener_client = if config.dexscreener.enabled {
            let dexscreener_config = dex_client::DexScreenerClientConfig {
                api_base_url: config.dexscreener.api_base_url.clone(),
                request_timeout_seconds: config.dexscreener.request_timeout_seconds,
                rate_limit_delay_ms: config.dexscreener.rate_limit_delay_ms,
                max_retries: config.dexscreener.max_retries,
                enabled: config.dexscreener.enabled,
                // Browser automation settings
                chrome_executable_path: config.dexscreener.chrome_executable_path.clone(),
                headless_mode: config.dexscreener.headless_mode,
                anti_detection_enabled: config.dexscreener.anti_detection_enabled,
            };
            Some(Arc::new(Mutex::new(DexScreenerClient::new(dexscreener_config)?)))
        } else {
            None
        };
        
        let redis_arc = Arc::new(Mutex::new(redis_client));
        let cache_duration = config.discovery.token_cache_duration_hours.unwrap_or(1);
        let token_cache = TokenCache::new(redis_arc.clone(), cache_duration);
        
        Ok(Self {
            config,
            birdeye_client,
            dexscreener_client,
            redis_client: redis_arc,
            is_running: Arc::new(Mutex::new(false)),
            token_cache,
        })
    }

    /// Start the trending discovery loop
    pub async fn start(&self) -> Result<()> {
        let mut is_running = self.is_running.lock().await;
        if *is_running {
            warn!("BirdEye trending orchestrator is already running");
            return Ok(());
        }
        *is_running = true;
        drop(is_running);

        info!("🚀 Starting Enhanced Multi-Sort BirdEye Discovery Orchestrator");
        let max_traders_per_token = 100; // Default limit for discovery
        info!("📋 Enhanced Discovery: 3 sorting strategies (rank + volume + liquidity), unlimited tokens, max_traders_per_token={}, cycle_interval={}s",
              max_traders_per_token, 60);

        loop {
            // Check if we should stop
            {
                let is_running = self.is_running.lock().await;
                if !*is_running {
                    info!("🛑 BirdEye trending orchestrator stopped");
                    break;
                }
            }

            // Execute one cycle
            match self.execute_discovery_cycle().await {
                Ok(discovered_wallets) => {
                    if discovered_wallets > 0 {
                        info!("✅ Cycle completed: discovered {} quality wallets", discovered_wallets);
                    } else {
                        debug!("🔍 Cycle completed: no new quality wallets discovered");
                    }
                }
                Err(e) => {
                    error!("❌ Discovery cycle failed: {}", e);
                }
            }

            // Wait before next cycle (interruptible sleep)
            let sleep_duration = Duration::from_secs(60); // BirdEye polling interval
            let mut interval = tokio::time::interval(Duration::from_millis(500)); // Check stop flag every 500ms
            let start_time = std::time::Instant::now();
            
            loop {
                interval.tick().await;
                
                // Check if we should stop during sleep
                {
                    let is_running = self.is_running.lock().await;
                    if !*is_running {
                        info!("🛑 Stop requested during sleep, breaking out early");
                        return Ok(());
                    }
                }
                
                // Check if we've slept long enough
                if start_time.elapsed() >= sleep_duration {
                    break;
                }
            }
        }

        Ok(())
    }

    /// Stop the trending discovery loop
    pub async fn stop(&self) {
        let mut is_running = self.is_running.lock().await;
        *is_running = false;
        info!("🛑 BirdEye trending orchestrator stop requested");
    }

    /// Execute one complete discovery cycle with enhanced multi-source strategy
    pub async fn execute_discovery_cycle(&self) -> Result<usize> {
        // Set is_running to true for this cycle
        {
            let mut is_running = self.is_running.lock().await;
            *is_running = true;
        }
        
        info!("🔄 Starting Enhanced Multichain Discovery Cycle");
        debug!("📊 Discovery sources: 1) Paginated trending tokens (unlimited), 2) Paginated gainers (3 timeframes), 3) DexScreener boosted");
        
        let mut total_discovered_wallets = 0;
        
        // Iterate through all enabled chains
        for chain in &self.config.multichain.enabled_chains {
            info!("🔗 Processing chain: {}", chain);
            
            total_discovered_wallets += self.execute_discovery_cycle_for_chain(chain).await?;
            
            // Check if we should stop between chains
            {
                let is_running = self.is_running.lock().await;
                if !*is_running {
                    info!("🛑 Stop requested between chains, breaking out");
                    break;
                }
            }
        }
        
        info!("✅ Multichain discovery cycle completed: {} total wallets discovered across {} chains", 
              total_discovered_wallets, self.config.multichain.enabled_chains.len());
        
        // Reset is_running flag after cycle completes
        {
            let mut is_running = self.is_running.lock().await;
            *is_running = false;
        }
        
        Ok(total_discovered_wallets)
    }
    
    /// Execute discovery cycle for a specific chain
    async fn execute_discovery_cycle_for_chain(&self, chain: &str) -> Result<usize> {
        info!("🔄 Starting discovery cycle for chain: {}", chain);
        
        // Step 1: Get trending tokens using enhanced multi-sort discovery for this chain
        let trending_tokens = self.get_trending_tokens_for_chain(chain).await?;
        if trending_tokens.is_empty() {
            debug!("📊 No trending tokens found from multi-sort discovery");
            return Ok(0);
        }

        info!("📈 Paginated trending discovery: {} tokens (unlimited processing)", trending_tokens.len());
        
        // Safety mechanism: warn if processing a very large number of tokens
        if trending_tokens.len() > 1000 {
            warn!("⚠️ Processing {} trending tokens - this may take longer and use more API calls", trending_tokens.len());
        }

        let mut total_discovered_wallets = 0;

        // Step 2: For each trending token, get top traders
        for (i, token) in trending_tokens.iter().enumerate() {
            // Check if we should stop before processing each token
            {
                let is_running = self.is_running.lock().await;
                if !*is_running {
                    info!("🛑 Stop requested during token processing, breaking out of loop at token {}/{}", 
                          i + 1, trending_tokens.len());
                    break;
                }
            }

            debug!("🎯 Processing token {}/{}: {} ({})", 
                   i + 1, trending_tokens.len(), token.symbol, token.address);

            // Check if token is cached (skip if processed recently)
            if self.token_cache.is_token_cached(&token.address, chain).await {
                debug!("⏭️ Skipping cached token {} ({}) - processed recently", 
                       token.symbol, token.address);
                continue;
            }

            // Security check for non-Solana chains using Honeypot.is
            if chain != "solana" {
                if !dex_client::is_token_safe(&token.address, chain).await {
                    warn!("🚫 Skipping honeypot/high-risk token: {} ({}) on {}", 
                          token.symbol, token.address, chain);
                    // Cache the rejected token to avoid rechecking
                    if let Err(e) = self.token_cache.cache_token(&token.address, chain).await {
                        warn!("⚠️ Failed to cache rejected token {} ({}): {}", token.symbol, token.address, e);
                    }
                    continue;
                }
            }

            match self.get_top_traders_for_token(&token.address, chain).await {
                Ok(top_traders) => {
                    if !top_traders.is_empty() {
                        info!("👤 Found {} quality traders for {} ({})", 
                              top_traders.len(), token.symbol, token.address);

                        // Step 3: Push quality wallet-token pairs to Redis for P&L analysis
                        match self.push_wallet_token_pairs_to_queue(&top_traders, token, chain).await {
                            Ok(pushed_count) => {
                                total_discovered_wallets += pushed_count;
                                debug!("📤 Pushed {} wallets to analysis queue for {}", 
                                       pushed_count, token.symbol);
                            }
                            Err(e) => {
                                warn!("❌ Failed to push wallets for {}: {}", token.symbol, e);
                            }
                        }
                    } else {
                        debug!("⭕ No quality traders found for {} ({})", token.symbol, token.address);
                    }
                    
                    // Cache the token after successful processing (regardless of traders found)
                    if let Err(e) = self.token_cache.cache_token(&token.address, chain).await {
                        warn!("⚠️ Failed to cache token {} ({}): {}", token.symbol, token.address, e);
                    }
                }
                Err(e) => {
                    warn!("❌ Failed to get top traders for {} ({}): {}", token.symbol, token.address, e);
                    // Also cache failed tokens to avoid immediate retries
                    if let Err(cache_err) = self.token_cache.cache_token(&token.address, chain).await {
                        warn!("⚠️ Failed to cache failed token {} ({}): {}", token.symbol, token.address, cache_err);
                    }
                }
            }

            // Rate limiting between tokens (interruptible)
            if i < trending_tokens.len() - 1 {
                // Make this sleep interruptible by checking stop flag every 100ms
                let sleep_duration = Duration::from_millis(500);
                let check_interval = Duration::from_millis(100);
                let start_time = std::time::Instant::now();
                
                while start_time.elapsed() < sleep_duration {
                    tokio::time::sleep(check_interval).await;
                    
                    // Check if we should stop during rate limiting sleep
                    {
                        let is_running = self.is_running.lock().await;
                        if !*is_running {
                            info!("🛑 Stop requested during trending token rate limiting, breaking out early");
                            return Ok(total_discovered_wallets);
                        }
                    }
                }
            }
        }

        // Step 3: Removed BirdEye gainers discovery - using only DexScreener scraping for token discovery

        // Step 4: Removed DexScreener boosted tokens discovery - using only DexScreener scraping for trending tokens

        // Step 5: Removed BirdEye new listings discovery - using only DexScreener scraping for trending tokens

        info!("✅ DexScreener Scraping Discovery Cycle Completed for chain {}: {} total quality wallets discovered", chain, total_discovered_wallets);
        debug!("📊 Simplified discovery pipeline for chain {}: DexScreener trending tokens scraping → BirdEye top traders API → wallet queue", chain);
        Ok(total_discovered_wallets)
    }


    /// Get trending tokens for a specific chain using enhanced multi-sort discovery
    async fn get_trending_tokens_for_chain(&self, chain: &str) -> Result<Vec<BirdEyeTrendingToken>> {
        debug!("📊 Starting trending token discovery from DexScreener scraping for chain: {}", chain);

        // Use DexScreener scraping instead of BirdEye API
        if let Some(ref dexscreener_client_arc) = self.dexscreener_client {
            let mut dexscreener_client = dexscreener_client_arc.lock().await;
            
            // Use DexScreener scraping to get trending tokens (24h timeframe)
            match dexscreener_client.get_trending_tokens_scraped(chain, "trendingScoreH24").await {
                Ok(dex_tokens) => {
                    info!("🎯 DexScreener scraping completed: {} tokens found for chain {}", dex_tokens.len(), chain);
                    
                    // Convert DexScreener tokens to BirdEye format for compatibility
                    let mut converted_tokens: Vec<BirdEyeTrendingToken> = dex_tokens
                        .into_iter()
                        .map(|token| self.convert_dexscreener_to_birdeye_token(token))
                        .collect();
                    
                    // Apply volume-based sorting
                    converted_tokens.sort_by(|a, b| b.volume_24h.partial_cmp(&a.volume_24h).unwrap_or(std::cmp::Ordering::Equal));
                    
                    // Apply max trending tokens limit (0 = unlimited)
                    let max_trending_tokens = 25; // Default limit for quality filtering
                    if max_trending_tokens > 0 && converted_tokens.len() > max_trending_tokens {
                        converted_tokens.truncate(max_trending_tokens);
                        info!("📈 Processing trending tokens: {} tokens (limited to {}) for chain {}", converted_tokens.len(), max_trending_tokens, chain);
                    } else {
                        info!("📈 Processing all discovered trending tokens: {} tokens for chain {}", converted_tokens.len(), chain);
                    }

                    if self.config.system.debug_mode && !converted_tokens.is_empty() {
                        debug!("🎯 Top trending tokens from DexScreener scraping for chain {}:", chain);
                        for (i, token) in converted_tokens.iter().enumerate().take(8) {
                            debug!("  {}. {} ({}) - Vol: ${:.0}, Liq: ${:.0}, Change: {:.1}%", 
                                   i + 1, token.symbol, token.address, 
                                   token.volume_24h.unwrap_or(0.0),
                                   token.liquidity.unwrap_or(0.0),
                                   token.price_change_24h.unwrap_or(0.0));
                        }
                    }
                    
                    return Ok(converted_tokens);
                }
                Err(e) => {
                    error!("❌ DexScreener scraping failed for chain {}: {}", chain, e);
                    return Err(anyhow::anyhow!("DexScreener scraping failed - no trending tokens available"));
                }
            }
        } else {
            error!("❌ DexScreener client not initialized for chain {}", chain);
            return Err(anyhow::anyhow!("DexScreener client not available - trending token discovery requires DexScreener scraping"));
        }
    }

    /// Get top traders for a specific token on a specific chain
    async fn get_top_traders_for_token(&self, token_address: &str, chain: &str) -> Result<Vec<TopTrader>> {
        debug!("👥 Fetching top traders for token: {} on chain: {}", token_address, chain);

        match self.birdeye_client.get_top_traders_paginated(token_address, chain).await {
            Ok(traders) => {
                debug!("📊 Retrieved {} raw traders for token {} on chain {}", traders.len(), token_address, chain);

                // Apply quality filtering using trader filter config
                let quality_traders = self.birdeye_client.filter_top_traders(
                    traders,
                    self.config.trader_filter.min_capital_deployed_sol * 230.0, // Convert SOL to USD roughly
                    self.config.trader_filter.min_total_trades,
                    Some(self.config.trader_filter.min_win_rate),
                    Some(24), // Default to 24 hours
                );

                // Limit to max traders per token
                let mut filtered_traders = quality_traders;
                let max_traders_per_token = 100; // Default limit for discovery
                if filtered_traders.len() > max_traders_per_token as usize {
                    filtered_traders.truncate(max_traders_per_token as usize);
                }

                debug!("✅ Filtered to {} quality traders for token {} on chain {}", 
                       filtered_traders.len(), token_address, chain);

                if self.config.system.debug_mode && !filtered_traders.is_empty() {
                    for (i, trader) in filtered_traders.iter().enumerate().take(3) {
                        debug!("  {}. {} - Volume: ${:.0}, Trades: {}", 
                               i + 1, trader.owner, trader.volume, trader.trade);
                    }
                }

                Ok(filtered_traders)
            }
            Err(e) => {
                warn!("❌ Failed to fetch top traders for token {} on chain {}: {}", token_address, chain, e);
                Err(e.into())
            }
        }
    }

    /// Push quality wallet-token pairs to Redis queue for targeted P&L analysis
    async fn push_wallet_token_pairs_to_queue(&self, traders: &[TopTrader], token: &BirdEyeTrendingToken, chain: &str) -> Result<usize> {
        if traders.is_empty() {
            return Ok(0);
        }

        let wallet_token_pairs: Vec<DiscoveredWalletToken> = traders.iter()
            .map(|trader| DiscoveredWalletToken {
                wallet_address: trader.owner.clone(),
                chain: chain.to_string(),
                token_address: token.address.clone(),
                token_symbol: token.symbol.clone(),
                trader_volume_usd: trader.volume,
                trader_trades: trader.trade,
                discovered_at: chrono::Utc::now(),
            })
            .collect();

        debug!("📤 Pushing {} wallet-token pairs to Redis queue for token {} on chain {}", wallet_token_pairs.len(), token.symbol, chain);

        let redis = self.redis_client.lock().await;
        if let Some(ref redis_client) = *redis {
            match redis_client.push_discovered_wallet_token_pairs_deduplicated(&wallet_token_pairs).await {
                Ok(pushed_count) => {
                    let skipped_count = wallet_token_pairs.len() - pushed_count;
                    if skipped_count > 0 {
                        info!("✅ Pushed {} new wallet-token pairs to analysis queue for {} on chain {} (skipped {} duplicates)", 
                              pushed_count, token.symbol, chain, skipped_count);
                    } else {
                        info!("✅ Successfully pushed {} quality wallet-token pairs to analysis queue for {} on chain {}", 
                              pushed_count, token.symbol, chain);
                    }
                    Ok(pushed_count)
                }
                Err(e) => {
                    error!("❌ Failed to push wallet-token pairs to Redis queue: {}", e);
                    Err(e.into())
                }
            }
        } else {
            warn!("⚠️ Redis client not available, cannot push wallet-token pairs");
            Ok(0)
        }
    }

    // get_wallet_transaction_history method removed - was unused and relied on removed get_trader_transactions


    /// Get statistics about the current discovery state
    pub async fn get_discovery_stats(&self) -> Result<DiscoveryStats> {
        let redis = self.redis_client.lock().await;
        if let Some(ref redis_client) = *redis {
            let queue_size = redis_client.get_wallet_queue_size().await.unwrap_or(0);
            
            Ok(DiscoveryStats {
                is_running: *self.is_running.lock().await,
                wallet_queue_size: queue_size as u32,
                config: self.config.clone(),
                tokens_discovered: 0, // TODO: Track this metric
                wallet_token_pairs_discovered: queue_size as u32,
                new_listing_tokens_discovered: 0, // TODO: Track this metric
                new_listing_wallets_discovered: 0, // TODO: Track this metric
            })
        } else {
            Ok(DiscoveryStats {
                is_running: *self.is_running.lock().await,
                wallet_queue_size: 0,
                config: self.config.clone(),
                tokens_discovered: 0,
                wallet_token_pairs_discovered: 0,
                new_listing_tokens_discovered: 0,
                new_listing_wallets_discovered: 0,
            })
        }
    }
    
    /// Convert DexScreener token to BirdEye format for compatibility
    fn convert_dexscreener_to_birdeye_token(&self, dex_token: DexScreenerTrendingToken) -> BirdEyeTrendingToken {
        BirdEyeTrendingToken {
            address: dex_token.address,
            symbol: dex_token.symbol,
            name: dex_token.name,
            decimals: dex_token.decimals,
            price: dex_token.price,
            price_change_24h: dex_token.price_change_24h,
            volume_24h: dex_token.volume_24h,
            volume_change_24h: dex_token.volume_change_24h,
            liquidity: dex_token.liquidity,
            fdv: dex_token.fdv,
            marketcap: dex_token.marketcap,
            rank: dex_token.rank,
            logo_uri: dex_token.logo_uri,
            txns_24h: dex_token.txns_24h,
            last_trade_unix_time: dex_token.last_trade_unix_time,
        }
    }
}

/// Statistics about the discovery process
#[derive(Debug, Clone)]
pub struct DiscoveryStats {
    pub is_running: bool,
    pub wallet_queue_size: u32,
    pub config: SystemConfig,
    pub tokens_discovered: u32,
    pub wallet_token_pairs_discovered: u32,
    pub new_listing_tokens_discovered: u32,
    pub new_listing_wallets_discovered: u32,
}

/// Processed swap transaction for BirdEye data analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedSwap {
    pub token_in: String,
    pub token_out: String,
    pub amount_in: Decimal,
    pub amount_out: Decimal,
    pub sol_equivalent: Decimal,
    pub price_per_token: Decimal,
    pub tx_hash: String,
    pub timestamp: i64,
    pub source: String,
}

impl ProcessedSwap {
    /// Process BirdEye transactions into ProcessedSwap format
    pub fn from_birdeye_transactions(transactions: &[GeneralTraderTransaction]) -> Result<Vec<ProcessedSwap>> {
        let mut processed_swaps = Vec::new();
        
        for tx in transactions {
            // Determine which token is being sold (from) and which is being bought (to)
            let (token_in, amount_in, token_out, amount_out) = if tx.quote.type_swap == "from" {
                // Quote token is being sold, base token is being bought
                (
                    tx.quote.address.clone(),
                    Decimal::from_f64_retain(tx.quote.ui_amount).unwrap_or_default(),
                    tx.base.address.clone(),
                    Decimal::from_f64_retain(tx.base.ui_amount).unwrap_or_default(),
                )
            } else {
                // Base token is being sold, quote token is being bought  
                (
                    tx.base.address.clone(),
                    Decimal::from_f64_retain(tx.base.ui_amount).unwrap_or_default(),
                    tx.quote.address.clone(),
                    Decimal::from_f64_retain(tx.quote.ui_amount).unwrap_or_default(),
                )
            };
            
            // Calculate SOL equivalent and price
            let sol_mint = "So11111111111111111111111111111111111111112";
            let sol_equivalent = if token_in == sol_mint {
                amount_in
            } else if token_out == sol_mint {
                amount_out
            } else {
                // Use quote price to estimate SOL equivalent
                Decimal::from_f64_retain(tx.quote_price).unwrap_or_default() * amount_in
            };
            
            let price_per_token = if token_out == sol_mint {
                // Selling token for SOL
                if amount_out > Decimal::ZERO {
                    amount_out / amount_in
                } else {
                    Decimal::ZERO
                }
            } else if token_in == sol_mint {
                // Buying token with SOL
                tx.base.price.map(|p| Decimal::from_f64_retain(p).unwrap_or_default())
                    .unwrap_or_else(|| {
                        if amount_in > Decimal::ZERO {
                            amount_out / amount_in
                        } else {
                            Decimal::ZERO
                        }
                    })
            } else {
                // Token to token swap - use base price
                tx.base.price.map(|p| Decimal::from_f64_retain(p).unwrap_or_default())
                    .unwrap_or_default()
            };
            
            processed_swaps.push(ProcessedSwap {
                token_in,
                token_out,
                amount_in,
                amount_out,
                sol_equivalent,
                price_per_token,
                tx_hash: tx.tx_hash.clone(),
                timestamp: tx.block_unix_time,
                source: tx.source.clone(),
            });
        }
        
        Ok(processed_swaps)
    }
    
    // LEGACY METHOD REMOVED: to_financial_event()
    // This method converted ProcessedSwap to legacy FinancialEvent format
    // New P&L engine uses GeneralTraderTransaction directly with embedded prices
}

// Tests removed - will use integration tests with SystemConfig