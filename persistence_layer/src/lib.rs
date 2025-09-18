use redis::{AsyncCommands, Client};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

// Re-export postgres client
pub mod postgres_client;
pub use postgres_client::PostgresClient;

/// Wallet-chain pair for multichain batch processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletChainPair {
    /// Wallet address
    pub wallet_address: String,
    /// Blockchain network (solana, ethereum, base, bsc)
    pub chain: String,
}

/// Discovered wallet-token pair for targeted P&L analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredWalletToken {
    /// Wallet address of the trader
    pub wallet_address: String,
    /// Blockchain network (solana, ethereum, base, bsc)
    pub chain: String,
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
    pub pnl_report: pnl_core::PortfolioPnLResult,
    pub analyzed_at: chrono::DateTime<chrono::Utc>,
}

/// Stored portfolio P&L result with metadata (for PostgreSQL storage)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredPortfolioPnLResult {
    pub wallet_address: String,
    pub chain: String,
    pub portfolio_result: pnl_core::PortfolioPnLResult,
    pub analyzed_at: chrono::DateTime<chrono::Utc>,
    pub is_favorited: bool,
    pub is_archived: bool,
    pub unique_tokens_count: Option<u32>,
    pub active_days_count: Option<u32>,
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

/// Status of a batch job
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobStatus::Pending => write!(f, "Pending"),
            JobStatus::Running => write!(f, "Running"),
            JobStatus::Completed => write!(f, "Completed"),
            JobStatus::Failed => write!(f, "Failed"),
            JobStatus::Cancelled => write!(f, "Cancelled"),
        }
    }
}

/// Batch P&L analysis job for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchJob {
    pub id: Uuid,
    pub wallet_addresses: Vec<String>,
    pub chain: String,
    pub status: JobStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub filters: serde_json::Value, // Store as JSON for flexibility
    pub individual_jobs: Vec<Uuid>,
    // Results are stored separately and linked by job_id
}

/// Token analysis job for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenAnalysisJob {
    pub id: Uuid,
    pub token_addresses: Vec<String>,
    pub chain: String,
    pub status: JobStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub filters: serde_json::Value, // Store max_transactions and other params as JSON
    pub discovered_wallets: Vec<String>, // Wallets discovered from tokens
    pub analyzed_wallets: Vec<String>, // Wallets successfully analyzed
    pub failed_wallets: Vec<String>, // Wallets that failed analysis
}

impl TokenAnalysisJob {
    pub fn new(token_addresses: Vec<String>, chain: String, max_transactions: Option<u32>) -> Self {
        let filters = serde_json::json!({
            "max_transactions": max_transactions
        });

        Self {
            id: Uuid::new_v4(),
            token_addresses,
            chain,
            status: JobStatus::Pending,
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
            filters,
            discovered_wallets: Vec::new(),
            analyzed_wallets: Vec::new(),
            failed_wallets: Vec::new(),
        }
    }

    pub fn get_max_transactions(&self) -> Option<u32> {
        self.filters
            .get("max_transactions")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
    }
}

/// Token analysis job statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenAnalysisJobStats {
    pub total_jobs: u64,
    pub pending_jobs: u64,
    pub running_jobs: u64,
    pub completed_jobs: u64,
    pub failed_jobs: u64,
    pub cancelled_jobs: u64,
}

/// Batch job statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchJobStats {
    pub total_jobs: u64,
    pub pending_jobs: u64,
    pub running_jobs: u64,
    pub completed_jobs: u64,
    pub failed_jobs: u64,
    pub cancelled_jobs: u64,
}

#[derive(Error, Debug)]
pub enum PersistenceError {
    #[error("Redis connection error: {0}")]
    Connection(#[from] redis::RedisError),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Database pool creation error: {0}")]
    PoolCreation(String),
    #[error("Lock acquisition failed")]
    LockFailed,
    #[error("Lock not found")]
    LockNotFound,
}

pub type Result<T> = std::result::Result<T, PersistenceError>;

/// Unified persistence client that combines Redis and PostgreSQL operations
#[derive(Debug, Clone)]
pub struct PersistenceClient {
    redis_client: RedisClient,
    postgres_client: PostgresClient,
}

impl PersistenceClient {
    /// Create a new persistence client with both Redis and PostgreSQL connections
    pub async fn new(redis_url: &str, postgres_url: &str) -> Result<Self> {
        let redis_client = RedisClient::new(redis_url).await?;
        let postgres_client = PostgresClient::new(postgres_url)
            .await
            .map_err(|e| PersistenceError::PoolCreation(e.to_string()))?;

        Ok(Self {
            redis_client,
            postgres_client,
        })
    }

    // Delegate all Redis operations to RedisClient
    pub async fn acquire_lock(&self, key: &str, ttl_seconds: u64) -> Result<LockHandle> {
        self.redis_client.acquire_lock(key, ttl_seconds).await
    }

    pub async fn release_lock(&self, handle: &LockHandle) -> Result<bool> {
        self.redis_client.release_lock(handle).await.map(|_| true)
    }

    pub async fn pop_discovered_wallet_token_pair(
        &self,
        count: usize,
    ) -> Result<Option<DiscoveredWalletToken>> {
        self.redis_client
            .pop_discovered_wallet_token_pair(count as u64)
            .await
    }

    pub async fn pop_discovered_wallet_token_pairs(
        &self,
        count: usize,
    ) -> Result<Vec<DiscoveredWalletToken>> {
        self.redis_client
            .pop_discovered_wallet_token_pairs(count)
            .await
    }

    pub async fn push_discovered_wallet_token_pairs_deduplicated(
        &self,
        pairs: &[DiscoveredWalletToken],
    ) -> Result<usize> {
        self.redis_client
            .push_discovered_wallet_token_pairs_deduplicated(pairs)
            .await
    }

    pub async fn push_failed_wallet_token_pairs_for_retry(
        &self,
        pairs: &[DiscoveredWalletToken],
    ) -> Result<()> {
        self.redis_client
            .push_failed_wallet_token_pairs_for_retry(pairs)
            .await
    }

