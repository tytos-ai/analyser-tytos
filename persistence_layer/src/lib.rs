use redis::{AsyncCommands, Client};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, error, info, warn};
use rust_decimal::Decimal;

/// Discovered wallet-token pair for targeted P&L analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredWalletToken {
    /// Wallet address of the trader
    pub wallet_address: String,
    /// Token address that this wallet was discovered trading
    pub token_address: String,
    /// Token symbol (for logging/display)
    pub token_symbol: String,
    /// Trader's volume on this token (USD)
    pub trader_volume_usd: f64,
    /// Number of trades on this token
    pub trader_trades: u32,
    /// Discovery timestamp
    pub discovered_at: chrono::DateTime<chrono::Utc>,
}

/// Stored P&L analysis result with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredPnLResult {
    pub wallet_address: String,
    pub token_address: String,
    pub token_symbol: String,
    pub pnl_report: pnl_core::PnLReport,
    pub analyzed_at: chrono::DateTime<chrono::Utc>,
}

/// Aggregated P&L summary statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PnLSummaryStats {
    pub total_wallets_analyzed: u64,
    pub profitable_wallets: u64,
    pub total_pnl_usd: Decimal,
    pub total_realized_pnl_usd: Decimal,
    pub average_pnl_usd: Decimal,
    pub total_trades: u64,
    pub profitability_rate: f64, // percentage
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

impl Default for PnLSummaryStats {
    fn default() -> Self {
        Self {
            total_wallets_analyzed: 0,
            profitable_wallets: 0,
            total_pnl_usd: Decimal::ZERO,
            total_realized_pnl_usd: Decimal::ZERO,
            average_pnl_usd: Decimal::ZERO,
            total_trades: 0,
            profitability_rate: 0.0,
            last_updated: chrono::Utc::now(),
        }
    }
}

/// Redis health status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisHealthStatus {
    pub connected: bool,
    pub latency_ms: u64,
    pub error: Option<String>,
}

#[derive(Error, Debug)]
pub enum PersistenceError {
    #[error("Redis connection error: {0}")]
    Connection(#[from] redis::RedisError),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Lock acquisition failed")]
    LockFailed,
    #[error("Lock not found")]
    LockNotFound,
}

pub type Result<T> = std::result::Result<T, PersistenceError>;

#[derive(Debug, Clone)]
pub struct RedisClient {
    client: Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockHandle {
    pub key: String,
    pub value: String,
    pub ttl_seconds: u64,
}

impl RedisClient {
    pub async fn new(redis_url: &str) -> Result<Self> {
        let client = Client::open(redis_url)?;
        
        // Test the connection
        let mut conn = client.get_async_connection().await?;
        let _: String = redis::cmd("PING").query_async(&mut conn).await?;
        
        Ok(Self { client })
    }
    
    async fn get_connection(&self) -> Result<redis::aio::Connection> {
        self.client.get_async_connection().await.map_err(PersistenceError::from)
    }

    // =====================================
    // Trending Pairs Management
    // =====================================
    
    /// Set a trending pair as not extracted
    pub async fn set_trending_pair(&self, pair: &str) -> Result<bool> {
        let key = format!("trending:{}", pair);
        let mut conn = self.get_connection().await?;
        let was_new: bool = conn.hset_nx(&key, "extracted", "false").await?;
        
        if was_new {
            debug!("New trending pair added: {}", pair);
        }
        
        Ok(was_new)
    }

    /// Mark a trending pair as extracted
    pub async fn mark_pair_extracted(&self, pair: &str) -> Result<()> {
        let key = format!("trending:{}", pair);
        let mut conn = self.get_connection().await?;
        let _: () = conn.hset(&key, "extracted", "true").await?;
        debug!("Marked pair as extracted: {}", pair);
        Ok(())
    }

