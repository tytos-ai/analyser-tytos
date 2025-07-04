use chrono::Utc;
use config_manager::SystemConfig;
use futures::future::join_all;
use dex_client::{BirdEyeClient, BirdEyeError, TraderTransaction, GeneralTraderTransaction};
use pnl_core::{FinancialEvent, EventType, EventMetadata};
use persistence_layer::{PersistenceError, RedisClient, DiscoveredWalletToken};
use pnl_core::{AnalysisTimeframe, PnLFilters, PnLReport, calculate_pnl_with_embedded_prices, enhance_report_with_current_prices};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

// Legacy module removed - trending functionality moved to BirdEye
// pub mod trending_orchestrator;
pub mod birdeye_trending_orchestrator;

// Legacy trending orchestrator removed - use BirdEye instead
// pub use trending_orchestrator::{TrendingOrchestrator, TrendingCycleStats};
pub use birdeye_trending_orchestrator::{BirdEyeTrendingOrchestrator, DiscoveryStats};

#[derive(Error, Debug, Clone)]
pub enum OrchestratorError {
    #[error("Persistence error: {0}")]
    Persistence(String),
    #[error("P&L calculation error: {0}")]
    PnL(String),
    #[error("BirdEye price client error: {0}")]
    BirdEyePrice(String),
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("Lock acquisition failed")]
    LockFailed,
    #[error("Job execution failed: {0}")]
    JobExecution(String),
    #[error("Invalid wallet address: {0}")]
    InvalidWallet(String),
    #[error("Anyhow error: {0}")]
    Anyhow(String),
}

impl From<anyhow::Error> for OrchestratorError {
    fn from(err: anyhow::Error) -> Self {
        OrchestratorError::Anyhow(err.to_string())
    }
}

impl From<PersistenceError> for OrchestratorError {
    fn from(err: PersistenceError) -> Self {
        OrchestratorError::Persistence(err.to_string())
    }
}

impl From<pnl_core::PnLError> for OrchestratorError {
    fn from(err: pnl_core::PnLError) -> Self {
        OrchestratorError::PnL(err.to_string())
    }
}




impl From<BirdEyeError> for OrchestratorError {
    fn from(err: BirdEyeError) -> Self {
        OrchestratorError::BirdEyePrice(err.to_string())
    }
}

impl From<config_manager::ConfigurationError> for OrchestratorError {
    fn from(err: config_manager::ConfigurationError) -> Self {
        OrchestratorError::Config(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, OrchestratorError>;

/// Status of a P&L analysis job
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// P&L analysis job information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PnLJob {
    pub id: Uuid,
    pub wallet_address: String,
    pub status: JobStatus,
    pub created_at: chrono::DateTime<Utc>,
    pub started_at: Option<chrono::DateTime<Utc>>,
    pub completed_at: Option<chrono::DateTime<Utc>>,
    pub error_message: Option<String>,
    pub filters: PnLFilters,
    pub result: Option<PnLReport>,
}

impl PnLJob {
    pub fn new(wallet_address: String, filters: PnLFilters) -> Self {
        Self {
            id: Uuid::new_v4(),
            wallet_address,
            status: JobStatus::Pending,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            error_message: None,
            filters,
            result: None,
        }
    }
}

/// Batch P&L analysis job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchJob {
    pub id: Uuid,
    pub wallet_addresses: Vec<String>,
    pub status: JobStatus,
    pub created_at: chrono::DateTime<Utc>,
    pub started_at: Option<chrono::DateTime<Utc>>,
    pub completed_at: Option<chrono::DateTime<Utc>>,
    pub filters: PnLFilters,
    pub individual_jobs: Vec<Uuid>,
    #[serde(skip)]
    pub results: HashMap<String, Result<PnLReport>>,
}

impl BatchJob {
    pub fn new(wallet_addresses: Vec<String>, filters: PnLFilters) -> Self {
        Self {
            id: Uuid::new_v4(),
            wallet_addresses,
            status: JobStatus::Pending,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            filters,
            individual_jobs: Vec::new(),
            results: HashMap::new(),
        }
    }

    /// Convert to persistence layer BatchJob format
    pub fn to_persistence_batch_job(&self) -> Result<persistence_layer::BatchJob> {
        let filters_json = serde_json::to_value(&self.filters)
            .map_err(|e| OrchestratorError::JobExecution(format!("Failed to serialize filters: {}", e)))?;
        
        Ok(persistence_layer::BatchJob {
            id: self.id,
            wallet_addresses: self.wallet_addresses.clone(),
            status: match self.status {
                JobStatus::Pending => persistence_layer::JobStatus::Pending,
                JobStatus::Running => persistence_layer::JobStatus::Running,
                JobStatus::Completed => persistence_layer::JobStatus::Completed,
                JobStatus::Failed => persistence_layer::JobStatus::Failed,
                JobStatus::Cancelled => persistence_layer::JobStatus::Cancelled,
            },
            created_at: self.created_at,
            started_at: self.started_at,
            completed_at: self.completed_at,
            filters: filters_json,
            individual_jobs: self.individual_jobs.clone(),
        })
    }