    pub async fn mark_wallet_as_processed(&self, wallet_address: &str) -> Result<()> {
        self.redis_client
            .mark_wallet_as_processed(wallet_address)
            .await
    }

    pub async fn mark_wallet_as_processed_for_chain(
        &self,
        wallet_address: &str,
        chain: &str,
    ) -> Result<()> {
        self.redis_client
            .mark_wallet_as_processed_for_chain(wallet_address, chain)
            .await
    }

    pub async fn mark_wallet_as_failed(&self, wallet_address: &str) -> Result<()> {
        self.redis_client
            .mark_wallet_as_failed(wallet_address)
            .await
    }

    pub async fn mark_wallet_as_failed_for_chain(
        &self,
        wallet_address: &str,
        chain: &str,
    ) -> Result<()> {
        self.redis_client
            .mark_wallet_as_failed_for_chain(wallet_address, chain)
            .await
    }

    pub async fn get_wallet_token_pairs_queue_size(&self) -> Result<usize> {
        self.redis_client
            .get_wallet_token_pairs_queue_size()
            .await
            .map(|size| size as usize)
    }

    pub async fn store_batch_job(&self, job: &BatchJob) -> Result<()> {
        self.redis_client.store_batch_job(job).await
    }

    pub async fn get_batch_job(&self, job_id: &str) -> Result<Option<BatchJob>> {
        self.redis_client.get_batch_job(job_id).await
    }

    pub async fn update_batch_job(&self, job: &BatchJob) -> Result<()> {
        self.redis_client.update_batch_job(job).await
    }

    pub async fn get_all_batch_jobs(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<BatchJob>, usize)> {
        self.redis_client.get_all_batch_jobs(limit, offset).await
    }

    pub async fn get_batch_job_stats(&self) -> Result<BatchJobStats> {
        self.redis_client.get_batch_job_stats().await
    }

    // =====================================
    // Token Analysis Job Management
    // =====================================

    pub async fn store_token_analysis_job(&self, job: &TokenAnalysisJob) -> Result<()> {
        self.postgres_client.store_token_analysis_job(job).await
    }

    pub async fn get_token_analysis_job(&self, job_id: &str) -> Result<Option<TokenAnalysisJob>> {
        self.postgres_client.get_token_analysis_job(job_id).await
    }

    pub async fn update_token_analysis_job(&self, job: &TokenAnalysisJob) -> Result<()> {
        self.postgres_client.update_token_analysis_job(job).await
    }

    pub async fn get_all_token_analysis_jobs(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<TokenAnalysisJob>, usize)> {
        self.postgres_client
            .get_all_token_analysis_jobs(limit, offset)
            .await
    }

    pub async fn get_token_analysis_job_stats(&self) -> Result<TokenAnalysisJobStats> {
        self.postgres_client.get_token_analysis_job_stats().await
    }

    pub async fn clear_temp_data(&self) -> Result<()> {
        self.redis_client.clear_temp_data().await
    }

    // Delegate PostgreSQL operations to PostgresClient
    pub async fn store_pnl_result(
        &self,
        wallet_address: &str,
        chain: &str,
        portfolio_result: &pnl_core::PortfolioPnLResult,
    ) -> Result<()> {
        self.postgres_client
            .store_pnl_result(wallet_address, chain, portfolio_result)
            .await
            .map_err(|e| PersistenceError::PoolCreation(e.to_string()))
    }

    pub async fn store_pnl_result_with_source(
        &self,
        wallet_address: &str,
        chain: &str,
        portfolio_result: &pnl_core::PortfolioPnLResult,
        analysis_source: &str,
    ) -> Result<()> {
        self.postgres_client
            .store_pnl_result_with_source(wallet_address, chain, portfolio_result, analysis_source)
            .await
            .map_err(|e| PersistenceError::PoolCreation(e.to_string()))
    }

    pub async fn get_portfolio_pnl_result(
        &self,
        wallet_address: &str,
        chain: &str,
    ) -> Result<Option<StoredPortfolioPnLResult>> {
        self.postgres_client
            .get_portfolio_pnl_result(wallet_address, chain)
            .await
            .map_err(|e| PersistenceError::PoolCreation(e.to_string()))
    }

    pub async fn get_all_pnl_results(
        &self,
        offset: usize,
        limit: usize,
        chain_filter: Option<&str>,
    ) -> Result<(Vec<StoredPortfolioPnLResult>, usize)> {
        self.postgres_client
            .get_all_pnl_results(offset, limit, chain_filter)
            .await
            .map_err(|e| PersistenceError::PoolCreation(e.to_string()))
    }

    pub async fn get_all_pnl_results_with_filters(
        &self,
        offset: usize,
        limit: usize,
        chain_filter: Option<&str>,
        min_unique_tokens: Option<u32>,
        min_active_days: Option<u32>,
        analysis_source_filter: Option<&str>,
    ) -> Result<(Vec<StoredPortfolioPnLResult>, usize)> {
        self.postgres_client
            .get_all_pnl_results_with_filters(
                offset,
                limit,
                chain_filter,
                min_unique_tokens,
                min_active_days,
                analysis_source_filter,
            )
            .await
            .map_err(|e| PersistenceError::PoolCreation(e.to_string()))
    }

    pub async fn get_pnl_results_by_analysis_source(
        &self,
        analysis_source: &str,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<StoredPortfolioPnLResult>, usize)> {
        self.postgres_client
            .get_pnl_results_by_analysis_source(analysis_source, offset, limit)
            .await
            .map_err(|e| PersistenceError::PoolCreation(e.to_string()))
    }

    pub async fn apply_advanced_filtering_migration(&self) -> Result<()> {
        self.postgres_client
            .apply_advanced_filtering_migration()
            .await
            .map_err(|e| PersistenceError::PoolCreation(e.to_string()))
    }

    pub async fn backfill_advanced_filtering_metrics(&self) -> Result<()> {
        self.postgres_client
            .backfill_advanced_filtering_metrics()
            .await
            .map_err(|e| PersistenceError::PoolCreation(e.to_string()))
    }

    pub async fn get_stats(&self) -> Result<(usize, usize)> {
        self.postgres_client
            .get_stats()
            .await
            .map_err(|e| PersistenceError::PoolCreation(e.to_string()))
    }

