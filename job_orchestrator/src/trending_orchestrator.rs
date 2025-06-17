use anyhow::Result;
use config_manager::{SystemConfig, TrendingConfig};
use dex_client::{DexClient, TrendingToken};
use persistence_layer::RedisClient;
use solana_client::SolanaClient;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

/// Orchestrates the complete trending discovery and wallet analysis pipeline
pub struct TrendingOrchestrator {
    config: SystemConfig,
    dex_client: DexClient,
    solana_client: SolanaClient,
    redis_client: Arc<Mutex<Option<RedisClient>>>,
    is_running: Arc<Mutex<bool>>,
}

impl TrendingOrchestrator {
    pub async fn new(
        config: SystemConfig,
        redis_client: Option<RedisClient>,
    ) -> Result<Self> {
        // Initialize Solana client
        let solana_config = solana_client::SolanaClientConfig {
            rpc_url: config.solana.rpc_url.clone(),
            rpc_timeout_seconds: config.solana.rpc_timeout_seconds,
            max_concurrent_requests: config.solana.max_concurrent_requests as usize,
            max_signatures: config.solana.max_signatures as u64,
        };
        let solana_client = SolanaClient::new(solana_config)?;

        // Initialize DexScreener client
        let dex_config = dex_client::DexClientConfig {
            api_base_url: config.dexscreener.api_base_url.clone(),
            ws_url: config.dexscreener.websocket_url.clone(),
            http_base_url: config.dexscreener.http_base_url.clone(),
            request_timeout_seconds: 15,
            debug: config.system.debug_mode,
            trending_criteria: dex_client::TrendingCriteria {
                min_volume_24h: config.dexscreener.trending.min_volume_24h,
                min_txns_24h: config.dexscreener.trending.min_txns_24h,
                min_liquidity_usd: config.dexscreener.trending.min_liquidity_usd,
                min_price_change_24h: config.dexscreener.trending.min_price_change_24h,
                max_pair_age_hours: config.dexscreener.trending.max_pair_age_hours,
            },
        };
        let dex_client = DexClient::new(dex_config, redis_client.clone()).await?;

        Ok(Self {
            config,
            dex_client,
            solana_client,
            redis_client: Arc::new(Mutex::new(redis_client)),
            is_running: Arc::new(Mutex::new(false)),
        })
    }

    /// Start the trending discovery and wallet analysis pipeline
    pub async fn start_trending_pipeline(&mut self) -> Result<()> {
        let mut is_running = self.is_running.lock().await;
        if *is_running {
            warn!("Trending pipeline is already running");
            return Ok(());
        }
        *is_running = true;
        drop(is_running);

        info!("🚀 Starting trending discovery and wallet analysis pipeline");
        info!("📊 Trending criteria:");
        info!("   • Min volume 24h: ${:.0}", self.config.dexscreener.trending.min_volume_24h);
        info!("   • Min transactions 24h: {}", self.config.dexscreener.trending.min_txns_24h);
        info!("   • Min liquidity: ${:.0}", self.config.dexscreener.trending.min_liquidity_usd);
        info!("   • Polling interval: {}s", self.config.dexscreener.trending.polling_interval_seconds);

        loop {
            // Check if we should stop
            let running = *self.is_running.lock().await;
            if !running {
                info!("Trending pipeline stopped");
                break;
            }

            // Run one cycle of trending discovery and analysis
            match self.run_trending_cycle().await {
                Ok(stats) => {
                    info!("✅ Trending cycle completed: {} tokens discovered, {} wallets found", 
                          stats.tokens_discovered, stats.wallets_discovered);
                }
                Err(e) => {
                    error!("❌ Trending cycle failed: {}", e);
                }
            }

            // Sleep until next cycle
            tokio::time::sleep(Duration::from_secs(
                self.config.dexscreener.trending.polling_interval_seconds
            )).await;
        }

        Ok(())
    }

    /// Stop the trending pipeline
    pub async fn stop_trending_pipeline(&self) {
        let mut is_running = self.is_running.lock().await;
        *is_running = false;
        info!("🛑 Stopping trending discovery pipeline");
    }