    /// Get all pairs that haven't been extracted yet
    pub async fn get_unextracted_pairs(&self) -> Result<Vec<String>> {
        let mut pairs = Vec::new();
        let mut conn = self.get_connection().await?;
        
        // Use KEYS command for simplicity (note: not recommended for production with large datasets)
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg("trending:*")
            .query_async(&mut conn)
            .await?;
        
        for key in keys {
            let extracted: Option<String> = conn.hget(&key, "extracted").await?;
            if extracted == Some("false".to_string()) {
                // Extract pair name from key
                if let Some(pair) = key.strip_prefix("trending:") {
                    pairs.push(pair.to_string());
                }
            }
        }
        
        debug!("Found {} unextracted pairs", pairs.len());
        Ok(pairs)
    }

    // =====================================
    // Wallet Queue Management
    // =====================================
    
    /// Push discovered wallet addresses to the queue
    pub async fn push_discovered_wallets(&self, wallets: &[String]) -> Result<()> {
        if wallets.is_empty() {
            return Ok(());
        }
        
        let queue_key = "discovered_wallets_queue";
        let mut conn = self.get_connection().await?;
        let _: () = conn.lpush(queue_key, wallets).await?;
        info!("Pushed {} wallets to discovery queue", wallets.len());
        Ok(())
    }

    /// Pop a wallet address from the queue (blocking with timeout)
    pub async fn pop_discovered_wallet(&self, timeout_seconds: u64) -> Result<Option<String>> {
        let queue_key = "discovered_wallets_queue";
        let mut conn = self.get_connection().await?;
        let result: Option<Vec<String>> = conn
            .brpop(queue_key, timeout_seconds as f64)
            .await?;
        
        match result {
            Some(mut items) if items.len() >= 2 => {
                // brpop returns [key, value]
                let wallet = items.pop().unwrap();
                debug!("Popped wallet from queue: {}", wallet);
                Ok(Some(wallet))
            }
            _ => Ok(None),
        }
    }

    /// Get the current size of the wallet queue
    pub async fn get_wallet_queue_size(&self) -> Result<u64> {
        let queue_key = "discovered_wallets_queue";
        let mut conn = self.get_connection().await?;
        let size: u64 = conn.llen(queue_key).await?;
        Ok(size)
    }

    /// Push discovered wallet-token pairs to the queue for targeted P&L analysis
    pub async fn push_discovered_wallet_token_pairs(&self, wallet_tokens: &[DiscoveredWalletToken]) -> Result<()> {
        if wallet_tokens.is_empty() {
            return Ok(());
        }
        
        let queue_key = "discovered_wallet_token_pairs_queue";
        let mut conn = self.get_connection().await?;
        
        // Serialize each wallet-token pair as JSON
        let json_pairs: Result<Vec<String>> = wallet_tokens
            .iter()
            .map(|wt| serde_json::to_string(wt).map_err(PersistenceError::from))
            .collect();
        
        let json_pairs = json_pairs?;
        let _: () = conn.lpush(queue_key, json_pairs).await?;
        info!("Pushed {} wallet-token pairs to discovery queue", wallet_tokens.len());
        Ok(())
    }

    /// Pop a wallet-token pair from the queue (blocking with timeout)
    pub async fn pop_discovered_wallet_token_pair(&self, timeout_seconds: u64) -> Result<Option<DiscoveredWalletToken>> {
        let queue_key = "discovered_wallet_token_pairs_queue";
        let mut conn = self.get_connection().await?;
        let result: Option<Vec<String>> = conn
            .brpop(queue_key, timeout_seconds as f64)
            .await?;
        
        match result {
            Some(mut items) if items.len() >= 2 => {
                // brpop returns [key, value]
                let json_data = items.pop().unwrap();
                let wallet_token: DiscoveredWalletToken = serde_json::from_str(&json_data)?;
                debug!("Popped wallet-token pair from queue: {} for token {}", 
                       wallet_token.wallet_address, wallet_token.token_symbol);
                Ok(Some(wallet_token))
            }
            _ => Ok(None),
        }
    }