    pub async fn update_wallet_favorite_status(
        &self,
        wallet_address: &str,
        chain: &str,
        is_favorited: bool,
    ) -> Result<()> {
        self.postgres_client
            .update_wallet_favorite_status(wallet_address, chain, is_favorited)
            .await
            .map_err(|e| PersistenceError::PoolCreation(e.to_string()))
    }

    pub async fn update_wallet_archive_status(
        &self,
        wallet_address: &str,
        chain: &str,
        is_archived: bool,
    ) -> Result<()> {
        self.postgres_client
            .update_wallet_archive_status(wallet_address, chain, is_archived)
            .await
            .map_err(|e| PersistenceError::PoolCreation(e.to_string()))
    }

    // Work-stealing delegation methods
    pub async fn claim_wallet_batch(
        &self,
        instance_id: &str,
        batch_size: usize,
    ) -> Result<(Vec<DiscoveredWalletToken>, String)> {
        self.redis_client
            .claim_wallet_batch(instance_id, batch_size)
            .await
    }

    pub async fn release_batch_claim(&self, batch_id: &str) -> Result<()> {
        self.redis_client.release_batch_claim(batch_id).await
    }

    pub async fn return_failed_batch(
        &self,
        batch_id: &str,
        failed_items: &[DiscoveredWalletToken],
    ) -> Result<()> {
        self.redis_client
            .return_failed_batch(batch_id, failed_items)
            .await
    }

    pub async fn cleanup_stale_processing_locks(&self, max_age_seconds: u64) -> Result<usize> {
        self.redis_client
            .cleanup_stale_processing_locks(max_age_seconds)
            .await
    }

    pub async fn get_processing_stats(&self) -> Result<(usize, usize)> {
        self.redis_client.get_processing_stats().await
    }
}

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
        self.client
            .get_async_connection()
            .await
            .map_err(PersistenceError::from)
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
        let result: Option<Vec<String>> = conn.brpop(queue_key, timeout_seconds as f64).await?;

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
    pub async fn push_discovered_wallet_token_pairs(
        &self,
        wallet_tokens: &[DiscoveredWalletToken],
    ) -> Result<()> {
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
        info!(
            "Pushed {} wallet-token pairs to discovery queue",
            wallet_tokens.len()
        );
        Ok(())
    }

    /// Push discovered wallet-token pairs with deduplication to prevent reprocessing
    pub async fn push_discovered_wallet_token_pairs_deduplicated(
        &self,
        wallet_tokens: &[DiscoveredWalletToken],
    ) -> Result<usize> {
        if wallet_tokens.is_empty() {
            return Ok(0);
        }

        // Group wallet tokens by chain
        let mut chain_groups: std::collections::HashMap<String, Vec<&DiscoveredWalletToken>> =
            std::collections::HashMap::new();
        for wallet_token in wallet_tokens {
            chain_groups
                .entry(wallet_token.chain.clone())
                .or_default()
                .push(wallet_token);
        }

        let mut total_pushed = 0;
        let mut conn = self.get_connection().await?;

        // Process each chain group separately
        for (chain, chain_wallet_tokens) in chain_groups {
            let queue_key = format!("discovered_wallet_token_pairs_queue:{}", chain);
            let processed_wallets_key = format!("processed_wallets:{}", chain);
            let pending_wallets_key = format!("pending_wallets:{}", chain);

            let mut new_wallet_tokens = Vec::new();
            let mut duplicate_count = 0;

            // Check each wallet for duplicates within this chain
            for wallet_token in chain_wallet_tokens {
                let wallet_address = &wallet_token.wallet_address;

                // Check if wallet is already processed or pending for this chain
                let is_processed: bool = conn
                    .sismember(&processed_wallets_key, wallet_address)
                    .await?;
                let is_pending: bool = conn.sismember(&pending_wallets_key, wallet_address).await?;

                if is_processed || is_pending {
                    duplicate_count += 1;
                    debug!("⭕ Skipping duplicate wallet: {} for chain {} (already processed or pending)", wallet_address, chain);
                    continue;
                }

                // Add to pending set and queue for processing
                let _: () = conn.sadd(&pending_wallets_key, wallet_address).await?;
                new_wallet_tokens.push((*wallet_token).clone());
            }

            if !new_wallet_tokens.is_empty() {
                // Serialize and push new wallets to chain-specific queue
                let json_pairs: Result<Vec<String>> = new_wallet_tokens
                    .iter()
                    .map(|wt| serde_json::to_string(wt).map_err(PersistenceError::from))
                    .collect();

                let json_pairs = json_pairs?;
                let _: () = conn.lpush(&queue_key, json_pairs).await?;

                info!("✅ Pushed {} new wallets to discovery queue for chain {}, skipped {} duplicates",
                      new_wallet_tokens.len(), chain, duplicate_count);

                total_pushed += new_wallet_tokens.len();
            } else if duplicate_count > 0 {
                info!(
                    "⭕ All {} wallets for chain {} were duplicates, skipped",
                    duplicate_count, chain
                );
            }
        }

        Ok(total_pushed)
    }

    /// Mark a wallet as processed for a specific chain (move from pending to processed)
    pub async fn mark_wallet_as_processed_for_chain(
        &self,
        wallet_address: &str,
        chain: &str,
    ) -> Result<()> {
        let processed_wallets_key = format!("processed_wallets:{}", chain);
        let pending_wallets_key = format!("pending_wallets:{}", chain);
        let mut conn = self.get_connection().await?;

        // Move from pending to processed for this chain
        let _: () = conn.srem(&pending_wallets_key, wallet_address).await?;
        let _: () = conn.sadd(&processed_wallets_key, wallet_address).await?;

        debug!(
            "✅ Marked wallet {} as processed for chain {}",
            wallet_address, chain
        );
        Ok(())
    }

    /// Mark a wallet as processed (backward compatibility - uses default chain)
    pub async fn mark_wallet_as_processed(&self, wallet_address: &str) -> Result<()> {
        // For backward compatibility, use 'solana' as default chain
        self.mark_wallet_as_processed_for_chain(wallet_address, "solana")
            .await
    }