    /// Create from persistence layer BatchJob format
    pub fn from_persistence_batch_job(persistent_job: persistence_layer::BatchJob) -> Result<Self> {
        let filters: PnLFilters = serde_json::from_value(persistent_job.filters)
            .map_err(|e| OrchestratorError::JobExecution(format!("Failed to deserialize filters: {}", e)))?;
        
        Ok(Self {
            id: persistent_job.id,
            wallet_addresses: persistent_job.wallet_addresses,
            status: match persistent_job.status {
                persistence_layer::JobStatus::Pending => JobStatus::Pending,
                persistence_layer::JobStatus::Running => JobStatus::Running,
                persistence_layer::JobStatus::Completed => JobStatus::Completed,
                persistence_layer::JobStatus::Failed => JobStatus::Failed,
                persistence_layer::JobStatus::Cancelled => JobStatus::Cancelled,
            },
            created_at: persistent_job.created_at,
            started_at: persistent_job.started_at,
            completed_at: persistent_job.completed_at,
            filters,
            individual_jobs: persistent_job.individual_jobs,
            results: HashMap::new(), // Results are loaded separately
        })
    }
}

/// Job orchestrator for managing P&L analysis tasks
pub struct JobOrchestrator {
    config: SystemConfig,
    birdeye_client: BirdEyeClient,
    redis_client: Arc<Mutex<RedisClient>>,
    running_jobs: Arc<Mutex<HashMap<Uuid, PnLJob>>>,
    // batch_jobs now stored in Redis via persistence_layer
    continuous_mode_filters: Arc<Mutex<Option<PnLFilters>>>,  // Runtime config for continuous mode
}

impl JobOrchestrator {
    pub async fn new(config: SystemConfig) -> Result<Self> {
        // Initialize Redis client
        let redis_client = RedisClient::new(&config.redis.url).await?;
        let redis_client = Arc::new(Mutex::new(redis_client));

        // Initialize BirdEye client and price fetcher
        let birdeye_config = config.birdeye.clone();
        
        let birdeye_client = BirdEyeClient::new(birdeye_config.clone())?;

        Ok(Self {
            config,
            birdeye_client,
            redis_client,
            running_jobs: Arc::new(Mutex::new(HashMap::new())),
            continuous_mode_filters: Arc::new(Mutex::new(None)),
        })
    }

    /// Start continuous mode monitoring
    pub async fn start_continuous_mode(&self) -> Result<()> {
        info!("Starting continuous mode P&L processing...");

        loop {
            match self.run_continuous_cycle().await {
                Ok(_) => {
                    debug!("Continuous cycle completed successfully");
                }
                Err(e) => {
                    error!("Continuous cycle failed: {}", e);
                }
            }

            // Sleep between cycles
            sleep(Duration::from_millis(self.config.system.process_loop_ms)).await;
        }
    }

