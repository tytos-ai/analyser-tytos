use anyhow::Result;
use config_manager::SystemConfig;
use dex_client::{BirdEyeClient, TopTrader, TrendingToken as BirdEyeTrendingToken, TraderTransaction};
use persistence_layer::{RedisClient, DiscoveredWalletToken};
// Removed unused serde imports
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

// BirdEyeTrendingConfig removed - now uses SystemConfig directly

/// Orchestrates trending token discovery and top trader identification using BirdEye API
pub struct BirdEyeTrendingOrchestrator {
    config: SystemConfig,
    birdeye_client: BirdEyeClient,
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
        
        Ok(Self {
            config,
            birdeye_client,
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

        info!("🚀 Starting BirdEye trending discovery orchestrator");
        info!("📋 Configuration: chain=solana, max_tokens={}, cycle_interval={}s", 
              self.config.dexscreener.trending.max_tokens_per_cycle, self.config.dexscreener.trending.polling_interval_seconds);

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
            tokio::time::sleep(Duration::from_secs(self.config.dexscreener.trending.polling_interval_seconds)).await;
        }

        Ok(())
    }

    /// Stop the trending discovery loop
    pub async fn stop(&self) {
        let mut is_running = self.is_running.lock().await;
        *is_running = false;
        info!("🛑 BirdEye trending orchestrator stop requested");
    }

    /// Execute one complete discovery cycle
    pub async fn execute_discovery_cycle(&self) -> Result<usize> {
        debug!("🔄 Starting BirdEye discovery cycle");

        // Step 1: Get trending tokens
        let trending_tokens = self.get_trending_tokens().await?;
        if trending_tokens.is_empty() {
            debug!("📊 No trending tokens found");
            return Ok(0);
        }

        info!("📈 Found {} trending tokens", trending_tokens.len());

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

        debug!("✅ Discovery cycle completed: {} total wallets discovered", total_discovered_wallets);
        Ok(total_discovered_wallets)
    }

    /// Get trending tokens from BirdEye
    async fn get_trending_tokens(&self) -> Result<Vec<BirdEyeTrendingToken>> {
        debug!("📊 Fetching trending tokens from BirdEye");

        match self.birdeye_client.get_trending_tokens("solana").await {
            Ok(mut tokens) => {
                // Apply basic filtering and sorting
                tokens.sort_by(|a, b| b.volume_24h.partial_cmp(&a.volume_24h).unwrap_or(std::cmp::Ordering::Equal));
                
                // Limit to max trending tokens
                if tokens.len() > self.config.dexscreener.trending.max_tokens_per_cycle as usize {
                    tokens.truncate(self.config.dexscreener.trending.max_tokens_per_cycle as usize);
                }

                debug!("📈 Retrieved {} trending tokens (limited to {})", 
                       tokens.len(), self.config.dexscreener.trending.max_tokens_per_cycle);

                if self.config.system.debug_mode {
                    for (i, token) in tokens.iter().enumerate().take(5) {
                        debug!("  {}. {} ({}) - Volume: ${:.0}, Change: {:.1}%", 
                               i + 1, token.symbol, token.address, 
                               token.volume_24h.unwrap_or(0.0), 
                               token.price_change_24h.unwrap_or(0.0));
                    }
                }

                Ok(tokens)
            }
            Err(e) => {
                error!("❌ Failed to fetch trending tokens from BirdEye: {}", e);
                Err(e.into())
            }
        }
    }

    /// Get top traders for a specific token
    async fn get_top_traders_for_token(&self, token_address: &str) -> Result<Vec<TopTrader>> {
        debug!("👥 Fetching top traders for token: {}", token_address);

        match self.birdeye_client.get_top_traders(token_address, Some(self.config.birdeye.max_traders_per_token)).await {
            Ok(traders) => {
                debug!("📊 Retrieved {} raw traders for token {}", traders.len(), token_address);

                // Apply quality filtering using trader filter config
                let quality_traders = self.birdeye_client.filter_top_traders(
                    traders,
                    self.config.trader_filter.min_capital_deployed_sol as f64 * 230.0, // Convert SOL to USD roughly
                    self.config.trader_filter.min_total_trades as u32,
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

    /// Get wallet transaction history for a specific wallet and token (optional utility method)
    pub async fn get_wallet_transaction_history(
        &self,
        wallet_address: &str,
        token_address: &str,
        from_time: Option<i64>,
        to_time: Option<i64>,
    ) -> Result<Vec<TraderTransaction>> {
        debug!("📜 Fetching transaction history for wallet {} token {}", wallet_address, token_address);

        match self.birdeye_client.get_trader_transactions(
            wallet_address,
            token_address,
            from_time,
            to_time,
            Some(self.config.birdeye.max_transactions_per_trader), // Configurable limit
        ).await {
            Ok(transactions) => {
                debug!("📊 Retrieved {} transactions for wallet {} token {}", 
                       transactions.len(), wallet_address, token_address);
                Ok(transactions)
            }
            Err(e) => {
                warn!("❌ Failed to fetch transaction history for wallet {} token {}: {}", 
                      wallet_address, token_address, e);
                Err(e.into())
            }
        }
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
            })
        } else {
            Ok(DiscoveryStats {
                is_running: *self.is_running.lock().await,
                wallet_queue_size: 0,
                config: self.config.clone(),
                tokens_discovered: 0,
                wallet_token_pairs_discovered: 0,
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
}

// Tests removed - will use integration tests with SystemConfig