    /// Mark a wallet as failed processing for a specific chain (remove from pending, don't add to processed)
    pub async fn mark_wallet_as_failed_for_chain(
        &self,
        wallet_address: &str,
        chain: &str,
    ) -> Result<()> {
        let pending_wallets_key = format!("pending_wallets:{}", chain);
        let mut conn = self.get_connection().await?;

        // Remove from pending for this chain (can be retried later)
        let _: () = conn.srem(&pending_wallets_key, wallet_address).await?;

        debug!(
            "❌ Marked wallet {} as failed for chain {} (removed from pending)",
            wallet_address, chain
        );
        Ok(())
    }

    /// Mark a wallet as failed processing (backward compatibility - uses default chain)
    pub async fn mark_wallet_as_failed(&self, wallet_address: &str) -> Result<()> {
        // For backward compatibility, use 'solana' as default chain
        self.mark_wallet_as_failed_for_chain(wallet_address, "solana")
            .await
    }

    /// Get deduplication statistics
    pub async fn get_deduplication_stats(&self) -> Result<(u64, u64, u64)> {
        let processed_wallets_key = "processed_wallets";
        let pending_wallets_key = "pending_wallets";
        let queue_key = "discovered_wallet_token_pairs_queue";
        let mut conn = self.get_connection().await?;

        let processed_count: u64 = conn.scard(processed_wallets_key).await?;
        let pending_count: u64 = conn.scard(pending_wallets_key).await?;
        let queue_count: u64 = conn.llen(queue_key).await?;

        Ok((processed_count, pending_count, queue_count))
    }

    /// Pop a wallet-token pair from the queue (blocking with timeout)
    pub async fn pop_discovered_wallet_token_pair(
        &self,
        timeout_seconds: u64,
    ) -> Result<Option<DiscoveredWalletToken>> {
        let queue_key = "discovered_wallet_token_pairs_queue";
        let mut conn = self.get_connection().await?;
        let result: Option<Vec<String>> = conn.brpop(queue_key, timeout_seconds as f64).await?;

        match result {
            Some(mut items) if items.len() >= 2 => {
                // brpop returns [key, value]
                let json_data = items.pop().unwrap();
                let wallet_token: DiscoveredWalletToken = serde_json::from_str(&json_data)?;
                debug!(
                    "Popped wallet-token pair from queue: {} for token {}",
                    wallet_token.wallet_address, wallet_token.token_symbol
                );
                Ok(Some(wallet_token))
            }
            _ => Ok(None),
        }
    }

    /// Pop multiple wallet-token pairs from the discovery queue for parallel processing
    pub async fn pop_discovered_wallet_token_pairs(
        &self,
        batch_size: usize,
    ) -> Result<Vec<DiscoveredWalletToken>> {
        let queue_key = "discovered_wallet_token_pairs_queue";
        let mut conn = self.get_connection().await?;
        let mut wallet_tokens = Vec::new();

        // Pop up to batch_size items from the queue
        for _ in 0..batch_size {
            let result: Option<Vec<String>> = conn
                .brpop(queue_key, 0.1) // Very short timeout for batching
                .await?;

            match result {
                Some(mut items) if items.len() >= 2 => {
                    // brpop returns [key, value]
                    let json_data = items.pop().unwrap();
                    match serde_json::from_str::<DiscoveredWalletToken>(&json_data) {
                        Ok(wallet_token) => {
                            debug!(
                                "Popped wallet-token pair from queue: {} for token {}",
                                wallet_token.wallet_address, wallet_token.token_symbol
                            );
                            wallet_tokens.push(wallet_token);
                        }
                        Err(e) => {
                            warn!("Failed to deserialize wallet-token pair: {}", e);
                        }
                    }
                }
                _ => {
                    // No more items available, break early
                    break;
                }
            }
        }

        if !wallet_tokens.is_empty() {
            debug!(
                "Popped {} wallet-token pairs from queue for parallel processing",
                wallet_tokens.len()
            );
        }

        Ok(wallet_tokens)
    }

    /// Get the current size of the wallet-token pairs queue
    pub async fn get_wallet_token_pairs_queue_size(&self) -> Result<u64> {
        let queue_key = "discovered_wallet_token_pairs_queue";
        let mut conn = self.get_connection().await?;
        let size: u64 = conn.llen(queue_key).await?;
        Ok(size)
    }