    /// Run a single continuous mode cycle
    async fn run_continuous_cycle(&self) -> Result<()> {
        // Try to acquire the aggregator lock
        let lock = {
            let redis = self.redis_client.lock().await;
            redis.acquire_lock("aggregator-lock", self.config.redis.default_lock_ttl_seconds).await
        };

        let lock_handle = match lock {
            Ok(handle) => handle,
            Err(PersistenceError::LockFailed) => {
                debug!("Another instance is processing, skipping cycle");
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        };

        info!("Acquired aggregator lock, processing discovered wallets...");

        // Process discovered wallets
        let result = self.process_discovered_wallets().await;

        // Release the lock
        {
            let redis = self.redis_client.lock().await;
            if let Err(e) = redis.release_lock(&lock_handle).await {
                warn!("Failed to release lock: {}", e);
            }
        }

        result
    }

    /// Run a single continuous mode cycle for testing (returns true if processed a pair)
    pub async fn start_continuous_mode_single_cycle(&self) -> Result<bool> {
        // Try to acquire the aggregator lock
        let lock = {
            let redis = self.redis_client.lock().await;
            redis.acquire_lock("aggregator-lock", self.config.redis.default_lock_ttl_seconds).await
        };

        let lock_handle = match lock {
            Ok(handle) => handle,
            Err(PersistenceError::LockFailed) => {
                debug!("Another instance is processing, skipping cycle");
                return Ok(false);
            }
            Err(e) => return Err(e.into()),
        };

        debug!("Acquired aggregator lock, processing single wallet-token pair...");

        // Process just one wallet-token pair
        let processed = self.process_single_discovered_wallet().await?;

        // Release the lock
        {
            let redis = self.redis_client.lock().await;
            if let Err(e) = redis.release_lock(&lock_handle).await {
                warn!("Failed to release lock: {}", e);
            }
        }

        Ok(processed)
    }

    /// Process a single wallet-token pair from the discovery queue
    async fn process_single_discovered_wallet(&self) -> Result<bool> {
        // Pop a single wallet-token pair from the discovery queue
        let wallet_token_pair = {
            let redis = self.redis_client.lock().await;
            redis.pop_discovered_wallet_token_pair(1).await?
        };

        let pair = match wallet_token_pair {
            Some(pair) => pair,
            None => {
                debug!("No wallet-token pairs in discovery queue");
                return Ok(false);
            }
        };

        info!("Processing discovered wallet-token pair: {} for {} ({})", 
              pair.wallet_address, pair.token_symbol, pair.token_address);

        // Get effective P&L filters (user override or config default)
        let filters = self.get_effective_continuous_filters().await;

        // Process the wallet-token pair using targeted BirdEye transactions
        match self.process_single_wallet_token_pair(&pair, filters).await {
            Ok(report) => {
                info!("Successfully processed wallet {} for token {}: P&L = {} USD", 
                      pair.wallet_address, pair.token_symbol, report.summary.total_pnl_usd);
                
                // Store the P&L result in Redis for later retrieval
                {
                    let redis = self.redis_client.lock().await;
                    match redis.store_pnl_result(
                        &pair.wallet_address,
                        &pair.token_address,
                        &pair.token_symbol,
                        &report,
                    ).await {
                        Ok(_) => {
                            // Mark wallet as successfully processed
                            if let Err(e) = redis.mark_wallet_as_processed(&pair.wallet_address).await {
                                warn!("Failed to mark wallet {} as processed: {}", pair.wallet_address, e);
                            } else {
                                debug!("Marked wallet {} as processed and stored P&L result", pair.wallet_address);
                            }
                        }
                        Err(e) => {
                            // Mark wallet as failed if storage fails
                            if let Err(mark_err) = redis.mark_wallet_as_failed(&pair.wallet_address).await {
                                warn!("Failed to mark wallet {} as failed: {}", pair.wallet_address, mark_err);
                            }
                            warn!("Failed to store P&L result for wallet {}: {}", pair.wallet_address, e);
                        }
                    }
                }
                
                Ok(true)
            }
            Err(e) => {
                // Mark wallet as failed
                {
                    let redis = self.redis_client.lock().await;
                    if let Err(mark_err) = redis.mark_wallet_as_failed(&pair.wallet_address).await {
                        warn!("Failed to mark wallet {} as failed: {}", pair.wallet_address, mark_err);
                    }
                }
                
                warn!("Failed to process wallet {} for token {}: {}", 
                      pair.wallet_address, pair.token_symbol, e);
                Ok(true) // Still processed (even if failed)
            }
        }
    }

    /// Process wallet-token pairs discovered by BirdEye (PARALLEL VERSION)
    async fn process_discovered_wallets(&self) -> Result<()> {
        let mut total_processed_count = 0;
        
        // Configurable batch size for parallel processing
        let batch_size = self.config.system.pnl_parallel_batch_size.unwrap_or(10);
        
        loop {
            // Pop multiple wallet-token pairs from the discovery queue for parallel processing
            let wallet_token_pairs = {
                let redis = self.redis_client.lock().await;
                redis.pop_discovered_wallet_token_pairs(batch_size).await?
            };

            if wallet_token_pairs.is_empty() {
                debug!("No more wallet-token pairs in discovery queue");
                break;
            }

            info!("🚀 Processing {} wallet-token pairs in parallel...", wallet_token_pairs.len());

            // Get effective P&L filters (user override or config default)
            let filters = self.get_effective_continuous_filters().await;

            // Process wallet-token pairs in parallel with timeout
            let futures = wallet_token_pairs.iter().map(|pair| {
                let filters = filters.clone();
                let pair_clone = pair.clone();
                async move {
                    info!("Processing discovered wallet-token pair: {} for {} ({})", 
                          pair_clone.wallet_address, pair_clone.token_symbol, pair_clone.token_address);
                    
                    // Add timeout for each wallet processing (5 minutes max for queue processing)
                    let timeout_duration = Duration::from_secs(300);
                    let result = match tokio::time::timeout(
                        timeout_duration, 
                        self.process_single_wallet_token_pair(&pair_clone, filters)
                    ).await {
                        Ok(Ok(report)) => {
                            // Store the P&L result in Redis for later retrieval
                            let store_result = {
                                let redis = self.redis_client.lock().await;
                                redis.store_pnl_result(
                                    &pair_clone.wallet_address,
                                    &pair_clone.token_address,
                                    &pair_clone.token_symbol,
                                    &report,
                                ).await
                            };
                            
                            match store_result {
                                Ok(_) => {
                                    // Mark wallet as successfully processed
                                    let redis = self.redis_client.lock().await;
                                    if let Err(e) = redis.mark_wallet_as_processed(&pair_clone.wallet_address).await {
                                        warn!("Failed to mark wallet {} as processed: {}", pair_clone.wallet_address, e);
                                    }
                                    
                                    info!("✅ Successfully processed wallet {} for token {}: P&L = {} USD", 
                                          pair_clone.wallet_address, pair_clone.token_symbol, report.summary.total_pnl_usd);
                                    Ok(())
                                }
                                Err(e) => {
                                    // Mark wallet as failed
                                    let redis = self.redis_client.lock().await;
                                    if let Err(mark_err) = redis.mark_wallet_as_failed(&pair_clone.wallet_address).await {
                                        warn!("Failed to mark wallet {} as failed: {}", pair_clone.wallet_address, mark_err);
                                    }
                                    
                                    warn!("Failed to store P&L result for wallet {}: {}", pair_clone.wallet_address, e);
                                    Err(OrchestratorError::Persistence(e.to_string()))
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            // Mark wallet as failed
                            let redis = self.redis_client.lock().await;
                            if let Err(mark_err) = redis.mark_wallet_as_failed(&pair_clone.wallet_address).await {
                                warn!("Failed to mark wallet {} as failed: {}", pair_clone.wallet_address, mark_err);
                            }
                            
                            warn!("Failed to process wallet {} for token {}: {}", 
                                  pair_clone.wallet_address, pair_clone.token_symbol, e);
                            Err(e)
                        }
                        Err(_) => {
                            // Mark wallet as failed due to timeout
                            let redis = self.redis_client.lock().await;
                            if let Err(mark_err) = redis.mark_wallet_as_failed(&pair_clone.wallet_address).await {
                                warn!("Failed to mark wallet {} as failed: {}", pair_clone.wallet_address, mark_err);
                            }
                            
                            warn!("⏰ Wallet {} for token {} timed out after {} seconds", 
                                  pair_clone.wallet_address, pair_clone.token_symbol, timeout_duration.as_secs());
                            Err(OrchestratorError::JobExecution(
                                format!("Wallet processing timed out after {} seconds", timeout_duration.as_secs())
                            ))
                        }
                    };
                    (pair_clone.wallet_address.clone(), pair_clone.token_symbol.clone(), result)
                }
            });

            // Execute all wallet processing tasks in parallel
            let results = join_all(futures).await;
            
            // Categorize results and collect failed wallets for retry
            let mut success_count = 0;
            let mut _failed_count = 0;
            let mut rate_limited_count = 0;
            let mut other_failed_count = 0;
            let mut wallets_to_retry = Vec::new();
            
            for ((wallet, token, result), original_pair) in results.into_iter().zip(wallet_token_pairs.iter()) {
                match result {
                    Ok(_) => success_count += 1,
                    Err(e) => {
                        _failed_count += 1;
                        let error_msg = e.to_string();
                        
                        // Check if this is a rate limit error that should be retried
                        if error_msg.contains("Rate limit exceeded") || error_msg.contains("rate limit") {
                            rate_limited_count += 1;
                            wallets_to_retry.push(original_pair.clone());
                            debug!("Rate-limited wallet {} for token {} will be retried", wallet, token);
                        } else {
                            other_failed_count += 1;
                            warn!("Wallet {} for token {} failed permanently: {}", wallet, token, error_msg);
                        }
                    }
                }
            }
            
            // Push rate-limited wallets back to the front of the queue for retry
            if !wallets_to_retry.is_empty() {
                let redis = self.redis_client.lock().await;
                if let Err(e) = redis.push_failed_wallet_token_pairs_for_retry(&wallets_to_retry).await {
                    warn!("Failed to push {} rate-limited wallets back to queue: {}", wallets_to_retry.len(), e);
                } else {
                    info!("🔄 Pushed {} rate-limited wallets back to queue for retry", wallets_to_retry.len());
                }
            }
            
            total_processed_count += wallet_token_pairs.len();
            
            info!("📊 Parallel batch completed: {}/{} successful, {} rate-limited (retrying), {} permanently failed", 
                  success_count, wallet_token_pairs.len(), rate_limited_count, other_failed_count);

            // Adaptive delay between parallel batches based on rate limiting
            let delay_ms = if rate_limited_count > 0 {
                // Longer delay if we hit rate limits to give API time to recover
                let base_delay = 2000; // 2 seconds base
                let additional_delay = rate_limited_count * 200; // +200ms per rate-limited wallet
                base_delay + additional_delay
            } else {
                200 // Normal delay when no rate limits
            };
            
            if rate_limited_count > 0 {
                info!("⏱️ Applying extended delay of {}ms due to {} rate-limited wallets", delay_ms, rate_limited_count);
            }
            
            sleep(Duration::from_millis(delay_ms)).await;
        }

        if total_processed_count > 0 {
            info!("🎯 Total processed {} discovered wallet-token pairs using parallel processing", total_processed_count);
        }

        Ok(())
    }

    /// Submit a batch P&L job
    pub async fn submit_batch_job(
        &self,
        wallet_addresses: Vec<String>,
        filters: Option<PnLFilters>,
    ) -> Result<Uuid> {
        let filters = self.merge_filters_with_config(filters);
        let batch_job = BatchJob::new(wallet_addresses.clone(), filters);
        let job_id = batch_job.id;

        // Store the batch job in Redis for persistence
        {
            let redis = self.redis_client.lock().await;
            let persistent_job = batch_job.to_persistence_batch_job()?;
            redis.store_batch_job(&persistent_job).await?;
        }

        // Process in background
        let orchestrator = self.clone();
        tokio::spawn(async move {
            if let Err(e) = orchestrator.execute_batch_job(job_id).await {
                error!("Batch job {} failed: {}", job_id, e);
            }
        });

        info!("Submitted batch job {} for {} wallets", job_id, wallet_addresses.len());

        Ok(job_id)
    }

    /// Execute a batch job
    async fn execute_batch_job(&self, job_id: Uuid) -> Result<()> {
        // Load job from Redis and update status to Running
        let (wallet_addresses, filters) = {
            let redis = self.redis_client.lock().await;
            let persistent_job = redis.get_batch_job(&job_id.to_string()).await?
                .ok_or_else(|| OrchestratorError::JobExecution(format!("Batch job {} not found", job_id)))?;
            
            let mut job = BatchJob::from_persistence_batch_job(persistent_job)?;
            job.status = JobStatus::Running;
            job.started_at = Some(Utc::now());

            // Update status in Redis
            let updated_persistent_job = job.to_persistence_batch_job()?;
            redis.update_batch_job(&updated_persistent_job).await?;

            (job.wallet_addresses.clone(), job.filters.clone())
        };

        info!("Executing batch job {} for {} wallets", job_id, wallet_addresses.len());

        // Process wallets in parallel with timeout
        let futures = wallet_addresses.iter().map(|wallet| {
            let filters = filters.clone();
            let wallet_clone = wallet.clone();
            async move {
                // Add timeout for each wallet processing (10 minutes max)
                let timeout_duration = Duration::from_secs(600);
                let result = match tokio::time::timeout(
                    timeout_duration, 
                    self.process_single_wallet(&wallet_clone, filters)
                ).await {
                    Ok(Ok(report)) => Ok(report),
                    Ok(Err(e)) => Err(e),
                    Err(_) => Err(OrchestratorError::JobExecution(
                        format!("Wallet processing timed out after {} seconds", timeout_duration.as_secs())
                    )),
                };
                (wallet_clone, result)
            }
        });

        let results = join_all(futures).await;

        // Update batch job status to Completed in Redis
        let successful_count = {
            let redis = self.redis_client.lock().await;
            let persistent_job = redis.get_batch_job(&job_id.to_string()).await?
                .ok_or_else(|| OrchestratorError::JobExecution(format!("Batch job {} not found", job_id)))?;
            
            let mut job = BatchJob::from_persistence_batch_job(persistent_job)?;
            job.status = JobStatus::Completed;
            job.completed_at = Some(Utc::now());

            // Add results to the job for counting
            for (wallet, result) in &results {
                job.results.insert(wallet.clone(), result.clone());
            }

            let success_count = job.results.values().filter(|r| r.is_ok()).count();

            // Store the results separately in Redis
            let results_map: HashMap<String, Result<PnLReport>> = results.into_iter().collect();
            // Convert to anyhow::Error format expected by persistence layer
            let anyhow_results: HashMap<String, std::result::Result<PnLReport, anyhow::Error>> = results_map
                .into_iter()
                .map(|(wallet, result)| {
                    let anyhow_result = result.map_err(|e| anyhow::anyhow!("{}", e));
                    (wallet, anyhow_result)
                })
                .collect();
            redis.store_batch_job_results(&job_id.to_string(), &anyhow_results).await
                .map_err(|e| anyhow::anyhow!("Failed to store batch job results: {}", e))?;

            // Update final status in Redis
            let updated_persistent_job = job.to_persistence_batch_job()?;
            redis.update_batch_job(&updated_persistent_job).await?;

            success_count
        };

        info!("Batch job {} completed: {}/{} wallets successful", 
              job_id, successful_count, wallet_addresses.len());

        Ok(())
    }

    /// Get batch job status
    pub async fn get_batch_job_status(&self, job_id: Uuid) -> Option<BatchJob> {
        let redis = self.redis_client.lock().await;
        match redis.get_batch_job(&job_id.to_string()).await {
            Ok(Some(persistent_job)) => {
                match BatchJob::from_persistence_batch_job(persistent_job) {
                    Ok(job) => Some(job),
                    Err(e) => {
                        error!("Failed to convert persistent batch job: {}", e);
                        None
                    }
                }
            }
            Ok(None) => None,
            Err(e) => {
                error!("Failed to load batch job from Redis: {}", e);
                None
            }
        }
    }

    /// Get batch job results from Redis
    pub async fn get_batch_job_results(&self, job_id: &str) -> anyhow::Result<HashMap<String, Result<PnLReport>>> {
        let redis = self.redis_client.lock().await;
        let results = redis.get_batch_job_results(job_id).await
            .map_err(|e| anyhow::anyhow!("Failed to load batch job results from Redis: {}", e))?;
        
        // Convert anyhow::Error to OrchestratorError for the HashMap
        let converted_results: HashMap<String, Result<PnLReport>> = results
            .into_iter()
            .map(|(wallet, result)| {
                let converted_result = result.map_err(|e| OrchestratorError::PnL(e.to_string()));
                (wallet, converted_result)
            })
            .collect();
        
        Ok(converted_results)
    }

    /// Get all batch jobs with pagination
    pub async fn get_all_batch_jobs(&self, limit: usize, offset: usize) -> Result<(Vec<BatchJob>, usize)> {
        let redis = self.redis_client.lock().await;
        let (persistent_jobs, total_count) = redis.get_all_batch_jobs(limit, offset).await?;
        
        let mut jobs = Vec::new();
        for persistent_job in persistent_jobs {
            match BatchJob::from_persistence_batch_job(persistent_job) {
                Ok(job) => jobs.push(job),
                Err(e) => {
                    warn!("Failed to convert persistent batch job: {}", e);
                }
            }
        }
        
        Ok((jobs, total_count))
    }

    /// Process a single wallet-token pair for targeted P&L analysis using BirdEye transactions
    pub async fn process_single_wallet_token_pair(
        &self,
        pair: &DiscoveredWalletToken,
        filters: PnLFilters,
    ) -> Result<PnLReport> {
        debug!("Starting targeted P&L analysis for wallet: {} on token: {} ({})", 
               pair.wallet_address, pair.token_symbol, pair.token_address);

        // Fetch all trading transactions for the wallet using BirdEye with pagination
        let max_total_transactions = filters.max_transactions_to_fetch
            .unwrap_or(self.config.birdeye.default_max_transactions);
        
        // Extract time bounds from filters for BirdEye API optimization
        let (from_time, to_time) = Self::extract_time_bounds_for_birdeye(&filters);
        debug!("Fetching up to {} transactions for wallet-token pair {} with time bounds: {:?} to {:?}", 
               max_total_transactions, pair.wallet_address, from_time, to_time);
        
        let transactions = self
            .birdeye_client
            .get_all_trader_transactions_paginated(&pair.wallet_address, from_time, to_time, max_total_transactions)
            .await?;

        if transactions.is_empty() {
            return Err(OrchestratorError::JobExecution(format!(
                "No BirdEye transactions found for wallet: {} on token: {}",
                pair.wallet_address, pair.token_symbol
            )));
        }

        info!("📊 Found {} BirdEye transactions for {} trading {}", 
              transactions.len(), pair.wallet_address, pair.token_symbol);

        // Convert BirdEye transactions to financial events (use general method)
        let events = self
            .convert_general_birdeye_transactions_to_events(&transactions, &pair.wallet_address)?;

        if events.is_empty() {
            return Err(OrchestratorError::JobExecution(format!(
                "No financial events found for wallet: {} on token: {}",
                pair.wallet_address, pair.token_symbol
            )));
        }

        // Calculate P&L using the targeted transactions with embedded prices
        let mut report = calculate_pnl_with_embedded_prices(&pair.wallet_address, events, filters)
            .await?;

        // Enhance with current market prices for accurate unrealized P&L and SOL fees
        match enhance_report_with_current_prices(report.clone(), &self.birdeye_client).await {
            Ok(enhanced_report) => {
                report = enhanced_report;
                debug!("Enhanced report with current market prices");
            }
            Err(e) => {
                warn!("Failed to enhance report with current prices: {}", e);
                // Continue with original report
            }
        }

        debug!("✅ Targeted P&L analysis completed for wallet: {} on token: {}", 
               pair.wallet_address, pair.token_symbol);

        Ok(report)
    }

    /// Process a single wallet for P&L analysis (legacy method using Solana RPC)
    pub async fn process_single_wallet(
        &self,
        wallet_address: &str,
        filters: PnLFilters,
    ) -> Result<PnLReport> {
        debug!("Starting P&L analysis for wallet: {} using BirdEye API", wallet_address);

        // Fetch all trading transactions for the wallet using BirdEye with pagination
        let max_total_transactions = filters.max_transactions_to_fetch
            .unwrap_or(self.config.birdeye.default_max_transactions);
        
        // Extract time bounds from filters for BirdEye API optimization
        let (from_time, to_time) = Self::extract_time_bounds_for_birdeye(&filters);
        debug!("Fetching up to {} transactions for wallet {} with time bounds: {:?} to {:?}", 
               max_total_transactions, wallet_address, from_time, to_time);
        
        let transactions = self
            .birdeye_client
            .get_all_trader_transactions_paginated(wallet_address, from_time, to_time, max_total_transactions)
            .await?;

        if transactions.is_empty() {
            return Err(OrchestratorError::JobExecution(format!(
                "No transactions found for wallet: {}",
                wallet_address
            )));
        }

        info!("📊 Found {} BirdEye transactions for wallet {}", 
              transactions.len(), wallet_address);

        // Convert BirdEye transactions to financial events
        let events = self
            .convert_general_birdeye_transactions_to_events(&transactions, wallet_address)?;

        if events.is_empty() {
            return Err(OrchestratorError::JobExecution(format!(
                "No financial events found for wallet: {}",
                wallet_address
            )));
        }

        // Calculate P&L with embedded prices from BirdEye transactions
        let mut report = calculate_pnl_with_embedded_prices(wallet_address, events, filters)
            .await?;

        // Enhance with current market prices for accurate unrealized P&L and SOL fees
        match enhance_report_with_current_prices(report.clone(), &self.birdeye_client).await {
            Ok(enhanced_report) => {
                report = enhanced_report;
                debug!("Enhanced report with current market prices");
            }
            Err(e) => {
                warn!("Failed to enhance report with current prices: {}", e);
                // Continue with original report
            }
        }

        debug!("✅ P&L analysis completed for wallet: {} using BirdEye data", wallet_address);

        Ok(report)
    }

    /// Convert BirdEye transactions to FinancialEvents for P&L analysis
    #[allow(dead_code)]
    fn convert_birdeye_transactions_to_events(
        &self,
        transactions: &[TraderTransaction],
        wallet_address: &str,
    ) -> Result<Vec<FinancialEvent>> {
        let mut events = Vec::new();

        for tx in transactions {
            let event_type = match tx.side.as_str() {
                "buy" => EventType::Buy,
                "sell" => EventType::Sell,
                _ => continue, // Skip unknown transaction types
            };

            // Convert timestamp from Unix time to DateTime
            let timestamp = chrono::DateTime::<chrono::Utc>::from_timestamp(tx.block_unix_time, 0)
                .unwrap_or_else(|| chrono::Utc::now());

            // Create extra metadata map for additional BirdEye data
            let mut extra = HashMap::new();
            if let Some(ref symbol) = tx.token_symbol {
                extra.insert("token_symbol".to_string(), symbol.clone());
            }
            if let Some(ref pool) = tx.pool_address {
                extra.insert("pool_address".to_string(), pool.clone());
            }
            extra.insert("volume_usd".to_string(), tx.volume_usd.to_string());

            let token_amount = Decimal::try_from(tx.token_amount).unwrap_or(Decimal::ZERO);
            let token_price_usd = Decimal::try_from(tx.token_price).unwrap_or(Decimal::ZERO);

            let event = FinancialEvent {
                id: Uuid::new_v4(),
                transaction_id: tx.tx_hash.clone(),
                wallet_address: wallet_address.to_string(),
                event_type,
                token_mint: tx.token_address.clone(),
                token_amount,
                sol_amount: Decimal::ZERO, // Legacy BirdEye doesn't provide SOL-specific data
                usd_value: token_amount * token_price_usd, // USD value calculated from amount × price
                timestamp,
                transaction_fee: Decimal::ZERO, // BirdEye doesn't provide fees
                metadata: EventMetadata {
                    program_id: tx.source.clone(),
                    instruction_index: None,
                    exchange: tx.source.clone(), // Use source as exchange identifier
                    price_per_token: Some(token_price_usd),
                    extra,
                },
            };

            events.push(event);
        }

        info!("✅ Converted {} BirdEye transactions to {} financial events for wallet {}", 
              transactions.len(), events.len(), wallet_address);

        Ok(events)
    }

    /// Convert general BirdEye transactions to FinancialEvents for P&L analysis
    pub fn convert_general_birdeye_transactions_to_events(
        &self,
        transactions: &[GeneralTraderTransaction],
        wallet_address: &str,
    ) -> Result<Vec<FinancialEvent>> {
        let mut events = Vec::new();

        for tx in transactions {
            // Process both swaps and transfers to match TypeScript logic
            let main_operation = match tx.tx_type.as_str() {
                "swap" => "swap",
                _ => {
                    // Check if this is a transfer based on transfer_type fields
                    let is_transfer = tx.quote.transfer_type.as_ref()
                        .map(|t| t.contains("transfer") || t.contains("mintTo") || t.contains("burn"))
                        .unwrap_or(false)
                        || tx.base.transfer_type.as_ref()
                        .map(|t| t.contains("transfer") || t.contains("mintTo") || t.contains("burn"))
                        .unwrap_or(false);
                    
                    if is_transfer {
                        "transfer"
                    } else {
                        debug!("Skipping unknown transaction type: {} for {}", tx.tx_type, tx.tx_hash);
                        continue;
                    }
                }
            };
            
            debug!("Processing {} transaction: {}", main_operation, tx.tx_hash);

            // Convert timestamp from Unix time to DateTime
            let timestamp = chrono::DateTime::<chrono::Utc>::from_timestamp(tx.block_unix_time, 0)
                .unwrap_or_else(|| chrono::Utc::now());

            // Create TWO FinancialEvents - one for each side of the transaction (like TypeScript)
            // This matches the TypeScript approach where each transaction creates buy/sell pairs

            // Process the "quote" side (usually the token being sold/transferred out)
            if tx.quote.type_swap == "from" && tx.quote.ui_change_amount < 0.0 {
                let quote_amount = Decimal::try_from(tx.quote.ui_amount.abs()).unwrap_or(Decimal::ZERO);
                
                // Get USD price for this side
                let quote_price_usd = if let Some(price) = tx.quote.price {
                    Decimal::try_from(price).unwrap_or(Decimal::ZERO)
                } else if let Some(nearest_price) = tx.quote.nearest_price {
                    Decimal::try_from(nearest_price).unwrap_or(Decimal::ZERO)
                } else {
                    debug!("No price data for quote side of {}", tx.tx_hash);
                    Decimal::ZERO
                };

                if quote_price_usd > Decimal::ZERO {
                    // Calculate actual SOL amount involved (only if quote token is SOL)
                    let sol_mint = "So11111111111111111111111111111111111111112";
                    let actual_sol_amount = if tx.quote.address == sol_mint {
                        quote_amount  // This IS SOL, so use the actual SOL quantity
                    } else {
                        Decimal::ZERO  // This is NOT SOL, so no SOL amount
                    };

                    let mut extra = HashMap::new();
                    extra.insert("token_symbol".to_string(), tx.quote.symbol.clone());
                    extra.insert("source".to_string(), tx.source.clone());
                    extra.insert("main_operation".to_string(), main_operation.to_string());
                    extra.insert("transfer_type".to_string(), tx.quote.transfer_type.clone().unwrap_or_default());

                    let quote_event = FinancialEvent {
                        id: Uuid::new_v4(),
                        transaction_id: tx.tx_hash.clone(),
                        wallet_address: wallet_address.to_string(),
                        event_type: if main_operation == "transfer" { EventType::TransferOut } else { EventType::Sell },
                        token_mint: tx.quote.address.clone(),
                        token_amount: quote_amount,
                        sol_amount: actual_sol_amount,  // FIXED: Contains actual SOL quantity, not USD value
                        usd_value: quote_amount * quote_price_usd,  // ADD: Store USD value separately  
                        timestamp,
                        transaction_fee: Decimal::ZERO,
                        metadata: EventMetadata {
                            program_id: Some(tx.address.clone()),
                            instruction_index: None,
                            exchange: Some(tx.source.clone()),
                            price_per_token: Some(quote_price_usd),
                            extra: extra.clone(),
                        },
                    };
                    
                    events.push(quote_event);
                }
            }

            // Process the "base" side (usually the token being bought/transferred in)
            if tx.base.type_swap == "to" && tx.base.ui_change_amount > 0.0 {
                let base_amount = Decimal::try_from(tx.base.ui_amount.abs()).unwrap_or(Decimal::ZERO);
                
                // Get USD price for this side
                let base_price_usd = if let Some(price) = tx.base.price {
                    Decimal::try_from(price).unwrap_or(Decimal::ZERO)
                } else if let Some(nearest_price) = tx.base.nearest_price {
                    Decimal::try_from(nearest_price).unwrap_or(Decimal::ZERO)
                } else {
                    debug!("No price data for base side of {}", tx.tx_hash);
                    Decimal::ZERO
                };

                if base_price_usd > Decimal::ZERO {
                    // Calculate actual SOL amount involved (only if base token is SOL)
                    let sol_mint = "So11111111111111111111111111111111111111112";
                    let actual_sol_amount = if tx.base.address == sol_mint {
                        base_amount  // This IS SOL, so use the actual SOL quantity
                    } else {
                        Decimal::ZERO  // This is NOT SOL, so no SOL amount
                    };

                    let mut extra = HashMap::new();
                    extra.insert("token_symbol".to_string(), tx.base.symbol.clone());
                    extra.insert("source".to_string(), tx.source.clone());
                    extra.insert("main_operation".to_string(), main_operation.to_string());
                    extra.insert("transfer_type".to_string(), tx.base.transfer_type.clone().unwrap_or_default());

                    let base_event = FinancialEvent {
                        id: Uuid::new_v4(),
                        transaction_id: tx.tx_hash.clone(),
                        wallet_address: wallet_address.to_string(),
                        event_type: if main_operation == "transfer" { EventType::TransferIn } else { EventType::Buy },
                        token_mint: tx.base.address.clone(),
                        token_amount: base_amount,
                        sol_amount: actual_sol_amount,  // FIXED: Contains actual SOL quantity, not USD value
                        usd_value: base_amount * base_price_usd,  // ADD: Store USD value separately
                        timestamp,
                        transaction_fee: Decimal::ZERO,
                        metadata: EventMetadata {
                            program_id: Some(tx.address.clone()),
                            instruction_index: None,
                            exchange: Some(tx.source.clone()),
                            price_per_token: Some(base_price_usd),
                            extra,
                        },
                    };
                    
                    events.push(base_event);
                }
            }
        }

        info!("✅ Converted {} general BirdEye transactions to {} financial events for wallet {}", 
              transactions.len(), events.len(), wallet_address);

        Ok(events)
    }

    /// Create P&L filters from system configuration
    fn create_pnl_filters_from_config(&self) -> PnLFilters {
        // Parse timeframe if configured
        let timeframe_filter = match self.config.pnl.timeframe_mode.as_str() {
            "general" => {
                if let Some(ref timeframe) = self.config.pnl.timeframe_general {
                    if let Ok(start_time) = pnl_core::timeframe::parse_general_timeframe(timeframe) {
                        Some(AnalysisTimeframe {
                            start_time: Some(start_time),
                            end_time: None,
                            mode: "general".to_string(),
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            "specific" => {
                if let Some(ref timeframe) = self.config.pnl.timeframe_specific {
                    if let Ok(start_time) = pnl_core::timeframe::parse_specific_timeframe(timeframe) {
                        Some(AnalysisTimeframe {
                            start_time: Some(start_time),
                            end_time: None,
                            mode: "specific".to_string(),
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        };

        PnLFilters {
            min_capital_sol: Decimal::from_f64_retain(self.config.pnl.wallet_min_capital).unwrap_or(Decimal::ZERO),
            min_hold_minutes: Decimal::from_f64_retain(self.config.pnl.aggregator_min_hold_minutes).unwrap_or(Decimal::ZERO),
            min_trades: self.config.pnl.amount_trades,
            min_win_rate: Decimal::from_f64_retain(self.config.pnl.win_rate).unwrap_or(Decimal::ZERO),
            max_signatures: Some(self.config.birdeye.default_max_transactions),
            max_transactions_to_fetch: Some(self.config.birdeye.default_max_transactions),
            timeframe_filter,
        }
    }

    /// Merge user-provided filters with config defaults, preserving user overrides
    fn merge_filters_with_config(&self, user_filters: Option<PnLFilters>) -> PnLFilters {
        let config_filters = self.create_pnl_filters_from_config();
        
        let mut effective = match user_filters {
            Some(mut user) => {
                // Only use config defaults for fields that user didn't explicitly set
                if user.min_capital_sol == Decimal::ZERO {
                    user.min_capital_sol = config_filters.min_capital_sol;
                }
                if user.min_hold_minutes == Decimal::ZERO {
                    user.min_hold_minutes = config_filters.min_hold_minutes;
                }
                if user.min_trades == 0 {
                    user.min_trades = config_filters.min_trades;
                }
                if user.min_win_rate == Decimal::ZERO {
                    user.min_win_rate = config_filters.min_win_rate;
                }
                if user.max_signatures.is_none() {
                    user.max_signatures = config_filters.max_signatures;
                }
                if user.max_transactions_to_fetch.is_none() {
                    user.max_transactions_to_fetch = config_filters.max_transactions_to_fetch;
                }
                if user.timeframe_filter.is_none() {
                    user.timeframe_filter = config_filters.timeframe_filter;
                }
                user
            }
            None => config_filters,
        };

        // Validate and fix parameter conflicts
        self.validate_and_fix_filters(&mut effective);
        
        effective
    }

    /// Validate filter parameters and fix conflicts
    fn validate_and_fix_filters(&self, filters: &mut PnLFilters) {
        // Fix conflict: max_signatures cannot exceed max_transactions_to_fetch
        if let (Some(fetch_limit), Some(sig_limit)) = (filters.max_transactions_to_fetch, filters.max_signatures) {
            if sig_limit > fetch_limit {
                warn!("max_signatures ({}) > max_transactions_to_fetch ({}), adjusting max_signatures to match fetch limit", 
                      sig_limit, fetch_limit);
                filters.max_signatures = Some(fetch_limit);
            }
        }

        // Validate timeframe bounds
        if let Some(ref timeframe) = filters.timeframe_filter {
            if let (Some(start), Some(end)) = (timeframe.start_time, timeframe.end_time) {
                if start >= end {
                    warn!("Invalid timeframe: start_time ({}) >= end_time ({}), clearing end_time", 
                          start, end);
                    // Fix by clearing end_time to make it "from start_time to now"
                    if let Some(ref mut tf) = filters.timeframe_filter {
                        tf.end_time = None;
                    }
                }
            }
        }

        // Validate numeric ranges
        if filters.min_win_rate > Decimal::from(100) {
            warn!("min_win_rate ({}) > 100%, capping at 100%", filters.min_win_rate);
            filters.min_win_rate = Decimal::from(100);
        }

        if filters.min_win_rate < Decimal::ZERO {
            warn!("min_win_rate ({}) < 0%, setting to 0%", filters.min_win_rate);
            filters.min_win_rate = Decimal::ZERO;
        }
    }

    /// Extract time bounds from PnL filters for BirdEye API calls
    /// Returns (from_time, to_time) as Unix timestamps in seconds
    fn extract_time_bounds_for_birdeye(filters: &PnLFilters) -> (Option<i64>, Option<i64>) {
        match &filters.timeframe_filter {
            Some(timeframe) => {
                let from_time = timeframe.start_time.map(|dt| dt.timestamp());
                let to_time = timeframe.end_time.map(|dt| dt.timestamp());
                debug!("Extracted time bounds for BirdEye API: from_time={:?}, to_time={:?}", from_time, to_time);
                (from_time, to_time)
            }
            None => {
                debug!("No timeframe filter specified, fetching all transactions");
                (None, None)
            }
        }
    }

    /// Set runtime filters for continuous mode
    pub async fn set_continuous_mode_filters(&self, filters: Option<PnLFilters>) {
        let mut continuous_filters = self.continuous_mode_filters.lock().await;
        *continuous_filters = filters;
    }

    /// Get effective filters for continuous mode (user override or config default)
    async fn get_effective_continuous_filters(&self) -> PnLFilters {
        let continuous_filters = self.continuous_mode_filters.lock().await;
        self.merge_filters_with_config(continuous_filters.clone())
    }

    /// Get system status
    pub async fn get_status(&self) -> Result<OrchestratorStatus> {
        // Try to get wallet-token pairs queue size with timeout, fallback to 0 if Redis unavailable
        let queue_size = {
            use tokio::time::{timeout, Duration};
            match timeout(Duration::from_millis(1000), async {
                let redis = self.redis_client.lock().await;
                redis.get_wallet_token_pairs_queue_size().await
            }).await {
                Ok(Ok(size)) => size,
                Ok(Err(_)) => {
                    warn!("Redis unavailable for queue size check");
                    0
                },
                Err(_) => {
                    warn!("Redis queue size check timed out");
                    0
                }
            }
        };

        let running_jobs_count = self.running_jobs.lock().await.len();
        
        // Get batch jobs count from Redis
        let batch_jobs_count = {
            use tokio::time::{timeout, Duration as TokioDuration};
            match timeout(TokioDuration::from_millis(1000), async {
                let redis = self.redis_client.lock().await;
                redis.get_batch_job_stats().await
            }).await {
                Ok(Ok(stats)) => stats.total_jobs,
                Ok(Err(_)) => {
                    warn!("Redis unavailable for batch jobs count");
                    0
                },
                Err(_) => {
                    warn!("Redis batch jobs count check timed out");
                    0
                }
            }
        };

        Ok(OrchestratorStatus {
            discovery_queue_size: queue_size,
            running_jobs_count: running_jobs_count as u64,
            batch_jobs_count: batch_jobs_count as u64,
            is_continuous_mode: self.config.system.redis_mode,
        })
    }

    /// Clear temporary data
    pub async fn clear_temp_data(&self) -> Result<()> {
        let redis = self.redis_client.lock().await;
        redis.clear_temp_data().await?;
        info!("Cleared temporary Redis data");
        Ok(())
    }
}

// Clone implementation for JobOrchestrator
impl Clone for JobOrchestrator {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            birdeye_client: self.birdeye_client.clone(),
            redis_client: self.redis_client.clone(),
            running_jobs: self.running_jobs.clone(),
            continuous_mode_filters: self.continuous_mode_filters.clone(),
        }
    }
}

/// System status information
#[derive(Debug, Serialize, Deserialize)]
pub struct OrchestratorStatus {
    pub discovery_queue_size: u64,
    pub running_jobs_count: u64,
    pub batch_jobs_count: u64,
    pub is_continuous_mode: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pnl_job_creation() {
        let wallet = "test_wallet".to_string();
        let filters = PnLFilters {
            min_capital_sol: Decimal::ZERO,
            min_hold_minutes: Decimal::ZERO,
            min_trades: 1,
            min_win_rate: Decimal::ZERO,
            max_signatures: None,
            timeframe_filter: None,
        };

        let job = PnLJob::new(wallet.clone(), filters);
        assert_eq!(job.wallet_address, wallet);
        assert_eq!(job.status, JobStatus::Pending);
    }

    #[test]
    fn test_batch_job_creation() {
        let wallets = vec!["wallet1".to_string(), "wallet2".to_string()];
        let filters = PnLFilters {
            min_capital_sol: Decimal::ZERO,
            min_hold_minutes: Decimal::ZERO,
            min_trades: 1,
            min_win_rate: Decimal::ZERO,
            max_signatures: None,
            timeframe_filter: None,
        };

        let batch_job = BatchJob::new(wallets.clone(), filters);
        assert_eq!(batch_job.wallet_addresses, wallets);
        assert_eq!(batch_job.status, JobStatus::Pending);
    }
}