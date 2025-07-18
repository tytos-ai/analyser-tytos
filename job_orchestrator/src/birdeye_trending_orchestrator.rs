use anyhow::Result;
use config_manager::SystemConfig;
use dex_client::{BirdEyeClient, TopTrader, TrendingToken as BirdEyeTrendingToken, GeneralTraderTransaction, GainerLoser, DexScreenerClient, DexScreenerBoostedToken, NewListingToken, NewListingTokenFilter};
use persistence_layer::{RedisClient, DiscoveredWalletToken};
use pnl_core::{FinancialEvent, EventType, EventMetadata};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use chrono::{DateTime, Utc};

// BirdEyeTrendingConfig removed - now uses SystemConfig directly

/// Orchestrates trending token discovery and top trader identification using BirdEye API + DexScreener boosted tokens
pub struct BirdEyeTrendingOrchestrator {
    config: SystemConfig,
    birdeye_client: BirdEyeClient,
    dexscreener_client: Option<DexScreenerClient>,
    redis_client: Arc<Mutex<Option<RedisClient>>>,
    is_running: Arc<Mutex<bool>>,
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
            };
            Some(DexScreenerClient::new(dexscreener_config)?)
        } else {
            None
        };
        
        Ok(Self {
            config,
            birdeye_client,
            dexscreener_client,
            redis_client: Arc::new(Mutex::new(redis_client)),
            is_running: Arc::new(Mutex::new(false)),
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
        info!("📋 Enhanced Discovery: 3 sorting strategies (rank + volume + liquidity), unlimited tokens, max_traders_per_token={}, cycle_interval={}s", 
              self.config.birdeye.max_traders_per_token, 60);

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

            // Wait before next cycle
            tokio::time::sleep(Duration::from_secs(60)).await; // BirdEye polling interval
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
        info!("🔄 Starting Enhanced Multi-Source Discovery Cycle");
        debug!("📊 Discovery sources: 1) Paginated trending tokens (unlimited), 2) Paginated gainers (3 timeframes), 3) DexScreener boosted");

        // Step 1: Get trending tokens using enhanced multi-sort discovery
        let trending_tokens = self.get_trending_tokens().await?;
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
            debug!("🎯 Processing token {}/{}: {} ({})", 
                   i + 1, trending_tokens.len(), token.symbol, token.address);

            match self.get_top_traders_for_token(&token.address).await {
                Ok(top_traders) => {
                    if !top_traders.is_empty() {
                        info!("👤 Found {} quality traders for {} ({})", 
                              top_traders.len(), token.symbol, token.address);

                        // Step 3: Push quality wallet-token pairs to Redis for P&L analysis
                        match self.push_wallet_token_pairs_to_queue(&top_traders, token).await {
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
                }
                Err(e) => {
                    warn!("❌ Failed to get top traders for {} ({}): {}", token.symbol, token.address, e);
                }
            }

            // Rate limiting between tokens
            if i < trending_tokens.len() - 1 {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }

        // Step 3: Get top gainers across different timeframes with pagination (15 API calls total)
        info!("🏆 Starting paginated multi-timeframe gainers discovery");
        
        match self.get_top_gainers().await {
            Ok(gainers) => {
                if !gainers.is_empty() {
                    info!("💰 Found {} top gainers across all timeframes and pages", gainers.len());
                    
                    // Convert gainers to wallet-token pairs and push to queue
                    match self.push_gainers_to_queue(&gainers, "ALL_TIMEFRAMES").await {
                        Ok(pushed_count) => {
                            total_discovered_wallets += pushed_count;
                            debug!("📤 Pushed {} gainer wallets to analysis queue", pushed_count);
                        }
                        Err(e) => {
                            warn!("❌ Failed to push gainers: {}", e);
                        }
                    }
                } else {
                    debug!("⭕ No gainers found across all timeframes");
                }
            }
            Err(e) => {
                warn!("❌ Failed to get gainers: {}", e);
            }
        }

        // Step 4: Get boosted tokens from DexScreener (NEW DISCOVERY SOURCE)
        if let Some(ref dexscreener_client) = self.dexscreener_client {
            info!("🚀 Starting DexScreener boosted token discovery");
            
            // Get both latest and top boosted tokens
            match dexscreener_client.get_all_boosted_tokens().await {
                Ok((latest_tokens, top_tokens)) => {
                    // Process latest boosted tokens
                    if !latest_tokens.is_empty() {
                        info!("📈 Found {} latest boosted tokens", latest_tokens.len());
                        match self.process_boosted_tokens(&latest_tokens, "latest").await {
                            Ok(pushed_count) => {
                                total_discovered_wallets += pushed_count;
                                debug!("📤 Pushed {} wallets from latest boosted tokens", pushed_count);
                            }
                            Err(e) => {
                                warn!("❌ Failed to process latest boosted tokens: {}", e);
                            }
                        }
                    }
                    
                    // Process top boosted tokens
                    if !top_tokens.is_empty() {
                        info!("🏆 Found {} top boosted tokens", top_tokens.len());
                        match self.process_boosted_tokens(&top_tokens, "top").await {
                            Ok(pushed_count) => {
                                total_discovered_wallets += pushed_count;
                                debug!("📤 Pushed {} wallets from top boosted tokens", pushed_count);
                            }
                            Err(e) => {
                                warn!("❌ Failed to process top boosted tokens: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("❌ Failed to fetch boosted tokens from DexScreener: {}", e);
                }
            }
        } else {
            debug!("⭕ DexScreener client disabled, skipping boosted token discovery");
        }

        // Step 5: Get newly listed tokens (NEW DISCOVERY SOURCE)
        if self.config.birdeye.new_listing_enabled {
            info!("🆕 Starting new listing token discovery");
            
            match self.get_new_listing_tokens().await {
                Ok(new_listing_tokens) => {
                    if !new_listing_tokens.is_empty() {
                        info!("📈 Found {} new listing tokens", new_listing_tokens.len());
                        
                        match self.process_new_listing_tokens(&new_listing_tokens).await {
                            Ok(pushed_count) => {
                                total_discovered_wallets += pushed_count;
                                debug!("📤 Pushed {} wallets from new listing tokens", pushed_count);
                            }
                            Err(e) => {
                                warn!("❌ Failed to process new listing tokens: {}", e);
                            }
                        }
                    } else {
                        debug!("⭕ No new listing tokens found");
                    }
                }
                Err(e) => {
                    warn!("❌ Failed to fetch new listing tokens: {}", e);
                }
            }
        } else {
            debug!("⭕ New listing token discovery disabled");
        }

        info!("✅ Enhanced Multi-Source Discovery Cycle Completed: {} total quality wallets discovered", total_discovered_wallets);
        debug!("📊 Discovery breakdown: Paginated trending (unlimited tokens, 3 sorts × 5 pages = 15 calls) → paginated top traders (5x) | Paginated gainers (3 timeframes × 5 pages = 15 calls) → direct wallets | DexScreener boosted → paginated top traders (5x) | New listing tokens → paginated top traders (5x)");
        Ok(total_discovered_wallets)
    }

    /// Get top gainers across all timeframes with pagination
    async fn get_top_gainers(&self) -> Result<Vec<GainerLoser>> {
        debug!("💰 Fetching top gainers across all timeframes with pagination");

        match self.birdeye_client.get_gainers_losers_paginated().await {
            Ok(gainers) => {
                debug!("📊 Retrieved {} gainers across all timeframes and pages", gainers.len());
                Ok(gainers)
            }
            Err(e) => {
                error!("❌ Failed to fetch gainers: {}", e);
                Err(e.into())
            }
        }
    }

    /// Push gainer wallets to Redis queue for P&L analysis
    async fn push_gainers_to_queue(&self, gainers: &[GainerLoser], timeframe: &str) -> Result<usize> {
        if gainers.is_empty() {
            return Ok(0);
        }

        // Convert gainers to DiscoveredWalletToken format
        // For gainers, we don't have a specific token, so we'll use a generic identifier
        let wallet_token_pairs: Vec<DiscoveredWalletToken> = gainers.iter()
            .map(|gainer| DiscoveredWalletToken {
                wallet_address: gainer.address.clone(),
                token_address: "ALL_TOKENS".to_string(), // Generic for gainers
                token_symbol: format!("GAINER_{}", timeframe.to_uppercase()),
                trader_volume_usd: gainer.volume,
                trader_trades: gainer.trade_count,
                discovered_at: chrono::Utc::now(),
            })
            .collect();

        debug!("📤 Pushing {} gainer wallet-token pairs to Redis queue for timeframe {}", 
               wallet_token_pairs.len(), timeframe);

        let redis = self.redis_client.lock().await;
        if let Some(ref redis_client) = *redis {
            match redis_client.push_discovered_wallet_token_pairs_deduplicated(&wallet_token_pairs).await {
                Ok(pushed_count) => {
                    let skipped_count = wallet_token_pairs.len() - pushed_count;
                    if skipped_count > 0 {
                        info!("✅ Pushed {} new gainer wallet-token pairs for {} (skipped {} duplicates)", 
                              pushed_count, timeframe, skipped_count);
                    } else {
                        info!("✅ Successfully pushed {} gainer wallet-token pairs for {}", 
                              pushed_count, timeframe);
                    }
                    Ok(pushed_count)
                }
                Err(e) => {
                    error!("❌ Failed to push gainer wallet-token pairs to Redis queue: {}", e);
                    Err(e.into())
                }
            }
        } else {
            warn!("⚠️ Redis client not available, cannot push gainer wallet-token pairs");
            Ok(0)
        }
    }

    /// Process boosted tokens from DexScreener and get top traders for each
    async fn process_boosted_tokens(&self, boosted_tokens: &[DexScreenerBoostedToken], source: &str) -> Result<usize> {
        if boosted_tokens.is_empty() {
            return Ok(0);
        }

        debug!("🔄 Processing {} boosted tokens from {}", boosted_tokens.len(), source);
        
        // Limit to max boosted tokens (no filtering by boost amount needed)
        let mut processed_tokens = boosted_tokens.to_vec();
        if processed_tokens.len() > self.config.dexscreener.max_boosted_tokens as usize {
            processed_tokens.truncate(self.config.dexscreener.max_boosted_tokens as usize);
        }

        debug!("📊 Processing {} boosted tokens from {}", processed_tokens.len(), source);

        let mut total_discovered_wallets = 0;

        // For each boosted token, get top traders using BirdEye
        for (i, boosted_token) in processed_tokens.iter().enumerate() {
            debug!("🎯 Processing boosted token {}/{}: {}", 
                   i + 1, processed_tokens.len(), boosted_token.token_address);

            match self.get_top_traders_for_token(&boosted_token.token_address).await {
                Ok(top_traders) => {
                    if !top_traders.is_empty() {
                        info!("👤 Found {} quality traders for boosted token {} ({})", 
                              top_traders.len(), boosted_token.token_address, source);

                        // Create a synthetic "trending token" structure for boosted tokens
                        let synthetic_token = BirdEyeTrendingToken {
                            address: boosted_token.token_address.clone(),
                            symbol: format!("BOOSTED_{}", source.to_uppercase()),
                            name: boosted_token.description.clone().unwrap_or_else(|| "Boosted Token".to_string()),
                            decimals: None,
                            price: 0.0, // Default price for boosted tokens
                            price_change_24h: None,
                            volume_24h: Some(1000.0), // Default volume for boosted tokens
                            volume_change_24h: None,
                            liquidity: None,
                            fdv: None,
                            marketcap: None,
                            rank: None,
                            logo_uri: None,
                            txns_24h: None,
                            last_trade_unix_time: None,
                        };

                        // Push quality wallet-token pairs to Redis for P&L analysis
                        match self.push_wallet_token_pairs_to_queue(&top_traders, &synthetic_token).await {
                            Ok(pushed_count) => {
                                total_discovered_wallets += pushed_count;
                                debug!("📤 Pushed {} wallets to analysis queue for boosted token {}", 
                                       pushed_count, boosted_token.token_address);
                            }
                            Err(e) => {
                                warn!("❌ Failed to push wallets for boosted token {}: {}", 
                                      boosted_token.token_address, e);
                            }
                        }
                    } else {
                        debug!("⭕ No quality traders found for boosted token {}", boosted_token.token_address);
                    }
                }
                Err(e) => {
                    warn!("❌ Failed to get top traders for boosted token {}: {}", 
                          boosted_token.token_address, e);
                }
            }

            // Rate limiting between boosted tokens
            if i < processed_tokens.len() - 1 {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }

        debug!("✅ Boosted token processing completed: {} total wallets discovered from {}", 
               total_discovered_wallets, source);
        Ok(total_discovered_wallets)
    }

    /// Get trending tokens from BirdEye using enhanced multi-sort discovery
    async fn get_trending_tokens(&self) -> Result<Vec<BirdEyeTrendingToken>> {
        debug!("📊 Starting paginated trending token discovery from BirdEye");

        match self.birdeye_client.get_trending_tokens_paginated("solana").await {
            Ok(mut tokens) => {
                info!("🎯 Paginated discovery completed: {} unique tokens found across all pages", tokens.len());
                
                // Apply volume-based sorting (already done in multi-sort method but ensure consistency)
                tokens.sort_by(|a, b| b.volume_24h.partial_cmp(&a.volume_24h).unwrap_or(std::cmp::Ordering::Equal));
                
                // Apply max trending tokens limit (0 = unlimited)
                if self.config.birdeye.max_trending_tokens > 0 && tokens.len() > self.config.birdeye.max_trending_tokens {
                    tokens.truncate(self.config.birdeye.max_trending_tokens);
                    info!("📈 Processing trending tokens: {} tokens (limited to {})", tokens.len(), self.config.birdeye.max_trending_tokens);
                } else {
                    info!("📈 Processing all discovered trending tokens: {} tokens", tokens.len());
                }

                if self.config.system.debug_mode && !tokens.is_empty() {
                    debug!("🎯 Top trending tokens by multi-sort discovery:");
                    for (i, token) in tokens.iter().enumerate().take(8) {
                        debug!("  {}. {} ({}) - Vol: ${:.0}, Liq: ${:.0}, Change: {:.1}%", 
                               i + 1, token.symbol, token.address, 
                               token.volume_24h.unwrap_or(0.0),
                               token.liquidity.unwrap_or(0.0),
                               token.price_change_24h.unwrap_or(0.0));
                    }
                }

                Ok(tokens)
            }
            Err(e) => {
                error!("❌ Multi-sort trending token discovery failed: {}", e);
                warn!("🔄 Falling back to single-sort discovery method");
                
                // Fallback to original method
                match self.birdeye_client.get_trending_tokens_paginated("solana").await {
                    Ok(mut fallback_tokens) => {
                        fallback_tokens.sort_by(|a, b| b.volume_24h.partial_cmp(&a.volume_24h).unwrap_or(std::cmp::Ordering::Equal));
                        warn!("⚠️ Using fallback discovery: {} tokens retrieved", fallback_tokens.len());
                        Ok(fallback_tokens)
                    }
                    Err(fallback_e) => {
                        error!("❌ Both multi-sort and fallback discovery failed: {}", fallback_e);
                        Err(e.into())
                    }
                }
            }
        }
    }

    /// Get top traders for a specific token
    async fn get_top_traders_for_token(&self, token_address: &str) -> Result<Vec<TopTrader>> {
        debug!("👥 Fetching top traders for token: {}", token_address);

        match self.birdeye_client.get_top_traders_paginated(token_address).await {
            Ok(traders) => {
                debug!("📊 Retrieved {} raw traders for token {}", traders.len(), token_address);

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
                if filtered_traders.len() > self.config.birdeye.max_traders_per_token as usize {
                    filtered_traders.truncate(self.config.birdeye.max_traders_per_token as usize);
                }

                debug!("✅ Filtered to {} quality traders for token {}", 
                       filtered_traders.len(), token_address);

                if self.config.system.debug_mode && !filtered_traders.is_empty() {
                    for (i, trader) in filtered_traders.iter().enumerate().take(3) {
                        debug!("  {}. {} - Volume: ${:.0}, Trades: {}", 
                               i + 1, trader.owner, trader.volume, trader.trade);
                    }
                }

                Ok(filtered_traders)
            }
            Err(e) => {
                warn!("❌ Failed to fetch top traders for token {}: {}", token_address, e);
                Err(e.into())
            }
        }
    }

    /// Push quality wallet-token pairs to Redis queue for targeted P&L analysis
    async fn push_wallet_token_pairs_to_queue(&self, traders: &[TopTrader], token: &BirdEyeTrendingToken) -> Result<usize> {
        if traders.is_empty() {
            return Ok(0);
        }

        let wallet_token_pairs: Vec<DiscoveredWalletToken> = traders.iter()
            .map(|trader| DiscoveredWalletToken {
                wallet_address: trader.owner.clone(),
                token_address: token.address.clone(),
                token_symbol: token.symbol.clone(),
                trader_volume_usd: trader.volume,
                trader_trades: trader.trade,
                discovered_at: chrono::Utc::now(),
            })
            .collect();

        debug!("📤 Pushing {} wallet-token pairs to Redis queue for token {}", wallet_token_pairs.len(), token.symbol);

        let redis = self.redis_client.lock().await;
        if let Some(ref redis_client) = *redis {
            match redis_client.push_discovered_wallet_token_pairs_deduplicated(&wallet_token_pairs).await {
                Ok(pushed_count) => {
                    let skipped_count = wallet_token_pairs.len() - pushed_count;
                    if skipped_count > 0 {
                        info!("✅ Pushed {} new wallet-token pairs to analysis queue for {} (skipped {} duplicates)", 
                              pushed_count, token.symbol, skipped_count);
                    } else {
                        info!("✅ Successfully pushed {} quality wallet-token pairs to analysis queue for {}", 
                              pushed_count, token.symbol);
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

    /// Get new listing tokens with comprehensive coverage
    async fn get_new_listing_tokens(&self) -> Result<Vec<NewListingToken>> {
        debug!("🆕 Fetching new listing tokens with comprehensive coverage");
        
        let all_tokens = self.birdeye_client.get_new_listing_tokens_comprehensive("solana").await?;
        
        // Apply quality filtering
        let filter = NewListingTokenFilter {
            min_liquidity: Some(self.config.birdeye.new_listing_min_liquidity),
            max_age_hours: Some(self.config.birdeye.new_listing_max_age_hours),
            max_tokens: Some(self.config.birdeye.new_listing_max_tokens),
            exclude_sources: None,
        };
        
        let filtered_tokens = self.birdeye_client.filter_new_listing_tokens(all_tokens, &filter);
        
        info!("🎯 New listing discovery completed: {} quality tokens after filtering", filtered_tokens.len());
        
        if self.config.system.debug_mode && !filtered_tokens.is_empty() {
            debug!("🆕 Top new listing tokens:");
            for (i, token) in filtered_tokens.iter().enumerate().take(8) {
                debug!("  {}. {} ({}) - Liquidity: ${:.2}, Source: {}", 
                       i + 1, token.symbol, token.address, token.liquidity, token.source);
            }
        }
        
        Ok(filtered_tokens)
    }

    /// Process new listing tokens and get top traders for each
    async fn process_new_listing_tokens(&self, new_listing_tokens: &[NewListingToken]) -> Result<usize> {
        if new_listing_tokens.is_empty() {
            return Ok(0);
        }
        
        debug!("🔄 Processing {} new listing tokens", new_listing_tokens.len());
        
        let mut total_discovered_wallets = 0;
        
        for (i, token) in new_listing_tokens.iter().enumerate() {
            debug!("🎯 Processing new listing token {}/{}: {} ({})", 
                   i + 1, new_listing_tokens.len(), token.symbol, token.address);
            
            match self.get_top_traders_for_token(&token.address).await {
                Ok(top_traders) => {
                    if !top_traders.is_empty() {
                        info!("👤 Found {} quality traders for new listing token {} ({})", 
                              top_traders.len(), token.symbol, token.address);
                        
                        // Convert NewListingToken to TrendingToken format for compatibility
                        let synthetic_trending_token = BirdEyeTrendingToken {
                            address: token.address.clone(),
                            symbol: token.symbol.clone(),
                            name: token.name.clone(),
                            decimals: Some(token.decimals),
                            price: 0.0, // Will be fetched by price service
                            price_change_24h: None,
                            volume_24h: Some(token.liquidity), // Use liquidity as volume proxy
                            volume_change_24h: None,
                            liquidity: Some(token.liquidity),
                            fdv: None,
                            marketcap: None,
                            rank: None,
                            logo_uri: token.logo_uri.clone(),
                            txns_24h: None,
                            last_trade_unix_time: None,
                        };
                        
                        // Use existing wallet-token pair pushing logic
                        match self.push_wallet_token_pairs_to_queue(&top_traders, &synthetic_trending_token).await {
                            Ok(pushed_count) => {
                                total_discovered_wallets += pushed_count;
                                debug!("📤 Pushed {} wallets for new listing token {}", pushed_count, token.symbol);
                            }
                            Err(e) => {
                                warn!("❌ Failed to push wallets for new listing token {}: {}", token.symbol, e);
                            }
                        }
                    } else {
                        debug!("⭕ No quality traders found for new listing token {}", token.symbol);
                    }
                }
                Err(e) => {
                    warn!("❌ Failed to get top traders for new listing token {}: {}", token.symbol, e);
                }
            }
            
            // Rate limiting between tokens
            if i < new_listing_tokens.len() - 1 {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
        
        info!("✅ New listing token processing completed: {} total wallets discovered", total_discovered_wallets);
        Ok(total_discovered_wallets)
    }

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
    
    /// Convert ProcessedSwap to FinancialEvent
    pub fn to_financial_event(&self, wallet_address: &str) -> FinancialEvent {
        // Determine if this is a buy or sell based on the token being acquired
        let sol_mint = "So11111111111111111111111111111111111111112";
        let event_type = if self.token_out == sol_mint {
            EventType::Sell // Selling token for SOL
        } else {
            EventType::Buy // Buying token with something else
        };
        
        let (token_mint, token_amount, sol_amount) = if event_type == EventType::Buy {
            (self.token_out.clone(), self.amount_out, -self.sol_equivalent)
        } else {
            (self.token_in.clone(), self.amount_in, self.sol_equivalent)
        };
        
        let timestamp = DateTime::from_timestamp(self.timestamp, 0)
            .unwrap_or_else(Utc::now);
        
        let mut extra = HashMap::new();
        extra.insert("source".to_string(), self.source.clone());
        extra.insert("tx_hash".to_string(), self.tx_hash.clone());
        
        FinancialEvent {
            id: Uuid::new_v4(),
            transaction_id: self.tx_hash.clone(),
            wallet_address: wallet_address.to_string(),
            event_type,
            token_mint,
            token_amount,
            sol_amount,
            usd_value: self.price_per_token * token_amount,
            timestamp,
            transaction_fee: Decimal::ZERO,
            metadata: EventMetadata {
                program_id: None,
                instruction_index: None,
                exchange: Some(self.source.clone()),
                price_per_token: Some(self.price_per_token),
                extra,
            },
        }
    }
}

// Tests removed - will use integration tests with SystemConfig