    /// Get the current size of the wallet-token pairs queue
    pub async fn get_wallet_token_pairs_queue_size(&self) -> Result<u64> {
        let queue_key = "discovered_wallet_token_pairs_queue";
        let mut conn = self.get_connection().await?;
        let size: u64 = conn.llen(queue_key).await?;
        Ok(size)
    }

    // =====================================
    // P&L Results Storage and Aggregation
    // =====================================
    
    /// Store P&L analysis result for a wallet-token pair
    pub async fn store_pnl_result(
        &self,
        wallet_address: &str,
        token_address: &str,
        token_symbol: &str,
        pnl_report: &pnl_core::PnLReport,
    ) -> Result<()> {
        let result_key = format!("pnl_result:{}:{}", wallet_address, token_address);
        
        let stored_result = StoredPnLResult {
            wallet_address: wallet_address.to_string(),
            token_address: token_address.to_string(),
            token_symbol: token_symbol.to_string(),
            pnl_report: pnl_report.clone(),
            analyzed_at: chrono::Utc::now(),
        };
        
        let result_json = serde_json::to_string(&stored_result)?;
        let mut conn = self.get_connection().await?;
        
        // Store individual result
        let _: () = conn.set(&result_key, &result_json).await?;
        
        // Add to results index for easy retrieval
        let _: () = conn.sadd("pnl_results_index", &result_key).await?;
        
        // Update summary statistics
        self.update_pnl_summary_stats(pnl_report).await?;
        
        info!("Stored P&L result for wallet {} token {}", wallet_address, token_symbol);
        Ok(())
    }
    
    /// Get P&L result for a specific wallet-token pair
    pub async fn get_pnl_result(
        &self,
        wallet_address: &str,
        token_address: &str,
    ) -> Result<Option<StoredPnLResult>> {
        let result_key = format!("pnl_result:{}:{}", wallet_address, token_address);
        let mut conn = self.get_connection().await?;
        
        let result_json: Option<String> = conn.get(&result_key).await?;
        
        match result_json {
            Some(json) => {
                let result: StoredPnLResult = serde_json::from_str(&json)?;
                Ok(Some(result))
            }
            None => Ok(None),
        }
    }
    
    /// Get all P&L results with pagination
    pub async fn get_all_pnl_results(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<StoredPnLResult>, usize)> {
        let mut conn = self.get_connection().await?;
        
        // Get all result keys
        let result_keys: Vec<String> = conn.smembers("pnl_results_index").await?;
        let total_count = result_keys.len();
        
        // Apply pagination
        let paginated_keys: Vec<String> = result_keys
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect();
        
        let mut results = Vec::new();
        
        // Fetch results in batch
        for key in paginated_keys {
            let result_json: Option<String> = conn.get(&key).await?;
            if let Some(json) = result_json {
                if let Ok(result) = serde_json::from_str::<StoredPnLResult>(&json) {
                    results.push(result);
                }
            }
        }
        
        // Sort by analysis time (newest first)
        results.sort_by(|a, b| b.analyzed_at.cmp(&a.analyzed_at));
        
        Ok((results, total_count))
    }
    