    /// Push failed wallet-token pairs back to the front of the queue for retry
    pub async fn push_failed_wallet_token_pairs_for_retry(
        &self,
        wallet_tokens: &[DiscoveredWalletToken],
    ) -> Result<()> {
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

        // Push to the FRONT of the queue (lpush) so they get retried sooner
        let _: () = conn.lpush(queue_key, json_pairs).await?;

        debug!(
            "Pushed {} failed wallet-token pairs back to front of queue for retry",
            wallet_tokens.len()
        );
        Ok(())
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
        pnl_report: &pnl_core::PortfolioPnLResult,
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

        info!(
            "Stored P&L result for wallet {} token {}",
            wallet_address, token_symbol
        );
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
        let paginated_keys: Vec<String> =
            result_keys.into_iter().skip(offset).take(limit).collect();

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
    async fn update_pnl_summary_stats(
        &self,
        pnl_report: &pnl_core::PortfolioPnLResult,
    ) -> Result<()> {
        let mut conn = self.get_connection().await?;
        let stats_key = "pnl_summary_stats";

        // Get current stats or create new ones
        let current_stats_json: Option<String> = conn.get(stats_key).await?;
        let mut stats = match current_stats_json {
            Some(json) => serde_json::from_str::<PnLSummaryStats>(&json).unwrap_or_default(),
            None => PnLSummaryStats::default(),
        };

        // Update stats
        stats.total_wallets_analyzed += 1;
        stats.total_pnl_usd += pnl_report.total_pnl_usd;
        stats.total_realized_pnl_usd += pnl_report.total_realized_pnl_usd;
        stats.total_trades += pnl_report.total_trades as u64;

        if pnl_report.total_pnl_usd > Decimal::ZERO {
            stats.profitable_wallets += 1;
        }

        // Calculate averages
        if stats.total_wallets_analyzed > 0 {
            stats.average_pnl_usd =
                stats.total_pnl_usd / Decimal::from(stats.total_wallets_analyzed);
            stats.profitability_rate =
                (stats.profitable_wallets as f64 / stats.total_wallets_analyzed as f64) * 100.0;
        }

        stats.last_updated = chrono::Utc::now();

        // Store updated stats
        let stats_json = serde_json::to_string(&stats)?;
        let _: () = conn.set(stats_key, &stats_json).await?;

        Ok(())
    }

    /// Get aggregated P&L summary statistics
    pub async fn get_pnl_summary_stats(&self) -> Result<PnLSummaryStats> {
        let mut conn = self.get_connection().await?;
        let stats_key = "pnl_summary_stats";

        let stats_json: Option<String> = conn.get(stats_key).await?;

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
        let value = format!(
            "{}:{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );

        let mut conn = self.get_connection().await?;

        // Try to set the lock with NX (only if not exists) and EX (expiration)
        let acquired: bool = conn.set_nx(&key, &value).await?;

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
            warn!(
                "Lock was already expired or held by another process: {}",
                handle.key
            );
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
        let _: () = conn.set_ex(&cache_key, prices_json, ttl_seconds).await?;

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
    pub async fn store_temp_tx_ids(&self, wallet: &str, tx_ids: &[String]) -> Result<()> {
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
        info!(
            "Stored {} trending wallets for pair: {}",
            wallets.len(),
            pair_address
        );
        Ok(())
    }

    /// Get discovered wallets for a trending pair
    pub async fn get_trending_wallets(&self, pair_address: &str) -> Result<Option<Vec<String>>> {
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
            "trending_wallets:*",
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
    // =====================================
    // Batch Job Storage and Management
    // =====================================

    /// Store a batch job in Redis for persistence
    pub async fn store_batch_job(&self, job: &BatchJob) -> Result<()> {
        let job_key = format!("batch_job:{}", job.id);
        let job_json = serde_json::to_string(job)?;

        let mut conn = self.get_connection().await?;

        // Store the job data
        let _: () = conn.set(&job_key, &job_json).await?;

        // Add to batch jobs index for easy retrieval
        let _: () = conn.sadd("batch_jobs_index", job.id.to_string()).await?;

        // Add to time-ordered sorted set for chronological listing
        let timestamp = job.created_at.timestamp() as f64;
        let _: () = conn
            .zadd("batch_jobs_timeline", job.id.to_string(), timestamp)
            .await?;

        debug!("Stored batch job {} in Redis", job.id);
        Ok(())
    }

    /// Update an existing batch job in Redis
    pub async fn update_batch_job(&self, job: &BatchJob) -> Result<()> {
        let job_key = format!("batch_job:{}", job.id);
        let job_json = serde_json::to_string(job)?;

        let mut conn = self.get_connection().await?;
        let _: () = conn.set(&job_key, &job_json).await?;

        debug!("Updated batch job {} in Redis", job.id);
        Ok(())
    }

    /// Get a specific batch job by ID
    pub async fn get_batch_job(&self, job_id: &str) -> Result<Option<BatchJob>> {
        let job_key = format!("batch_job:{}", job_id);
        let mut conn = self.get_connection().await?;

        let job_json: Option<String> = conn.get(&job_key).await?;

        match job_json {
            Some(json) => {
                match serde_json::from_str::<BatchJob>(&json) {
                    Ok(job) => Ok(Some(job)),
                    Err(e) => {
                        warn!("Corrupted batch job data for {}: {}", job_id, e);
                        // Delete corrupted entry
                        let _: () = conn.del(&job_key).await?;
                        let _: () = conn.srem("batch_jobs_index", job_id).await?;
                        let _: () = conn.zrem("batch_jobs_timeline", job_id).await?;
                        Ok(None)
                    }
                }
            }
            None => Ok(None),
        }
    }

    /// Get all batch jobs with pagination, sorted by creation time (newest first)
    pub async fn get_all_batch_jobs(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<BatchJob>, usize)> {
        let mut conn = self.get_connection().await?;

        // Get total count
        let total_count: usize = conn.zcard("batch_jobs_timeline").await?;

        if total_count == 0 {
            return Ok((Vec::new(), 0));
        }

        // Get job IDs from timeline, newest first (reverse order)
        let job_ids: Vec<String> = conn
            .zrevrange(
                "batch_jobs_timeline",
                offset as isize,
                (offset + limit - 1) as isize,
            )
            .await?;

        // Fetch job details for each ID
        let mut jobs = Vec::new();
        for job_id in job_ids {
            if let Some(job) = self.get_batch_job(&job_id).await? {
                jobs.push(job);
            }
        }

        Ok((jobs, total_count))
    }

    /// Get batch jobs filtered by status
    pub async fn get_batch_jobs_by_status(
        &self,
        status: &str,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<BatchJob>, usize)> {
        let mut conn = self.get_connection().await?;

        // Get all job IDs from timeline
        let all_job_ids: Vec<String> = conn.zrevrange("batch_jobs_timeline", 0, -1).await?;

        // Filter by status and paginate
        let mut filtered_jobs = Vec::new();
        let mut found_count = 0;
        let mut total_matching = 0;

        for job_id in all_job_ids {
            if let Some(job) = self.get_batch_job(&job_id).await? {
                if format!("{:?}", job.status).to_lowercase() == status.to_lowercase() {
                    total_matching += 1;

                    if found_count >= offset && filtered_jobs.len() < limit {
                        filtered_jobs.push(job);
                    }
                    found_count += 1;
                }
            }
        }

        Ok((filtered_jobs, total_matching))
    }

    /// Delete a batch job from Redis
    pub async fn delete_batch_job(&self, job_id: &str) -> Result<bool> {
        let job_key = format!("batch_job:{}", job_id);
        let mut conn = self.get_connection().await?;

        // Delete from all locations
        let deleted: usize = conn.del(&job_key).await?;
        let _: () = conn.srem("batch_jobs_index", job_id).await?;
        let _: () = conn.zrem("batch_jobs_timeline", job_id).await?;

        info!("Deleted batch job {} from Redis", job_id);
        Ok(deleted > 0)
    }

    /// Get batch job statistics
    pub async fn get_batch_job_stats(&self) -> Result<BatchJobStats> {
        let mut conn = self.get_connection().await?;

        let total_jobs: usize = conn.zcard("batch_jobs_timeline").await?;

        // Count jobs by status
        let all_job_ids: Vec<String> = conn.smembers("batch_jobs_index").await?;
        let mut pending_count = 0;
        let mut running_count = 0;
        let mut completed_count = 0;
        let mut failed_count = 0;
        let mut cancelled_count = 0;

        for job_id in all_job_ids {
            if let Some(job) = self.get_batch_job(&job_id).await? {
                match job.status {
                    JobStatus::Pending => pending_count += 1,
                    JobStatus::Running => running_count += 1,
                    JobStatus::Completed => completed_count += 1,
                    JobStatus::Failed => failed_count += 1,
                    JobStatus::Cancelled => cancelled_count += 1,
                }
            }
        }

        Ok(BatchJobStats {
            total_jobs: total_jobs as u64,
            pending_jobs: pending_count,
            running_jobs: running_count,
            completed_jobs: completed_count,
            failed_jobs: failed_count,
            cancelled_jobs: cancelled_count,
        })
    }

    // =====================================
    // Batch Job Result Storage
    // =====================================

    /// Store results for a batch job
    pub async fn store_batch_job_results(
        &self,
        job_id: &str,
        results: &std::collections::HashMap<
            String,
            std::result::Result<pnl_core::PortfolioPnLResult, anyhow::Error>,
        >,
    ) -> Result<()> {
        let results_key = format!("batch_job_results:{}", job_id);
        let mut conn = self.get_connection().await?;

        // Convert results to a serializable format
        let mut serializable_results = std::collections::HashMap::new();
        for (wallet, result) in results {
            match result {
                Ok(report) => {
                    let report_json = serde_json::to_string(report)?;
                    serializable_results.insert(
                        wallet.clone(),
                        serde_json::json!({
                            "success": true,
                            "data": report_json
                        }),
                    );
                }
                Err(error) => {
                    serializable_results.insert(
                        wallet.clone(),
                        serde_json::json!({
                            "success": false,
                            "error": error.to_string()
                        }),
                    );
                }
            }
        }

        let results_json = serde_json::to_string(&serializable_results)?;
        let _: () = conn.set(&results_key, &results_json).await?;

        debug!(
            "Stored batch job results for {} ({} wallets)",
            job_id,
            results.len()
        );
        Ok(())
    }

    /// Get results for a batch job
    pub async fn get_batch_job_results(
        &self,
        job_id: &str,
    ) -> Result<
        std::collections::HashMap<
            String,
            std::result::Result<pnl_core::PortfolioPnLResult, anyhow::Error>,
        >,
    > {
        let results_key = format!("batch_job_results:{}", job_id);
        let mut conn = self.get_connection().await?;

        let results_json: Option<String> = conn.get(&results_key).await?;

        match results_json {
            Some(json) => {
                let serializable_results: std::collections::HashMap<String, serde_json::Value> =
                    serde_json::from_str(&json)?;
                let mut results = std::collections::HashMap::new();

                for (wallet, result_data) in serializable_results {
                    if let Some(success) = result_data.get("success").and_then(|v| v.as_bool()) {
                        if success {
                            if let Some(data_str) = result_data.get("data").and_then(|v| v.as_str())
                            {
                                match serde_json::from_str::<pnl_core::PortfolioPnLResult>(data_str)
                                {
                                    Ok(report) => {
                                        results.insert(wallet, Ok(report));
                                    }
                                    Err(e) => {
                                        results.insert(
                                            wallet,
                                            Err(anyhow::anyhow!(
                                                "Failed to deserialize report: {}",
                                                e
                                            )),
                                        );
                                    }
                                }
                            } else {
                                results.insert(wallet, Err(anyhow::anyhow!("Missing report data")));
                            }
                        } else {
                            let error_msg = result_data
                                .get("error")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Unknown error");
                            results.insert(wallet, Err(anyhow::anyhow!("{}", error_msg)));
                        }
                    } else {
                        results.insert(wallet, Err(anyhow::anyhow!("Invalid result format")));
                    }
                }

                Ok(results)
            }
            None => Ok(std::collections::HashMap::new()),
        }
    }

    /// Delete batch job results
    pub async fn delete_batch_job_results(&self, job_id: &str) -> Result<bool> {
        let results_key = format!("batch_job_results:{}", job_id);
        let mut conn = self.get_connection().await?;

        let deleted: i32 = conn.del(&results_key).await?;
        Ok(deleted > 0)
    }

    // =====================================
    // Token Analysis Job Management (Redis)
    // =====================================

    /// Store a token analysis job in Redis for persistence
    pub async fn store_token_analysis_job(&self, job: &TokenAnalysisJob) -> Result<()> {
        let mut conn = self.get_connection().await?;
        let job_key = format!("token_analysis_job:{}", job.id);
        let job_json = serde_json::to_string(job)?;

        // Store the job data
        let _: () = conn.set(&job_key, &job_json).await?;

        // Add to token analysis jobs index for easy retrieval
        let _: () = conn
            .sadd("token_analysis_jobs_index", job.id.to_string())
            .await?;

        // Add to timeline for chronological ordering
        let timestamp = job.created_at.timestamp() as f64;
        let _: () = conn
            .zadd(
                "token_analysis_jobs_timeline",
                job.id.to_string(),
                timestamp,
            )
            .await?;

        debug!("Stored token analysis job {} in Redis", job.id);
        Ok(())
    }

    /// Update an existing token analysis job in Redis
    pub async fn update_token_analysis_job(&self, job: &TokenAnalysisJob) -> Result<()> {
        let mut conn = self.get_connection().await?;
        let job_key = format!("token_analysis_job:{}", job.id);
        let job_json = serde_json::to_string(job)?;

        let _: () = conn.set(&job_key, &job_json).await?;
        debug!("Updated token analysis job {} in Redis", job.id);
        Ok(())
    }

    /// Get a specific token analysis job by ID
    pub async fn get_token_analysis_job(&self, job_id: &str) -> Result<Option<TokenAnalysisJob>> {
        let mut conn = self.get_connection().await?;
        let job_key = format!("token_analysis_job:{}", job_id);

        let job_json: Option<String> = conn.get(&job_key).await?;
        match job_json {
            Some(json) => {
                match serde_json::from_str::<TokenAnalysisJob>(&json) {
                    Ok(job) => Ok(Some(job)),
                    Err(e) => {
                        warn!("Corrupted token analysis job data for {}: {}", job_id, e);
                        // Clean up corrupted data
                        let _: () = conn.del(&job_key).await?;
                        let _: () = conn.srem("token_analysis_jobs_index", job_id).await?;
                        let _: () = conn.zrem("token_analysis_jobs_timeline", job_id).await?;
                        Ok(None)
                    }
                }
            }
            None => Ok(None),
        }
    }

    /// Get all token analysis jobs with pagination, sorted by creation time (newest first)
    pub async fn get_all_token_analysis_jobs(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<TokenAnalysisJob>, usize)> {
        let mut conn = self.get_connection().await?;

        let total_count: usize = conn.zcard("token_analysis_jobs_timeline").await?;

        // Get job IDs from timeline, newest first (reverse order)
        let job_ids: Vec<String> = conn
            .zrevrange(
                "token_analysis_jobs_timeline",
                offset as isize,
                (offset + limit - 1) as isize,
            )
            .await?;

        // Fetch job details for each ID
        let mut jobs = Vec::new();
        for job_id in job_ids {
            if let Some(job) = self.get_token_analysis_job(&job_id).await? {
                jobs.push(job);
            }
        }

        Ok((jobs, total_count))
    }

    /// Get token analysis job statistics
    pub async fn get_token_analysis_job_stats(&self) -> Result<TokenAnalysisJobStats> {
        let mut conn = self.get_connection().await?;

        let total_jobs: usize = conn.zcard("token_analysis_jobs_timeline").await?;

        // Count jobs by status
        let all_job_ids: Vec<String> = conn.smembers("token_analysis_jobs_index").await?;

        let mut pending_count = 0u64;
        let mut running_count = 0u64;
        let mut completed_count = 0u64;
        let mut failed_count = 0u64;
        let mut cancelled_count = 0u64;

        for job_id in all_job_ids {
            if let Some(job) = self.get_token_analysis_job(&job_id).await? {
                match job.status {
                    JobStatus::Pending => pending_count += 1,
                    JobStatus::Running => running_count += 1,
                    JobStatus::Completed => completed_count += 1,
                    JobStatus::Failed => failed_count += 1,
                    JobStatus::Cancelled => cancelled_count += 1,
                }
            }
        }

        Ok(TokenAnalysisJobStats {
            total_jobs: total_jobs as u64,
            pending_jobs: pending_count,
            running_jobs: running_count,
            completed_jobs: completed_count,
            failed_jobs: failed_count,
            cancelled_jobs: cancelled_count,
        })
    }

    // =====================================
    // Work-Stealing for Multi-Instance Processing
    // =====================================

    /// Atomically claim a batch of wallet-token pairs for processing by a specific instance from all enabled chains
    /// Returns the claimed batch and a unique batch ID for tracking
    pub async fn claim_wallet_batch(
        &self,
        instance_id: &str,
        batch_size: usize,
    ) -> Result<(Vec<DiscoveredWalletToken>, String)> {
        // For backward compatibility, try to claim from all chains
        self.claim_wallet_batch_multichain(instance_id, batch_size, None)
            .await
    }

    /// Atomically claim a batch of wallet-token pairs for processing from specific chains or all chains
    /// Returns the claimed batch and a unique batch ID for tracking
    pub async fn claim_wallet_batch_multichain(
        &self,
        instance_id: &str,
        batch_size: usize,
        chains: Option<Vec<String>>,
    ) -> Result<(Vec<DiscoveredWalletToken>, String)> {
        let batch_id = format!(
            "{}:{}",
            instance_id,
            Uuid::new_v4().to_string()[..8].to_string()
        );
        let processing_key = format!("processing:{}", batch_id);

        // If no specific chains provided, use common chains or discover from available queues
        let target_chains = if let Some(chains) = chains {
            chains
        } else {
            // Default chains for backward compatibility and multichain support
            vec![
                "solana".to_string(),
                "ethereum".to_string(),
                "base".to_string(),
                "bsc".to_string(),
            ]
        };

        // Use Lua script for atomic multichain batch claiming
        let script = r#"
            local processing_key = KEYS[1]
            local batch_size = tonumber(ARGV[1])
            local ttl_seconds = tonumber(ARGV[2])
            local timestamp = ARGV[3]
            local instance_id = ARGV[4]
            local chain_count = tonumber(ARGV[5])

            -- Build queue keys from chain arguments
            local queue_keys = {}
            for i = 1, chain_count do
                local chain = ARGV[5 + i]
                table.insert(queue_keys, "discovered_wallet_token_pairs_queue:" .. chain)
            end

            -- Pop items from all chain queues in round-robin fashion
            local items = {}
            local items_per_chain = math.ceil(batch_size / chain_count)

            for _, queue_key in ipairs(queue_keys) do
                local chain_items = 0
                for i = 1, items_per_chain do
                    if #items >= batch_size then
                        break
                    end
                    local item = redis.call("RPOP", queue_key)
                    if item then
                        table.insert(items, item)
                        chain_items = chain_items + 1
                    else
                        break  -- No more items in this chain queue
                    end
                end
            end

            -- If we got items, set processing lock with metadata
            if #items > 0 then
                redis.call("HMSET", processing_key,
                    "timestamp", timestamp,
                    "count", #items,
                    "instance", instance_id)
                redis.call("EXPIRE", processing_key, ttl_seconds)

                -- Store the actual items for potential cleanup
                for i, item in ipairs(items) do
                    redis.call("LPUSH", processing_key .. ":items", item)
                end
                redis.call("EXPIRE", processing_key .. ":items", ttl_seconds)
            end

            return items
        "#;

        let mut conn = self.get_connection().await?;
        let timestamp = chrono::Utc::now().timestamp();
        let ttl_seconds = 300; // 5 minutes TTL for processing locks

        // Use EVAL command directly with individual arguments
        let mut cmd = redis::cmd("EVAL");
        cmd.arg(script)
            .arg(1) // number of keys
            .arg(&processing_key)
            .arg(batch_size)
            .arg(ttl_seconds)
            .arg(timestamp)
            .arg(instance_id)
            .arg(target_chains.len());

        // Add each chain as an argument
        for chain in &target_chains {
            cmd.arg(chain);
        }

        let items: Vec<String> = cmd.query_async(&mut conn).await?;

        // Deserialize the items
        let mut wallet_tokens = Vec::new();
        for item in items {
            match serde_json::from_str::<DiscoveredWalletToken>(&item) {
                Ok(wallet_token) => wallet_tokens.push(wallet_token),
                Err(e) => warn!("Failed to deserialize claimed wallet-token pair: {}", e),
            }
        }

        if !wallet_tokens.is_empty() {
            // Group by chain for logging
            let mut chain_counts: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for wallet_token in &wallet_tokens {
                *chain_counts.entry(wallet_token.chain.clone()).or_insert(0) += 1;
            }

            let chain_summary = chain_counts
                .iter()
                .map(|(chain, count)| format!("{}:{}", chain, count))
                .collect::<Vec<_>>()
                .join(", ");

            info!(
                "Claimed multichain batch {} with {} wallet-token pairs for instance {} ({})",
                batch_id,
                wallet_tokens.len(),
                instance_id,
                chain_summary
            );
        }

        Ok((wallet_tokens, batch_id))
    }

    /// Release a claimed batch (called when processing completes successfully)
    pub async fn release_batch_claim(&self, batch_id: &str) -> Result<()> {
        let processing_key = format!("processing:{}", batch_id);
        let items_key = format!("{}:items", processing_key);

        let mut conn = self.get_connection().await?;

        // Delete both the metadata and items
        let _: () = conn.del(&[&processing_key, &items_key]).await?;

        debug!("Released batch claim: {}", batch_id);
        Ok(())
    }

    /// Return failed items back to their respective chain queues for retry
    pub async fn return_failed_batch(
        &self,
        batch_id: &str,
        failed_items: &[DiscoveredWalletToken],
    ) -> Result<()> {
        let processing_key = format!("processing:{}", batch_id);
        let items_key = format!("{}:items", processing_key);

        let mut conn = self.get_connection().await?;

        // Group failed items by chain
        let mut chain_groups: std::collections::HashMap<String, Vec<&DiscoveredWalletToken>> =
            std::collections::HashMap::new();
        for item in failed_items {
            chain_groups
                .entry(item.chain.clone())
                .or_default()
                .push(item);
        }

        // Push failed items back to their respective chain queues for priority retry
        for (chain, chain_items) in chain_groups {
            let queue_key = format!("discovered_wallet_token_pairs_queue:{}", chain);

            for item in chain_items {
                let json_data = serde_json::to_string(item)?;
                let _: () = conn.lpush(&queue_key, json_data).await?;
            }
        }

        // Clean up the processing keys
        let _: () = conn.del(&[&processing_key, &items_key]).await?;

        if !failed_items.is_empty() {
            info!(
                "Returned {} failed items from batch {} back to chain-specific queues",
                failed_items.len(),
                batch_id
            );
        }

        Ok(())
    }

    /// Cleanup stale processing locks from dead instances
    pub async fn cleanup_stale_processing_locks(&self, max_age_seconds: u64) -> Result<usize> {
        let mut conn = self.get_connection().await?;
        let current_time = chrono::Utc::now().timestamp();
        let cutoff_time = current_time - max_age_seconds as i64;

        // Find all processing keys
        let pattern = "processing:*";
        let processing_keys: Vec<String> = conn.keys(pattern).await?;

        let mut cleaned_count = 0;
        let queue_key = "discovered_wallet_token_pairs_queue";

        for key in processing_keys {
            // Skip item keys (they have :items suffix)
            if key.ends_with(":items") {
                continue;
            }

            // Get the timestamp from the processing metadata
            let timestamp: Option<i64> = conn.hget(&key, "timestamp").await?;

            match timestamp {
                Some(ts) if ts < cutoff_time => {
                    // This lock is stale, return items to queue
                    let items_key = format!("{}:items", key);
                    let items: Vec<String> = conn.lrange(&items_key, 0, -1).await?;

                    // Return items to the front of the queue
                    for item in items.iter().rev() {
                        // Reverse to maintain order
                        let _: () = conn.lpush(queue_key, item).await?;
                    }

                    // Delete the stale processing keys
                    let _: () = conn.del(&[&key, &items_key]).await?;
                    cleaned_count += 1;

                    if !items.is_empty() {
                        warn!(
                            "Cleaned up stale processing lock {} and returned {} items to queue",
                            key,
                            items.len()
                        );
                    }
                }
                Some(_) => {
                    // Lock is still fresh, skip
                }
                None => {
                    // Malformed lock, clean it up
                    let items_key = format!("{}:items", key);
                    let _: () = conn.del(&[&key, &items_key]).await?;
                    cleaned_count += 1;
                    warn!("Cleaned up malformed processing lock: {}", key);
                }
            }
        }

        if cleaned_count > 0 {
            info!("Cleaned up {} stale processing locks", cleaned_count);
        }

        Ok(cleaned_count)
    }

    /// Get statistics about current processing locks
    pub async fn get_processing_stats(&self) -> Result<(usize, usize)> {
        let mut conn = self.get_connection().await?;

        // Count processing locks (not including :items keys)
        let pattern = "processing:*";
        let all_keys: Vec<String> = conn.keys(pattern).await?;
        let processing_locks = all_keys.iter().filter(|k| !k.ends_with(":items")).count();

        // Get queue size
        let queue_key = "discovered_wallet_token_pairs_queue";
        let queue_size: usize = conn.llen(queue_key).await?;

        Ok((processing_locks, queue_size))
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