    /// Run a single cycle of trending discovery and wallet analysis
    pub async fn run_trending_cycle(&self) -> Result<TrendingCycleStats> {
        info!("🔍 Starting trending discovery cycle...");

        let mut stats = TrendingCycleStats::default();

        // Step 1: Discover trending tokens via DexScreener
        let trending_tokens = match self.dex_client.discover_trending_tokens().await {
            Ok(tokens) => {
                stats.tokens_discovered = tokens.len();
                if !tokens.is_empty() {
                    info!("📈 Discovered {} trending tokens:", tokens.len());
                    for (i, token) in tokens.iter().take(5).enumerate() {
                        if let Some(ref pair) = token.top_pair {
                            info!("  {}. {}/{} - Volume: ${:.0}, Txns: {}, Change: {:.1}%", 
                                  i + 1,
                                  pair.base_token_symbol,
                                  pair.quote_token_symbol,
                                  pair.volume_24h,
                                  pair.txns_24h,
                                  pair.price_change_24h);
                        }
                    }
                }
                tokens
            }
            Err(e) => {
                error!("Failed to discover trending tokens: {}", e);
                stats.errors.push(format!("Trending discovery failed: {}", e));
                return Ok(stats);
            }
        };

        if trending_tokens.is_empty() {
            info!("No trending tokens found in this cycle");
            return Ok(stats);
        }

        // Step 2: Discover wallets from trending pairs
        let pair_addresses: Vec<String> = trending_tokens
            .iter()
            .filter_map(|token| token.top_pair.as_ref().map(|pair| pair.pair_address.clone()))
            .collect();

        if !pair_addresses.is_empty() {
            match self.solana_client.discover_wallets_from_pairs(&pair_addresses).await {
                Ok(discovered_wallets) => {
                    stats.wallets_discovered = discovered_wallets.len();
                    
                    if !discovered_wallets.is_empty() {
                        info!("👛 Discovered {} unique wallets from trending pairs", discovered_wallets.len());
                        
                        // Step 3: Push discovered wallets to Redis queue for P&L analysis
                        if let Err(e) = self.push_wallets_to_queue(&discovered_wallets).await {
                            error!("Failed to push wallets to queue: {}", e);
                            stats.errors.push(format!("Queue push failed: {}", e));
                        } else {
                            stats.wallets_queued = discovered_wallets.len();
                            info!("📤 Pushed {} wallets to P&L analysis queue", discovered_wallets.len());
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to discover wallets from trending pairs: {}", e);
                    stats.errors.push(format!("Wallet discovery failed: {}", e));
                }
            }
        }

        // Step 4: Store trending statistics
        if let Err(e) = self.store_trending_stats(&trending_tokens, &stats).await {
            warn!("Failed to store trending statistics: {}", e);
        }

        Ok(stats)
    }

    /// Push discovered wallets to Redis queue for P&L analysis
    async fn push_wallets_to_queue(&self, wallets: &[String]) -> Result<()> {
        let redis = self.redis_client.lock().await;
        if let Some(ref redis_client) = *redis {
            redis_client.push_discovered_wallets(wallets).await?;
        } else {
            warn!("Redis client not available, cannot queue wallets");
        }
        Ok(())
    }

    /// Store trending analysis statistics in Redis
    async fn store_trending_stats(&self, trending_tokens: &[TrendingToken], stats: &TrendingCycleStats) -> Result<()> {
        let redis = self.redis_client.lock().await;
        if let Some(ref redis_client) = *redis {
            let stats_data = serde_json::json!({
                "timestamp": chrono::Utc::now().timestamp(),
                "tokens_discovered": stats.tokens_discovered,
                "wallets_discovered": stats.wallets_discovered,
                "wallets_queued": stats.wallets_queued,
                "errors": stats.errors,
                "trending_tokens": trending_tokens.iter().take(10).map(|t| {
                    serde_json::json!({
                        "token_address": t.token_address,
                        "boost_amount": t.boost_amount,
                        "pair": t.top_pair.as_ref().map(|p| {
                            serde_json::json!({
                                "pair_address": p.pair_address,
                                "base_symbol": p.base_token_symbol,
                                "quote_symbol": p.quote_token_symbol,
                                "volume_24h": p.volume_24h,
                                "txns_24h": p.txns_24h,
                                "price_change_24h": p.price_change_24h
                            })
                        })
                    })
                }).collect::<Vec<_>>(),
                "config": {
                    "min_volume_24h": self.config.dexscreener.trending.min_volume_24h,
                    "min_txns_24h": self.config.dexscreener.trending.min_txns_24h,
                    "min_liquidity_usd": self.config.dexscreener.trending.min_liquidity_usd
                }
            });

            redis_client.store_trending_stats(&stats_data, 3600).await?; // 1 hour TTL
        }
        Ok(())
    }

    /// Get current queue size for discovered wallets
    pub async fn get_wallet_queue_size(&self) -> Result<u64> {
        let redis = self.redis_client.lock().await;
        if let Some(ref redis_client) = *redis {
            redis_client.get_wallet_queue_size().await.map_err(|e| anyhow::anyhow!(e))
        } else {
            Ok(0)
        }
    }

    /// Get trending analysis statistics
    pub async fn get_trending_stats(&self) -> Result<Option<serde_json::Value>> {
        let redis = self.redis_client.lock().await;
        if let Some(ref redis_client) = *redis {
            redis_client.get_trending_stats().await.map_err(|e| anyhow::anyhow!(e))
        } else {
            Ok(None)
        }
    }

    /// Update trending criteria
    pub async fn update_trending_criteria(&mut self, criteria: TrendingConfig) -> Result<()> {
        self.config.dexscreener.trending = criteria;
        info!("Updated trending criteria");
        // Note: Would need to reinitialize dex_client with new criteria
        // For now, just update the config - full reinitialization can be added later
        Ok(())
    }

    /// Get current trending criteria
    pub fn get_trending_criteria(&self) -> &TrendingConfig {
        &self.config.dexscreener.trending
    }

    /// Check if the trending pipeline is running
    pub async fn is_running(&self) -> bool {
        *self.is_running.lock().await
    }

    /// Run a manual trending analysis (one-time execution)
    pub async fn run_manual_trending_analysis(&self) -> Result<TrendingCycleStats> {
        info!("🔍 Running manual trending analysis...");
        self.run_trending_cycle().await
    }
}

#[derive(Debug, Default)]
pub struct TrendingCycleStats {
    pub tokens_discovered: usize,
    pub wallets_discovered: usize,
    pub wallets_queued: usize,
    pub errors: Vec<String>,
}

impl TrendingCycleStats {
    pub fn success_rate(&self) -> f64 {
        if self.tokens_discovered == 0 {
            0.0
        } else {
            (self.tokens_discovered - self.errors.len()) as f64 / self.tokens_discovered as f64
        }
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trending_cycle_stats() {
        let mut stats = TrendingCycleStats::default();
        assert_eq!(stats.success_rate(), 0.0);
        assert!(!stats.has_errors());

        stats.tokens_discovered = 10;
        stats.wallets_discovered = 50;
        stats.errors.push("test error".to_string());
        
        assert!(stats.has_errors());
        assert_eq!(stats.success_rate(), 0.9); // 9/10 successful
    }
}