    /// Update summary statistics for aggregated P&L data
    async fn update_pnl_summary_stats(&self, pnl_report: &pnl_core::PnLReport) -> Result<()> {
        let mut conn = self.get_connection().await?;
        let stats_key = "pnl_summary_stats";
        
        // Get current stats or create new ones
        let current_stats_json: Option<String> = conn.get(&stats_key).await?;
        let mut stats = match current_stats_json {
            Some(json) => serde_json::from_str::<PnLSummaryStats>(&json)
                .unwrap_or_default(),
            None => PnLSummaryStats::default(),
        };
        
        // Update stats
        stats.total_wallets_analyzed += 1;
        stats.total_pnl_usd += pnl_report.summary.total_pnl_usd;
        stats.total_realized_pnl_usd += pnl_report.summary.realized_pnl_usd;
        stats.total_trades += pnl_report.summary.total_trades as u64;
        
        if pnl_report.summary.total_pnl_usd > Decimal::ZERO {
            stats.profitable_wallets += 1;
        }
        
        // Calculate averages
        if stats.total_wallets_analyzed > 0 {
            stats.average_pnl_usd = stats.total_pnl_usd / Decimal::from(stats.total_wallets_analyzed);
            stats.profitability_rate = (stats.profitable_wallets as f64 / stats.total_wallets_analyzed as f64) * 100.0;
        }
        
        stats.last_updated = chrono::Utc::now();
        
        // Store updated stats
        let stats_json = serde_json::to_string(&stats)?;
        let _: () = conn.set(&stats_key, &stats_json).await?;
        
        Ok(())
    }
    
    /// Get aggregated P&L summary statistics
    pub async fn get_pnl_summary_stats(&self) -> Result<PnLSummaryStats> {
        let mut conn = self.get_connection().await?;
        let stats_key = "pnl_summary_stats";
        
        let stats_json: Option<String> = conn.get(&stats_key).await?;
        
        match stats_json {
            Some(json) => {
                let stats: PnLSummaryStats = serde_json::from_str(&json)?;
                Ok(stats)
            }
            None => Ok(PnLSummaryStats::default()),
        }
    }

    // =====================================
    // Health Checks and Connectivity
    // =====================================
    
    /// Test Redis connectivity and health
    pub async fn health_check(&self) -> Result<RedisHealthStatus> {
        let start_time = std::time::Instant::now();
        
        match self.get_connection().await {
            Ok(mut conn) => {
                // Test basic operations
                let test_key = "health_check_test";
                let test_value = "ok";
                
                // Test SET operation
                let set_result: redis::RedisResult<String> = conn.set(test_key, test_value).await;
                if set_result.is_err() {
                    return Ok(RedisHealthStatus {
                        connected: false,
                        latency_ms: start_time.elapsed().as_millis() as u64,
                        error: Some("Failed to execute SET command".to_string()),
                    });
                }
                
                // Test GET operation
                let get_result: redis::RedisResult<String> = conn.get(test_key).await;
                match get_result {
                    Ok(value) if value == test_value => {
                        // Cleanup test key
                        let _: redis::RedisResult<u32> = conn.del(test_key).await;
                        
                        Ok(RedisHealthStatus {
                            connected: true,
                            latency_ms: start_time.elapsed().as_millis() as u64,
                            error: None,
                        })
                    }
                    Ok(_) => Ok(RedisHealthStatus {
                        connected: false,
                        latency_ms: start_time.elapsed().as_millis() as u64,
                        error: Some("GET returned unexpected value".to_string()),
                    }),
                    Err(e) => Ok(RedisHealthStatus {
                        connected: false,
                        latency_ms: start_time.elapsed().as_millis() as u64,
                        error: Some(format!("GET command failed: {}", e)),
                    }),
                }
            }
            Err(e) => Ok(RedisHealthStatus {
                connected: false,
                latency_ms: start_time.elapsed().as_millis() as u64,
                error: Some(format!("Connection failed: {}", e)),
            }),
        }
    }

    // =====================================
    // Distributed Lock Management
    // =====================================
    
    /// Acquire a distributed lock
    pub async fn acquire_lock(&self, lock_name: &str, ttl_seconds: u64) -> Result<LockHandle> {
        let key = format!("lock:{}", lock_name);
        let value = format!("{}:{}", 
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );
        
        let mut conn = self.get_connection().await?;
        
        // Try to set the lock with NX (only if not exists) and EX (expiration)
        let acquired: bool = conn
            .set_nx(&key, &value)
            .await?;
        
        if !acquired {
            return Err(PersistenceError::LockFailed);
        }
        
        // Set TTL
        let _: () = conn.expire(&key, ttl_seconds as i64).await?;
        
        let handle = LockHandle {
            key,
            value,
            ttl_seconds,
        };
        
        info!("Acquired lock: {}", lock_name);
        Ok(handle)
    }

    /// Release a distributed lock
    pub async fn release_lock(&self, handle: &LockHandle) -> Result<()> {
        // Use Lua script to ensure atomic check-and-delete
        let script = r#"
            if redis.call("GET", KEYS[1]) == ARGV[1] then
                return redis.call("DEL", KEYS[1])
            else
                return 0
            end
        "#;
        
        let mut conn = self.get_connection().await?;
        let result: i32 = redis::Script::new(script)
            .key(&handle.key)
            .arg(&handle.value)
            .invoke_async(&mut conn)
            .await?;
        
        if result == 1 {
            info!("Released lock: {}", handle.key);
            Ok(())
        } else {
            warn!("Lock was already expired or held by another process: {}", handle.key);
            Err(PersistenceError::LockNotFound)
        }
    }

    /// Refresh/extend a lock's TTL
    pub async fn refresh_lock(&self, handle: &LockHandle) -> Result<()> {
        let script = r#"
            if redis.call("GET", KEYS[1]) == ARGV[1] then
                return redis.call("EXPIRE", KEYS[1], ARGV[2])
            else
                return 0
            end
        "#;
        
        let mut conn = self.get_connection().await?;
        let result: i32 = redis::Script::new(script)
            .key(&handle.key)
            .arg(&handle.value)
            .arg(handle.ttl_seconds)
            .invoke_async(&mut conn)
            .await?;
        
        if result == 1 {
            debug!("Refreshed lock: {}", handle.key);
            Ok(())
        } else {
            Err(PersistenceError::LockNotFound)
        }
    }

    // =====================================
    // Price Caching
    // =====================================
    
    /// Cache Jupiter token prices
    pub async fn cache_token_prices(
        &self,
        token_mints: &[String],
        vs_token: &str,
        prices: &std::collections::HashMap<String, f64>,
        ttl_seconds: u64,
    ) -> Result<()> {
        let sorted_mints = {
            let mut mints = token_mints.to_vec();
            mints.sort();
            mints.join("-")
        };
        let cache_key = format!("jupiterPrice:{}:{}", sorted_mints, vs_token);
        
        let prices_json = serde_json::to_string(prices)?;
        let mut conn = self.get_connection().await?;
        let _: () = conn.set_ex(&cache_key, prices_json, ttl_seconds)
            .await?;
        
        debug!("Cached prices for {} tokens", token_mints.len());
        Ok(())
    }

    /// Get cached Jupiter token prices
    pub async fn get_cached_token_prices(
        &self,
        token_mints: &[String],
        vs_token: &str,
    ) -> Result<Option<std::collections::HashMap<String, f64>>> {
        let sorted_mints = {
            let mut mints = token_mints.to_vec();
            mints.sort();
            mints.join("-")
        };
        let cache_key = format!("jupiterPrice:{}:{}", sorted_mints, vs_token);
        
        let mut conn = self.get_connection().await?;
        let cached: Option<String> = conn.get(&cache_key).await?;
        
        match cached {
            Some(json) => {
                match serde_json::from_str(&json) {
                    Ok(prices) => {
                        debug!("Cache hit for {} tokens", token_mints.len());
                        Ok(Some(prices))
                    }
                    Err(e) => {
                        warn!("Corrupted cache data for key {}: {}", cache_key, e);
                        // Delete corrupted cache entry
                        let _: () = conn.del(&cache_key).await?;
                        Ok(None)
                    }
                }
            }
            None => {
                debug!("Cache miss for {} tokens", token_mints.len());
                Ok(None)
            }
        }
    }

    // =====================================
    // Temporary Data Management
    // =====================================
    
    /// Store temporary account amounts data
    pub async fn store_temp_account_amounts(
        &self,
        wallet: &str,
        data: &serde_json::Value,
    ) -> Result<()> {
        let key = format!("accamounts:{}", wallet);
        let json = serde_json::to_string(data)?;
        let mut conn = self.get_connection().await?;
        let _: () = conn.set(&key, json).await?;
        Ok(())
    }

    /// Get temporary account amounts data
    pub async fn get_temp_account_amounts(
        &self,
        wallet: &str,
    ) -> Result<Option<serde_json::Value>> {
        let key = format!("accamounts:{}", wallet);
        let mut conn = self.get_connection().await?;
        let cached: Option<String> = conn.get(&key).await?;
        
        match cached {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }

    /// Store temporary transaction IDs
    pub async fn store_temp_tx_ids(
        &self,
        wallet: &str,
        tx_ids: &[String],
    ) -> Result<()> {
        let key = format!("temptxids:{}", wallet);
        let json = serde_json::to_string(tx_ids)?;
        let mut conn = self.get_connection().await?;
        let _: () = conn.set(&key, json).await?;
        Ok(())
    }

    /// Store transaction signatures list (TypeScript: temptxids:{wallet})
    pub async fn store_transaction_signatures_list(
        &self,
        wallet: &str,
        signatures: &[String],
    ) -> Result<()> {
        let key = format!("temptxids:{}", wallet);
        let mut conn = self.get_connection().await?;
        // TypeScript uses Redis lists (LPUSH/LRANGE)
        let _: () = conn.del(&key).await?; // Clear existing
        if !signatures.is_empty() {
            let _: () = conn.lpush(&key, signatures).await?;
        }
        Ok(())
    }

    /// Get transaction signatures list length (TypeScript: LLEN)
    pub async fn get_transaction_signatures_count(&self, wallet: &str) -> Result<u64> {
        let key = format!("temptxids:{}", wallet);
        let mut conn = self.get_connection().await?;
        let count: u64 = conn.llen(&key).await?;
        Ok(count)
    }

    /// Get transaction signatures range (TypeScript: LRANGE)
    pub async fn get_transaction_signatures_range(
        &self,
        wallet: &str,
        start: isize,
        end: isize,
    ) -> Result<Vec<String>> {
        let key = format!("temptxids:{}", wallet);
        let mut conn = self.get_connection().await?;
        let signatures: Vec<String> = conn.lrange(&key, start, end).await?;
        Ok(signatures)
    }

    /// Store parsed transaction (TypeScript: parsed:{wallet}:{txid})
    pub async fn store_parsed_transaction(
        &self,
        wallet: &str,
        txid: &str,
        transaction_data: &serde_json::Value,
    ) -> Result<()> {
        let key = format!("parsed:{}:{}", wallet, txid);
        let json = serde_json::to_string(transaction_data)?;
        let mut conn = self.get_connection().await?;
        let _: () = conn.set(&key, json).await?;
        Ok(())
    }

    /// Store account amounts per mint (TypeScript: accamounts:{wallet}:{mint})
    pub async fn store_account_amounts_per_mint(
        &self,
        wallet: &str,
        mint: &str,
        data: &serde_json::Value,
    ) -> Result<()> {
        let key = format!("accamounts:{}:{}", wallet, mint);
        let json = serde_json::to_string(data)?;
        let mut conn = self.get_connection().await?;
        // TypeScript uses JSON.SET for RedisJSON
        let _: () = conn.set(&key, json).await?;
        Ok(())
    }

    /// Get account amounts per mint (TypeScript: JSON.GET key $)
    pub async fn get_account_amounts_per_mint(
        &self,
        wallet: &str,
        mint: &str,
    ) -> Result<Option<serde_json::Value>> {
        let key = format!("accamounts:{}:{}", wallet, mint);
        let mut conn = self.get_connection().await?;
        let cached: Option<String> = conn.get(&key).await?;
        
        match cached {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }

    /// Scan for accamounts keys (TypeScript: SCAN with MATCH "accamounts:*")
    pub async fn scan_account_amounts_keys(&self) -> Result<Vec<String>> {
        let mut conn = self.get_connection().await?;
        
        // Use KEYS command for simplicity
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg("accamounts:*")
            .query_async(&mut conn)
            .await?;
        
        Ok(keys)
    }

    /// Scan for temptxids keys (TypeScript: SCAN with MATCH "temptxids:*")
    pub async fn scan_temp_tx_keys(&self) -> Result<Vec<String>> {
        let mut conn = self.get_connection().await?;
        
        // Use KEYS command for simplicity
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg("temptxids:*")
            .query_async(&mut conn)
            .await?;
        
        Ok(keys)
    }

    /// Clear temporary data for cleanup (TypeScript: clearRedisTempData)
    pub async fn clear_temp_data(&self) -> Result<()> {
        let patterns = ["accamounts:*", "temptxids:*", "parsed:*"];
        let mut conn = self.get_connection().await?;
        
        for pattern in &patterns {
            let keys: Vec<String> = redis::cmd("KEYS")
                .arg(pattern)
                .query_async(&mut conn)
                .await?;
            
            if !keys.is_empty() {
                let _: () = conn.del(keys).await?;
            }
        }
        
        info!("Cleared temporary Redis data");
        Ok(())
    }

    // =====================================
    // Trending Token Management (NEW)
    // =====================================
    
    /// Store trending token data
    pub async fn store_trending_token(
        &self,
        token_address: &str,
        trending_data: &serde_json::Value,
        ttl_seconds: u64,
    ) -> Result<()> {
        let key = format!("trending_token:{}", token_address);
        let json = serde_json::to_string(trending_data)?;
        let mut conn = self.get_connection().await?;
        let _: () = conn.set_ex(&key, json, ttl_seconds).await?;
        info!("Stored trending token data: {}", token_address);
        Ok(())
    }

    /// Get trending token data
    pub async fn get_trending_token(
        &self,
        token_address: &str,
    ) -> Result<Option<serde_json::Value>> {
        let key = format!("trending_token:{}", token_address);
        let mut conn = self.get_connection().await?;
        let cached: Option<String> = conn.get(&key).await?;
        
        match cached {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }

    /// Store trending pair metadata with additional analytics
    pub async fn store_trending_pair_metadata(
        &self,
        pair_address: &str,
        metadata: &serde_json::Value,
        ttl_seconds: u64,
    ) -> Result<()> {
        let key = format!("trending_pair_meta:{}", pair_address);
        let json = serde_json::to_string(metadata)?;
        let mut conn = self.get_connection().await?;
        let _: () = conn.set_ex(&key, json, ttl_seconds).await?;
        debug!("Stored trending pair metadata: {}", pair_address);
        Ok(())
    }

    /// Get trending pair metadata
    pub async fn get_trending_pair_metadata(
        &self,
        pair_address: &str,
    ) -> Result<Option<serde_json::Value>> {
        let key = format!("trending_pair_meta:{}", pair_address);
        let mut conn = self.get_connection().await?;
        let cached: Option<String> = conn.get(&key).await?;
        
        match cached {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }

    /// Store discovered wallets from trending analysis
    pub async fn store_trending_wallets(
        &self,
        pair_address: &str,
        wallets: &[String],
        ttl_seconds: u64,
    ) -> Result<()> {
        let key = format!("trending_wallets:{}", pair_address);
        let json = serde_json::to_string(wallets)?;
        let mut conn = self.get_connection().await?;
        let _: () = conn.set_ex(&key, json, ttl_seconds).await?;
        info!("Stored {} trending wallets for pair: {}", wallets.len(), pair_address);
        Ok(())
    }

    /// Get discovered wallets for a trending pair
    pub async fn get_trending_wallets(
        &self,
        pair_address: &str,
    ) -> Result<Option<Vec<String>>> {
        let key = format!("trending_wallets:{}", pair_address);
        let mut conn = self.get_connection().await?;
        let cached: Option<String> = conn.get(&key).await?;
        
        match cached {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }

    /// Store trending analysis statistics
    pub async fn store_trending_stats(
        &self,
        stats: &serde_json::Value,
        ttl_seconds: u64,
    ) -> Result<()> {
        let key = "trending_analysis_stats";
        let json = serde_json::to_string(stats)?;
        let mut conn = self.get_connection().await?;
        let _: () = conn.set_ex(key, json, ttl_seconds).await?;
        debug!("Stored trending analysis statistics");
        Ok(())
    }

    /// Get trending analysis statistics
    pub async fn get_trending_stats(&self) -> Result<Option<serde_json::Value>> {
        let key = "trending_analysis_stats";
        let mut conn = self.get_connection().await?;
        let cached: Option<String> = conn.get(key).await?;
        
        match cached {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }

    /// Get all current trending pairs
    pub async fn get_all_trending_pairs(&self) -> Result<Vec<String>> {
        let mut conn = self.get_connection().await?;
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg("trending:*")
            .query_async(&mut conn)
            .await?;
        
        let pairs: Vec<String> = keys
            .into_iter()
            .filter_map(|key| key.strip_prefix("trending:").map(|s| s.to_string()))
            .collect();
        
        Ok(pairs)
    }

    /// Clean up old trending data
    pub async fn cleanup_old_trending_data(&self) -> Result<u64> {
        let patterns = [
            "trending_token:*",
            "trending_pair_meta:*", 
            "trending_wallets:*"
        ];
        let mut deleted_count = 0u64;
        let mut conn = self.get_connection().await?;
        
        for pattern in &patterns {
            let keys: Vec<String> = redis::cmd("KEYS")
                .arg(pattern)
                .query_async(&mut conn)
                .await?;
            
            if !keys.is_empty() {
                let count: u64 = conn.del(&keys).await?;
                deleted_count += count;
            }
        }
        
        if deleted_count > 0 {
            info!("Cleaned up {} old trending data entries", deleted_count);
        }
        Ok(deleted_count)
    }

    // =====================================
    // General Redis Operations
    // =====================================
    
    /// Flush the Redis database (use with caution)
    pub async fn flush_db(&self) -> Result<()> {
        let mut conn = self.get_connection().await?;
        let _: () = redis::cmd("FLUSHDB").query_async(&mut conn).await?;
        warn!("Flushed Redis database");
        Ok(())
    }

    /// Test Redis connection
    pub async fn ping(&self) -> Result<String> {
        let mut conn = self.get_connection().await?;
        let pong: String = redis::cmd("PING").query_async(&mut conn).await?;
        Ok(pong)
    }

    /// Get cached data (generic Redis GET)
    pub async fn get_cached_data(&self, key: &str) -> Result<Option<String>> {
        let mut conn = self.get_connection().await?;
        let result: Option<String> = conn.get(key).await?;
        Ok(result)
    }

    /// Set data with expiry (Redis SET with EX)
    pub async fn set_with_expiry(&self, key: &str, value: &str, ttl_seconds: u64) -> Result<()> {
        let mut conn = self.get_connection().await?;
        let _: () = conn.set_ex(key, value, ttl_seconds).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_redis_connection() {
        // This test requires a running Redis instance
        // Skip if REDIS_URL is not set
        if std::env::var("REDIS_URL").is_err() {
            return;
        }
        
        let redis_url = std::env::var("REDIS_URL").unwrap();
        let client = RedisClient::new(&redis_url).await.unwrap();
        let result = client.ping().await.unwrap();
        assert_eq!(result, "PONG");
    }
}