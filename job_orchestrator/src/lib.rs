use chrono::Utc;
use config_manager::SystemConfig;
use futures::future::join_all;
use dex_client::{
    BirdEyeClient, BirdEyeError, GeneralTraderTransaction, TokenTransactionSide, 
    PriceEnricher, EnrichedTransaction, PriceStrategy,
    // Portfolio API imports
    extract_current_prices_from_portfolio
};
use goldrush_client::{
    GoldRushClient, GoldRushError, GoldRushChain, GoldRushTransaction,
    GoldRushEventConverter, TokenTransfer, LogEvent,
};
use persistence_layer::{PersistenceError, PersistenceClient, DiscoveredWalletToken, TokenAnalysisJob, JobStatus};
// New algorithm imports (primary P&L system)
use pnl_core::{NewTransactionParser, NewPnLEngine, PortfolioPnLResult, NewFinancialEvent, BalanceFetcher, HistoryTransactionParser};
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

pub mod birdeye_trending_orchestrator;

pub use birdeye_trending_orchestrator::{BirdEyeTrendingOrchestrator, DiscoveryStats, ProcessedSwap};

#[derive(Error, Debug, Clone)]
pub enum OrchestratorError {
    #[error("Persistence error: {0}")]
    Persistence(String),
    #[error("P&L calculation error: {0}")]
    PnL(String),
    #[error("BirdEye price client error: {0}")]
    BirdEyePrice(String),
    #[error("GoldRush client error: {0}")]
    GoldRush(String),
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

impl From<GoldRushError> for OrchestratorError {
    fn from(err: GoldRushError) -> Self {
        OrchestratorError::GoldRush(err.to_string())
    }
}



impl From<config_manager::ConfigurationError> for OrchestratorError {
    fn from(err: config_manager::ConfigurationError) -> Self {
        OrchestratorError::Config(err.to_string())
    }
}

impl From<String> for OrchestratorError {
    fn from(err: String) -> Self {
        OrchestratorError::PnL(err)
    }
}

pub type Result<T> = std::result::Result<T, OrchestratorError>;

/// P&L analysis job information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PnLJob {
    pub id: Uuid,
    pub wallet_address: String,
    pub chain: String, // "solana", "ethereum", "base", "bsc"
    pub status: JobStatus,
    pub created_at: chrono::DateTime<Utc>,
    pub started_at: Option<chrono::DateTime<Utc>>,
    pub completed_at: Option<chrono::DateTime<Utc>>,
    pub error_message: Option<String>,
    pub max_transactions: Option<u32>, // Simple filter - max transactions to fetch
    pub result: Option<PortfolioPnLResult>,
}

impl PnLJob {
    pub fn new(wallet_address: String, chain: String, max_transactions: Option<u32>) -> Self {
        Self {
            id: Uuid::new_v4(),
            wallet_address,
            chain,
            status: JobStatus::Pending,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            error_message: None,
            max_transactions,
            result: None,
        }
    }
}

/// Batch P&L analysis job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchJob {
    pub id: Uuid,
    pub wallet_addresses: Vec<String>,
    pub chain: String,
    pub status: JobStatus,
    pub created_at: chrono::DateTime<Utc>,
    pub started_at: Option<chrono::DateTime<Utc>>,
    pub completed_at: Option<chrono::DateTime<Utc>>,
    pub filters: serde_json::Value, // Store max_transactions and other params as JSON
    pub individual_jobs: Vec<Uuid>,
    // Results stored in PostgreSQL, not in memory
}

impl BatchJob {
    pub fn new(wallet_addresses: Vec<String>, chain: String, max_transactions: Option<u32>) -> Self {
        // Store max_transactions in filters JSON for PostgreSQL storage
        let filters = serde_json::json!({
            "max_transactions": max_transactions
        });
        
        Self {
            id: Uuid::new_v4(),
            wallet_addresses,
            chain,
            status: JobStatus::Pending,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            filters,
            individual_jobs: Vec::new(),
        }
    }
    
    /// Get max_transactions from filters JSON
    pub fn get_max_transactions(&self) -> Option<u32> {
        self.filters.get("max_transactions")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
    }
    
    /// Convert to persistence layer BatchJob format
    pub fn to_persistence_batch_job(&self) -> Result<persistence_layer::BatchJob> {
        Ok(persistence_layer::BatchJob {
            id: self.id,
            wallet_addresses: self.wallet_addresses.clone(),
            chain: self.chain.clone(),
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
            filters: self.filters.clone(),
            individual_jobs: self.individual_jobs.clone(),
        })
    }
    
    /// Create from persistence layer BatchJob format
    pub fn from_persistence_batch_job(persistent_job: persistence_layer::BatchJob) -> Result<Self> {
        Ok(Self {
            id: persistent_job.id,
            wallet_addresses: persistent_job.wallet_addresses,
            chain: persistent_job.chain,
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
            filters: persistent_job.filters,
            individual_jobs: persistent_job.individual_jobs,
        })
    }


}

/// Job orchestrator for managing P&L analysis tasks
pub struct JobOrchestrator {
    config: SystemConfig,
    birdeye_client: BirdEyeClient,
    goldrush_client: Option<GoldRushClient>,
    persistence_client: Arc<PersistenceClient>,
    running_jobs: Arc<Mutex<HashMap<Uuid, PnLJob>>>,
    running_token_analysis_jobs: Arc<Mutex<HashMap<Uuid, TokenAnalysisJob>>>,
    // batch_jobs stored in PostgreSQL via persistence_client
    instance_id: String, // Unique identifier for this orchestrator instance
}

impl JobOrchestrator {
    pub async fn new(config: SystemConfig, persistence_client: Arc<PersistenceClient>) -> Result<Self> {
        // Initialize BirdEye client
        let birdeye_config = config.birdeye.clone();
        let birdeye_client = BirdEyeClient::new(birdeye_config.clone())?;

        // Initialize GoldRush client if enabled
        let goldrush_client = if config.goldrush.enabled {
            match GoldRushClient::with_config(goldrush_client::GoldRushConfig {
                api_key: config.goldrush.api_key.clone(),
                base_url: config.goldrush.api_base_url.clone(),
                timeout_seconds: config.goldrush.request_timeout_seconds,
            }) {
                Ok(client) => {
                    info!("GoldRush client initialized for EVM chains");
                    Some(client)
                }
                Err(e) => {
                    warn!("Failed to initialize GoldRush client: {}", e);
                    None
                }
            }
        } else {
            info!("GoldRush client disabled in configuration");
            None
        };

        // Generate unique instance ID using hostname and process ID
        let instance_id = format!("{}:{}:{}", 
            hostname::get().unwrap_or_default().to_string_lossy(),
            std::process::id(),
            Uuid::new_v4().to_string()[..8].to_string() // First 8 chars of UUID for uniqueness
        );

        info!("Job orchestrator instance ID: {}", instance_id);

        Ok(Self {
            config,
            birdeye_client,
            goldrush_client,
            persistence_client,
            running_jobs: Arc::new(Mutex::new(HashMap::new())),
            running_token_analysis_jobs: Arc::new(Mutex::new(HashMap::new())),
            instance_id,
        })
    }

    /// Get effective continuous mode filters
    async fn get_effective_continuous_filters(&self) -> Option<u32> {
        // Simple max_transactions filter for continuous mode
        Some(self.config.birdeye.default_max_transactions)
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

    /// Run a single continuous mode cycle using work-stealing pattern
    async fn run_continuous_cycle(&self) -> Result<()> {
        // Cleanup stale processing locks from dead instances (every cycle)
        {
            let redis = &self.persistence_client;
            if let Err(e) = redis.cleanup_stale_processing_locks(600).await { // 10 minutes max age
                warn!("Failed to cleanup stale processing locks: {}", e);
            }
        }

        // Claim a batch of work for this instance
        let batch_size = self.config.system.pnl_parallel_batch_size.unwrap_or(10);
        let (claimed_batch, batch_id) = {
            let redis = &self.persistence_client;
            redis.claim_wallet_batch(&self.instance_id, batch_size).await?
        };

        if claimed_batch.is_empty() {
            debug!("No work available for instance {}", self.instance_id);
            return Ok(());
        }

        info!("Instance {} claimed batch {} with {} wallet-token pairs", 
              self.instance_id, batch_id, claimed_batch.len());

        // Process the claimed batch
        let result = self.process_claimed_batch(&claimed_batch, &batch_id).await;

        // Handle the result - release or return failed items
        match result {
            Ok(_) => {
                // Successfully processed all items, release the claim
                let redis = &self.persistence_client;
                if let Err(e) = redis.release_batch_claim(&batch_id).await {
                    warn!("Failed to release batch claim {}: {}", batch_id, e);
                }
            }
            Err(e) => {
                // Some items failed, return them to the queue
                warn!("Batch {} processing failed: {}", batch_id, e);
                let redis = &self.persistence_client;
                if let Err(return_err) = redis.return_failed_batch(&batch_id, &claimed_batch).await {
                    error!("Failed to return failed batch {} to queue: {}", batch_id, return_err);
                }
            }
        }

        Ok(())
    }

    /// Process a claimed batch of wallet-token pairs
    async fn process_claimed_batch(&self, claimed_batch: &[DiscoveredWalletToken], batch_id: &str) -> Result<()> {
        info!("🚀 Processing claimed batch {} with {} wallet-token pairs in parallel...", 
              batch_id, claimed_batch.len());

        // Get effective P&L filters (user override or config default)
        let filters = self.get_effective_continuous_filters().await;

        // Process wallet-token pairs in parallel with timeout
        let futures = claimed_batch.iter().map(|pair| {
            let filters = filters.clone();
            let pair_clone = pair.clone();
            async move {
                info!("Processing claimed wallet-token pair: {} for {} ({})", 
                      pair_clone.wallet_address, pair_clone.token_symbol, pair_clone.token_address);
                
                // Add timeout for each wallet processing (5 minutes max for queue processing)
                let timeout_duration = Duration::from_secs(300);
                let result = match tokio::time::timeout(
                    timeout_duration, 
                    self.process_single_wallet_token_pair(&pair_clone, filters)
                ).await {
                    Ok(Ok(report)) => {
                        // Store the rich P&L portfolio result for later retrieval
                        let store_result = {
                            let redis = &self.persistence_client;
                            redis.store_pnl_result_with_source(
                                &pair_clone.wallet_address,
                                &pair_clone.chain,
                                &report,
                                "continuous",
                            ).await
                        };
                        
                        match store_result {
                            Ok(_) => {
                                // Mark wallet as successfully processed for this chain
                                let redis = &self.persistence_client;
                                if let Err(e) = redis.mark_wallet_as_processed_for_chain(&pair_clone.wallet_address, &pair_clone.chain).await {
                                    warn!("Failed to mark wallet {} as processed for chain {}: {}", pair_clone.wallet_address, pair_clone.chain, e);
                                } else {
                                    debug!("Marked wallet {} as processed for chain {} and stored P&L result", pair_clone.wallet_address, pair_clone.chain);
                                }
                                Ok(report)
                            }
                            Err(e) => {
                                // Mark wallet as failed for this chain if storage fails
                                let redis = &self.persistence_client;
                                if let Err(mark_err) = redis.mark_wallet_as_failed_for_chain(&pair_clone.wallet_address, &pair_clone.chain).await {
                                    warn!("Failed to mark wallet {} as failed for chain {}: {}", pair_clone.wallet_address, pair_clone.chain, mark_err);
                                }
                                warn!("Failed to store P&L result for wallet {}: {}", pair_clone.wallet_address, e);
                                Err(anyhow::anyhow!("{}", e))
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        // P&L calculation failed
                        warn!("P&L calculation failed for wallet {} token {}: {}", 
                              pair_clone.wallet_address, pair_clone.token_symbol, e);
                        let redis = &self.persistence_client;
                        if let Err(mark_err) = redis.mark_wallet_as_failed_for_chain(&pair_clone.wallet_address, &pair_clone.chain).await {
                            warn!("Failed to mark wallet {} as failed for chain {}: {}", pair_clone.wallet_address, pair_clone.chain, mark_err);
                        }
                        Err(anyhow::anyhow!("{}", e))
                    }
                    Err(_) => {
                        // Timeout occurred
                        let timeout_err = anyhow::anyhow!("Processing timeout for wallet {} token {}", 
                                                        pair_clone.wallet_address, pair_clone.token_symbol);
                        warn!("{}", timeout_err);
                        let redis = &self.persistence_client;
                        if let Err(mark_err) = redis.mark_wallet_as_failed_for_chain(&pair_clone.wallet_address, &pair_clone.chain).await {
                            warn!("Failed to mark wallet {} as failed for chain {}: {}", pair_clone.wallet_address, pair_clone.chain, mark_err);
                        }
                        Err(timeout_err)
                    }
                };
                
                (pair_clone, result)
            }
        });

        // Wait for all to complete
        let results = join_all(futures).await;
        
        // Count successes and failures
        let mut success_count = 0;
        let mut failure_count = 0;
        
        for (pair, result) in results {
            match result {
                Ok(report) => {
                    success_count += 1;
                    info!("✅ Successfully processed wallet {} for token {}: P&L = {} USD", 
                          pair.wallet_address, pair.token_symbol, report.total_pnl_usd);
                }
                Err(e) => {
                    failure_count += 1;
                    error!("❌ Failed to process wallet {} for token {}: {}", 
                           pair.wallet_address, pair.token_symbol, e);
                }
            }
        }

        info!("Batch {} completed: {} successes, {} failures", 
              batch_id, success_count, failure_count);

        // Return success if at least some items were processed successfully
        if success_count > 0 {
            Ok(())
        } else {
            Err(anyhow::anyhow!("All items in batch {} failed", batch_id).into())
        }
    }

    /// Run a single continuous mode cycle for testing (returns true if processed a pair)
    pub async fn start_continuous_mode_single_cycle(&self) -> Result<bool> {
        // Use work-stealing pattern to claim a single wallet-token pair
        let (claimed_batch, batch_id) = {
            let redis = &self.persistence_client;
            redis.claim_wallet_batch(&self.instance_id, 1).await?
        };

        if claimed_batch.is_empty() {
            debug!("No work available for test cycle on instance {}", self.instance_id);
            return Ok(false);
        }

        debug!("Test cycle claimed batch {} with {} wallet-token pair", 
               batch_id, claimed_batch.len());

        // Process the single claimed item
        let result = self.process_claimed_batch(&claimed_batch, &batch_id).await;

        // Handle the result - release or return failed items
        match result {
            Ok(_) => {
                // Successfully processed, release the claim
                let redis = &self.persistence_client;
                if let Err(e) = redis.release_batch_claim(&batch_id).await {
                    warn!("Failed to release test batch claim {}: {}", batch_id, e);
                }
                Ok(true)
            }
            Err(e) => {
                // Failed, return items to queue
                warn!("Test batch {} processing failed: {}", batch_id, e);
                let redis = &self.persistence_client;
                if let Err(return_err) = redis.return_failed_batch(&batch_id, &claimed_batch).await {
                    error!("Failed to return failed test batch {} to queue: {}", batch_id, return_err);
                }
                Err(e)
            }
        }
    }

    /// Submit a batch P&L job
    pub async fn submit_batch_job(
        &self,
        wallet_addresses: Vec<String>,
        chain: String,
        max_transactions: Option<u32>,
    ) -> Result<Uuid> {
        let batch_job = BatchJob::new(wallet_addresses.clone(), chain, max_transactions);
        let job_id = batch_job.id;

        // Store batch job in PostgreSQL
        {
            let persistence_client = &self.persistence_client;
            let persistent_job = batch_job.to_persistence_batch_job()?;
            persistence_client.store_batch_job(&persistent_job).await?;
        }

        // Process in background
        let orchestrator = self.clone();
        tokio::spawn(async move {
            if let Err(e) = orchestrator.execute_batch_job(job_id).await {
                error!("Batch job {} failed with system error: {}", job_id, e);
                
                // Mark job as Failed due to system-level error
                if let Err(update_err) = orchestrator.mark_batch_job_as_failed(job_id, &e.to_string()).await {
                    error!("Failed to update batch job {} status to Failed: {}", job_id, update_err);
                }
            }
        });

        info!("Submitted batch job {} for {} wallets", job_id, wallet_addresses.len());

        Ok(job_id)
    }

    /// Execute a batch job
    async fn execute_batch_job(&self, job_id: Uuid) -> Result<()> {
        let start_time = std::time::Instant::now();
        info!("🚀 Starting batch job execution: {}", job_id);
        
        // Load job from PostgreSQL and update status to Running
        info!("📋 Loading batch job details from database...");
        let (wallet_addresses, chain, max_transactions) = {
            let persistence_client = &self.persistence_client;
            let persistent_job = persistence_client.get_batch_job(&job_id.to_string()).await?
                .ok_or_else(|| OrchestratorError::JobExecution(format!("Batch job {} not found", job_id)))?;
            
            let mut job = BatchJob::from_persistence_batch_job(persistent_job)?;
            info!("✅ Job loaded successfully - Chain: {}, Wallets: {}, Max Transactions: {:?}", 
                  job.chain, job.wallet_addresses.len(), job.get_max_transactions());
            
            job.status = JobStatus::Running;
            job.started_at = Some(Utc::now());
            info!("🔄 Updating job status to Running...");

            // Update status in PostgreSQL
            let updated_persistent_job = job.to_persistence_batch_job()?;
            persistence_client.update_batch_job(&updated_persistent_job).await?;
            info!("✅ Job status updated in database");

            (job.wallet_addresses.clone(), job.chain.clone(), job.get_max_transactions())
        };

        info!("🎯 Starting batch job execution: {} for {} wallets on {} chain", 
              job_id, wallet_addresses.len(), chain);

        // Process wallets in parallel with timeout
        let wallet_count = wallet_addresses.len();
        info!("⚡ Starting parallel processing of {} wallets (timeout: 10 minutes per wallet)...", wallet_count);
        let futures = wallet_addresses.iter().enumerate().map(|(index, wallet)| {
            let wallet_clone = wallet.clone();
            let chain_clone = chain.clone();
            async move {
                let wallet_start_time = std::time::Instant::now();
                info!("🔄 Processing wallet {}/{}: {} on {}", index + 1, wallet_count, wallet_clone, chain_clone);
                
                // Add timeout for each wallet processing (10 minutes max)
                let timeout_duration = Duration::from_secs(600);
                let result = match tokio::time::timeout(
                    timeout_duration, 
                    self.process_single_wallet(&wallet_clone, &chain_clone, max_transactions)
                ).await {
                    Ok(Ok(report)) => {
                        let elapsed = wallet_start_time.elapsed();
                        info!("✅ Wallet {}/{} completed successfully in {:.2}s: {} (Realized P&L: ${:.2}, Unrealized P&L: ${:.2})", 
                              index + 1, wallet_count, elapsed.as_secs_f64(), wallet_clone,
                              report.total_realized_pnl_usd, report.total_unrealized_pnl_usd);
                        Ok(report)
                    },
                    Ok(Err(e)) => {
                        let elapsed = wallet_start_time.elapsed();
                        warn!("❌ Wallet {}/{} failed in {:.2}s: {} - Error: {}", 
                              index + 1, wallet_count, elapsed.as_secs_f64(), wallet_clone, e);
                        Err(e)
                    },
                    Err(_) => {
                        warn!("⏰ Wallet {}/{} timed out after {} seconds: {}", 
                              index + 1, wallet_count, timeout_duration.as_secs(), wallet_clone);
                        Err(OrchestratorError::JobExecution(
                            format!("Wallet processing timed out after {} seconds", timeout_duration.as_secs())
                        ))
                    },
                };
                (wallet_clone, result)
            }
        });

        info!("⏳ Waiting for all {} wallet processing tasks to complete...", wallet_count);
        let results = join_all(futures).await;
        let processing_elapsed = start_time.elapsed();
        info!("🏁 All wallet processing completed in {:.2}s", processing_elapsed.as_secs_f64());

        // Store individual P&L results in main PostgreSQL table and batch results
        info!("💾 Storing P&L results to database...");
        let successful_count = {
            let mut success_count = 0;
            let total_results = results.len();
            let successful_results = results.iter().filter(|(_, r)| r.is_ok()).count();
            info!("📊 Processing {} results ({} successful, {} failed)", 
                  total_results, successful_results, total_results - successful_results);
            
            // Store each successful wallet's rich P&L result in main pnl_results table
            for (wallet, result) in &results {
                if let Ok(portfolio_result) = result {
                    // Store rich portfolio result in main table for individual wallet queries
                    let persistence_client = &self.persistence_client;
                    // Use the chain from the batch job
                    info!("💾 Storing result for wallet: {} (Realized: ${:.2}, Unrealized: ${:.2}, {} tokens)", 
                          wallet, portfolio_result.total_realized_pnl_usd, 
                          portfolio_result.total_unrealized_pnl_usd, portfolio_result.token_results.len());
                    match persistence_client.store_pnl_result_with_source(wallet, &chain, portfolio_result, "batch").await {
                        Ok(_) => {
                            info!("✅ Successfully stored P&L result for wallet {} from batch job {}", wallet, job_id);
                            success_count += 1;
                        }
                        Err(e) => {
                            warn!("❌ Failed to store P&L result for wallet {} from batch job {}: {}", wallet, job_id, e);
                        }
                    }
                } else if let Err(e) = result {
                    warn!("⚠️ Skipping storage for failed wallet {}: {}", wallet, e);
                }
            }
            
            // Update batch job status to completed in PostgreSQL
            info!("🔄 Updating batch job status to Completed...");
            let persistence_client = &self.persistence_client;
            let persistent_job = persistence_client.get_batch_job(&job_id.to_string()).await?
                .ok_or_else(|| OrchestratorError::JobExecution(format!("Batch job {} not found", job_id)))?;
            
            let mut job = BatchJob::from_persistence_batch_job(persistent_job)?;
            job.status = JobStatus::Completed;
            job.completed_at = Some(Utc::now());
            info!("✅ Job completion time set: {}", job.completed_at.unwrap());

            // Note: Individual P&L results are already stored in pnl_results table above
            // No need to store results in batch job - they're retrieved from PostgreSQL when needed

            // Update final status
            let updated_persistent_job = job.to_persistence_batch_job()?;
            persistence_client.update_batch_job(&updated_persistent_job).await?;
            info!("✅ Batch job status updated to Completed in database");

            success_count
        };

        let total_elapsed = start_time.elapsed();
        info!("🎉 Batch job {} completed successfully in {:.2}s: {}/{} wallets successful ({:.1}% success rate)", 
              job_id, total_elapsed.as_secs_f64(), successful_count, wallet_count, 
              (successful_count as f64 / wallet_count as f64) * 100.0);

        if successful_count < wallet_count {
            let failed_count = wallet_count - successful_count;
            warn!("⚠️ {} out of {} wallets failed processing", failed_count, wallet_count);
        }

        Ok(())
    }

    /// Get batch job status
    pub async fn get_batch_job_status(&self, job_id: Uuid) -> Option<BatchJob> {
        let redis = &self.persistence_client;
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

    /// Get batch job results from PostgreSQL using wallet addresses from batch job
    pub async fn get_batch_job_results(&self, job_id: &str) -> Result<Vec<PortfolioPnLResult>> {
        let persistence = &self.persistence_client;
        
        // First get the batch job to get the wallet addresses
        match persistence.get_batch_job(job_id).await? {
            Some(batch_job) => {
                let mut results = Vec::new();
                
                // Fetch P&L results for each wallet in the batch
                for wallet_address in &batch_job.wallet_addresses {
                    match persistence.get_portfolio_pnl_result(wallet_address, &batch_job.chain).await? {
                        Some(stored_result) => {
                            results.push(stored_result.portfolio_result);
                        }
                        None => {
                            debug!("No P&L result found for wallet {} in batch {}", wallet_address, job_id);
                        }
                    }
                }
                
                Ok(results)
            }
            None => {
                warn!("Batch job {} not found", job_id);
                Ok(Vec::new())
            }
        }
    }

    /// Mark a batch job as failed due to system-level error
    async fn mark_batch_job_as_failed(&self, job_id: Uuid, error_message: &str) -> Result<()> {
        let redis = &self.persistence_client;
        
        // Try to get the job and update its status to Failed
        if let Some(persistent_job) = redis.get_batch_job(&job_id.to_string()).await? {
            let mut job = BatchJob::from_persistence_batch_job(persistent_job)?;
            job.status = JobStatus::Failed;
            job.completed_at = Some(Utc::now());
            
            // Update the job in database
            let updated_persistent_job = job.to_persistence_batch_job()?;
            redis.update_batch_job(&updated_persistent_job).await?;
            
            info!("Marked batch job {} as Failed due to system error: {}", job_id, error_message);
        } else {
            warn!("Could not find batch job {} to mark as failed", job_id);
        }
        
        Ok(())
    }

    /// Get all batch jobs with pagination
    pub async fn get_all_batch_jobs(&self, limit: usize, offset: usize) -> Result<(Vec<BatchJob>, usize)> {
        let redis = &self.persistence_client;
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
        max_transactions: Option<u32>,
    ) -> Result<PortfolioPnLResult> {
        debug!("Starting targeted P&L analysis for wallet: {} on token: {} ({})", 
               pair.wallet_address, pair.token_symbol, pair.token_address);

        // Use BirdEye for token-pair P&L analysis
        debug!("Using BirdEye for token-pair P&L analysis of wallet: {}", pair.wallet_address);
        let transactions = self.process_wallet_token_pair_with_birdeye(pair, max_transactions).await?;

        if transactions.is_empty() {
            return Err(OrchestratorError::JobExecution(format!(
                "No transactions found for wallet: {} on token: {}",
                pair.wallet_address, pair.token_symbol
            )));
        }

        // Calculate P&L using the new algorithm (for continuous analysis)
        let report = self.calculate_pnl_with_new_algorithm(&pair.wallet_address, &pair.chain, transactions).await?;

        debug!("✅ Targeted P&L analysis completed for wallet: {} on token: {}", 
               pair.wallet_address, pair.token_symbol);

        Ok(report)
    }

    /// Process a single wallet for P&L analysis (using rich PortfolioPnLResult format)
    pub async fn process_single_wallet(
        &self,
        wallet_address: &str,
        chain: &str,
        max_transactions: Option<u32>,
    ) -> Result<PortfolioPnLResult> {
        info!("🔍 Starting P&L analysis for wallet: {} on chain: {} (max_txs: {:?})", 
              wallet_address, chain, max_transactions);
              
        // Route to appropriate client based on chain
        let result = match chain.to_lowercase().as_str() {
            "solana" => {
                info!("📊 Using BirdEye for Solana P&L analysis of wallet: {}", wallet_address);
                self.process_single_wallet_with_birdeye(wallet_address, chain, max_transactions).await
            }
            "ethereum" | "base" | "bsc" => {
                info!("⛓️ Using GoldRush for {} P&L analysis of wallet: {}", chain, wallet_address);
                self.process_single_wallet_with_goldrush(wallet_address, chain, max_transactions).await
            }
            _ => {
                error!("❌ Unsupported chain: {}. Supported chains: solana, ethereum, base, bsc", chain);
                return Err(OrchestratorError::Config(format!("Unsupported chain: {}", chain)));
            }
        };
        
        match &result {
            Ok(portfolio) => {
                info!("✅ P&L analysis completed for wallet: {} - Realized: ${:.2}, Unrealized: ${:.2}, {} tokens", 
                      wallet_address, portfolio.total_realized_pnl_usd, 
                      portfolio.total_unrealized_pnl_usd, portfolio.token_results.len());
            }
            Err(e) => {
                warn!("❌ P&L analysis failed for wallet: {} - Error: {}", wallet_address, e);
            }
        }
        
        result
    }

    /// Process a single wallet using BirdEye transaction history API (captures swaps AND sends)
    async fn process_single_wallet_with_birdeye(
        &self,
        wallet_address: &str,
        chain: &str,
        max_transactions: Option<u32>,
    ) -> Result<PortfolioPnLResult> {
        let step_start = std::time::Instant::now();
        info!("🐦 Starting BirdEye P&L analysis for wallet: {} on chain: {}", wallet_address, chain);

        let max_total_transactions = max_transactions
            .unwrap_or(self.config.birdeye.default_max_transactions);
        info!("📋 Configuration: Max transactions = {}, BirdEye API timeout = {}s", 
              max_total_transactions, self.config.birdeye.request_timeout_seconds);
        
        // Step 1: Fetch transaction history (includes both swaps AND sends)
        info!("📥 Step 1/4: Fetching transaction history from BirdEye API...");
        info!("🔄 Requesting up to {} transaction history entries for wallet {}", 
               max_total_transactions, wallet_address);
        
        let history_start = std::time::Instant::now();
        let history_transactions = self
            .birdeye_client
            .get_wallet_transaction_history(wallet_address, Some(chain), Some(max_total_transactions))
            .await?;
        let history_elapsed = history_start.elapsed();

        if history_transactions.is_empty() {
            warn!("❌ No transaction history found for wallet: {}", wallet_address);
            return Err(OrchestratorError::JobExecution(format!(
                "No transaction history found for wallet: {}",
                wallet_address
            )));
        }

        info!("✅ Step 1 completed in {:.2}s: Found {} transaction history entries for wallet {}", 
              history_elapsed.as_secs_f64(), history_transactions.len(), wallet_address);

        // Step 2: Enrich transactions with price data
        info!("💰 Step 2/4: Enriching {} transactions with historical price data...", history_transactions.len());
        info!("🔧 Using PriceStrategy::Historical for accurate historical pricing (no fallbacks)");
        
        let enrich_start = std::time::Instant::now();
        let birdeye_client_for_enricher = BirdEyeClient::new(self.config.birdeye.clone())?;
        let mut price_enricher = PriceEnricher::new(birdeye_client_for_enricher);
        
        let enriched_transactions = price_enricher
            .enrich_transactions_batch(history_transactions, PriceStrategy::Historical)
            .await?;
        let enrich_elapsed = enrich_start.elapsed();

        if enriched_transactions.is_empty() {
            warn!("❌ No enriched transactions after price resolution for wallet: {}", wallet_address);
            return Err(OrchestratorError::JobExecution(format!(
                "No enriched transactions after price resolution for wallet: {}",
                wallet_address
            )));
        }

        let successfully_enriched = enriched_transactions.iter().filter(|t| t.price_resolution_complete).count();
        info!("✅ Step 2 completed in {:.2}s: {}/{} transactions successfully enriched ({:.1}% success rate)", 
              enrich_elapsed.as_secs_f64(), successfully_enriched, enriched_transactions.len(),
              (successfully_enriched as f64 / enriched_transactions.len() as f64) * 100.0);

        // Step 3: Fetch current portfolio for accurate unrealized P&L calculation
        info!("📈 Step 3/4: Fetching current portfolio for real-time token prices...");
        info!("🔄 Requesting current wallet portfolio from BirdEye API for wallet {}", wallet_address);
        
        let portfolio_start = std::time::Instant::now();
        let current_portfolio = self
            .birdeye_client
            .get_wallet_portfolio(wallet_address, Some(chain))
            .await?;
        let portfolio_elapsed = portfolio_start.elapsed();

        let portfolio_total_value = current_portfolio.iter().map(|t| t.value_usd).sum::<f64>();
        info!("✅ Step 3 completed in {:.2}s: Current portfolio fetched - {} tokens with total value ${:.2}", 
              portfolio_elapsed.as_secs_f64(), current_portfolio.len(), portfolio_total_value);

        // Extract current prices from portfolio for accurate unrealized P&L
        info!("🔍 Processing current portfolio holdings for unrealized P&L...");
        if !current_portfolio.is_empty() {
            info!("📋 Current Holdings Detail:");
            for (i, token) in current_portfolio.iter().enumerate().take(10) { // Show first 10 tokens
                info!("   Token {}: {} {} (${:.6}/token) = ${:.2} value (balance: {:.6})", 
                      i+1, token.symbol.as_ref().unwrap_or(&"UNKNOWN".to_string()), 
                      token.address, token.price_usd, token.value_usd, token.ui_amount);
            }
            if current_portfolio.len() > 10 {
                info!("   ... and {} more tokens", current_portfolio.len() - 10);
            }
        }
        
        let current_prices = extract_current_prices_from_portfolio(&current_portfolio);
        info!("💲 Extracted {} current token prices for unrealized P&L calculation", current_prices.len());
        
        // Show some sample prices for debugging
        if !current_prices.is_empty() {
            info!("📊 Sample Current Prices (for unrealized P&L calculation):");
            for (token_addr, price) in current_prices.iter().take(5) {
                info!("   {}: ${:.6}", token_addr, price);
            }
        }

        // Step 4: Calculate P&L using transaction history algorithm with current prices
        info!("🧮 Step 4/4: Calculating P&L using history algorithm with current prices...");
        info!("🔧 Processing {} enriched transactions with {} current prices", 
              enriched_transactions.len(), current_prices.len());
        
        let pnl_start = std::time::Instant::now();
        let report = self.calculate_pnl_with_history_algorithm_with_prices(
            wallet_address, 
            chain, 
            enriched_transactions,
            current_prices
        ).await?;
        let pnl_elapsed = pnl_start.elapsed();

        let total_elapsed = step_start.elapsed();
        info!("✅ Step 4 completed in {:.2}s: P&L calculation finished", pnl_elapsed.as_secs_f64());
        info!("🎉 BirdEye P&L analysis completed for wallet {} in {:.2}s total - Realized: ${:.2}, Unrealized: ${:.2}, {} tokens", 
              wallet_address, total_elapsed.as_secs_f64(), report.total_realized_pnl_usd, 
              report.total_unrealized_pnl_usd, report.token_results.len());

        Ok(report)
    }


    // convert_birdeye_transactions_to_events removed - was unused legacy function

    // LEGACY METHOD REMOVED: convert_general_birdeye_transactions_to_events()
    // This method converted GeneralTraderTransaction to old FinancialEvent format
    // New P&L engine works directly with GeneralTraderTransaction with embedded prices

    /// Get system status
    pub async fn get_status(&self) -> Result<OrchestratorStatus> {
        // Try to get wallet-token pairs queue size with timeout, fallback to 0 if Redis unavailable
        let queue_size = {
            use tokio::time::{timeout, Duration};
            match timeout(Duration::from_millis(1000), async {
                let redis = &self.persistence_client;
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
                let redis = &self.persistence_client;
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
            discovery_queue_size: queue_size as u64,
            running_jobs_count: running_jobs_count as u64,
            batch_jobs_count: batch_jobs_count as u64,
        })
    }

    /// Clear temporary data
    pub async fn clear_temp_data(&self) -> Result<()> {
        let redis = &self.persistence_client;
        redis.clear_temp_data().await?;
        info!("Cleared temporary Redis data");
        Ok(())
    }

    /// Consolidate duplicate transaction hashes by merging multi-step swaps into single transactions
    /// This preprocessing step ensures each unique tx_hash results in exactly one transaction
    /// preserving the existing P&L algorithm's expectation of one transaction = one buy/sell pair
    pub fn consolidate_duplicate_hashes(transactions: Vec<GeneralTraderTransaction>) -> Vec<GeneralTraderTransaction> {
        use std::collections::HashMap;
        
        // Group transactions by tx_hash
        let mut tx_groups: HashMap<String, Vec<GeneralTraderTransaction>> = HashMap::new();
        for tx in transactions {
            tx_groups.entry(tx.tx_hash.clone()).or_insert_with(Vec::new).push(tx);
        }

        let mut consolidated_transactions = Vec::new();
        let mut duplicates_found = 0;

        for (tx_hash, entries) in tx_groups {
            if entries.len() == 1 {
                // Fast path: Single entry - pass through unchanged (99.7% of cases)
                consolidated_transactions.push(entries.into_iter().next().unwrap());
            } else {
                // Consolidation path: Multiple entries - consolidate into single transaction (0.3% of cases)
                duplicates_found += entries.len() - 1;
                info!("🔄 Consolidating {} duplicate entries for tx_hash: {}", entries.len(), tx_hash);
                consolidated_transactions.push(Self::consolidate_duplicate_entries(entries));
            }
        }

        if duplicates_found > 0 {
            info!("✅ Consolidated {} duplicate hash entries across {} unique transactions", 
                  duplicates_found, consolidated_transactions.len());
        }

        consolidated_transactions
    }

    /// Consolidate multiple entries with the same tx_hash into a single net transaction
    /// This handles multi-step swaps by calculating net token flows and weighted average pricing
    pub fn consolidate_duplicate_entries(entries: Vec<GeneralTraderTransaction>) -> GeneralTraderTransaction {
        use std::collections::HashMap;
        
        let first_entry = &entries[0];
        
        // Calculate net flows for each token address
        let mut token_flows: HashMap<String, f64> = HashMap::new();
        let mut token_prices: HashMap<String, f64> = HashMap::new();
        let mut token_symbols: HashMap<String, String> = HashMap::new();
        let mut token_side_info: HashMap<String, TokenTransactionSide> = HashMap::new();

        for entry in &entries {
            // Process quote side
            let quote_addr = &entry.quote.address;
            *token_flows.entry(quote_addr.clone()).or_insert(0.0) += entry.quote.ui_change_amount;
            if let Some(price) = entry.quote.price {
                token_prices.insert(quote_addr.clone(), price);
            }
            token_symbols.insert(quote_addr.clone(), entry.quote.symbol.clone());
            token_side_info.insert(quote_addr.clone(), entry.quote.clone());

            // Process base side
            let base_addr = &entry.base.address;
            *token_flows.entry(base_addr.clone()).or_insert(0.0) += entry.base.ui_change_amount;
            if let Some(price) = entry.base.price {
                token_prices.insert(base_addr.clone(), price);
            }
            token_symbols.insert(base_addr.clone(), entry.base.symbol.clone());
            token_side_info.insert(base_addr.clone(), entry.base.clone());
        }

        // Find primary outflow (most negative) and inflow (most positive) tokens
        let mut primary_outflow: Option<(String, f64)> = None;
        let mut primary_inflow: Option<(String, f64)> = None;

        for (token_addr, &net_flow) in &token_flows {
            if net_flow.abs() < 0.000001 { continue; } // Skip dust amounts

            if net_flow < 0.0 {
                if primary_outflow.is_none() || net_flow.abs() > primary_outflow.as_ref().unwrap().1.abs() {
                    primary_outflow = Some((token_addr.clone(), net_flow));
                }
            } else {
                if primary_inflow.is_none() || net_flow > primary_inflow.as_ref().unwrap().1 {
                    primary_inflow = Some((token_addr.clone(), net_flow));
                }
            }
        }

        // Handle edge cases where no clear outflow/inflow pattern exists
        let (outflow_addr, outflow_amount) = match primary_outflow {
            Some(outflow) => outflow,
            None => {
                warn!("⚠️ Skipping duplicate consolidation for tx {}: No outflow token found. Token flows: {:?}", 
                      first_entry.tx_hash, token_flows);
                // Return the first entry unchanged when consolidation fails
                return first_entry.clone();
            }
        };
        
        let (inflow_addr, inflow_amount) = match primary_inflow {
            Some(inflow) => inflow,
            None => {
                warn!("⚠️ Skipping duplicate consolidation for tx {}: No inflow token found. Token flows: {:?}", 
                      first_entry.tx_hash, token_flows);
                // Return the first entry unchanged when consolidation fails
                return first_entry.clone();
            }
        };

        // Create consolidated quote side (outflow - "from")
        let mut consolidated_quote = token_side_info[&outflow_addr].clone();
        consolidated_quote.ui_change_amount = outflow_amount; // negative
        consolidated_quote.ui_amount = outflow_amount.abs();
        consolidated_quote.type_swap = "from".to_string();

        // Create consolidated base side (inflow - "to") 
        let mut consolidated_base = token_side_info[&inflow_addr].clone();
        consolidated_base.ui_change_amount = inflow_amount; // positive
        consolidated_base.ui_amount = inflow_amount;
        consolidated_base.type_swap = "to".to_string();

        // Calculate consolidated volume_usd
        let total_volume_usd = entries.iter()
            .map(|e| e.volume_usd)
            .sum::<f64>();

        // Create consolidated transaction that maintains the same structure
        GeneralTraderTransaction {
            quote: consolidated_quote,
            base: consolidated_base,
            base_price: token_prices.get(&inflow_addr).copied(),
            quote_price: token_prices.get(&outflow_addr).unwrap_or(&0.0).clone(),
            tx_hash: first_entry.tx_hash.clone(),
            source: format!("consolidated_{}_entries", entries.len()),
            block_unix_time: first_entry.block_unix_time, // Use earliest timestamp
            tx_type: first_entry.tx_type.clone(),
            address: first_entry.address.clone(),
            owner: first_entry.owner.clone(),
            volume_usd: total_volume_usd,
        }
    }

    /// Calculate P&L using transaction history with enriched price data
    /// This method handles both swaps AND send transactions using the new HistoryTransactionParser
    async fn calculate_pnl_with_history_algorithm(
        &self,
        wallet_address: &str,
        _chain: &str,
        enriched_transactions: Vec<EnrichedTransaction>,
    ) -> Result<PortfolioPnLResult> {
        info!("🚀 Starting history P&L algorithm for wallet: {} with {} enriched transactions", 
              wallet_address, enriched_transactions.len());
        
        // Step 1: Parse enriched transactions using HistoryTransactionParser
        let history_parser = HistoryTransactionParser::new(wallet_address.to_string());
        let financial_events = history_parser.parse_enriched_transactions(enriched_transactions).await
            .map_err(|e| OrchestratorError::JobExecution(format!("Failed to parse enriched transactions: {}", e)))?;
        
        info!("📊 Parsed enriched transactions into {} financial events (including send transactions)", 
              financial_events.len());
        
        // Step 2: Group events by token for P&L processing
        let events_by_token = HistoryTransactionParser::group_events_by_token(financial_events);
        
        info!("📊 Grouped events into {} token groups", events_by_token.len());
        
        // Step 3: Calculate P&L using the new engine with balance API integration
        let balance_fetcher = if self.config.birdeye.balance_api_enabled {
            BalanceFetcher::new(
                self.config.birdeye.api_key.clone(), 
                Some(self.config.birdeye.api_base_url.clone())
            )
        } else {
            // Create a dummy fetcher - balance API disabled in config
            BalanceFetcher::new("disabled".to_string(), None)
        };
        
        let pnl_engine = if self.config.birdeye.balance_api_enabled {
            info!("💰 Using balance API for accurate remaining position valuation");
            NewPnLEngine::with_balance_fetcher(wallet_address.to_string(), balance_fetcher)
        } else {
            info!("💰 Balance API disabled - using standard P&L calculations");
            NewPnLEngine::new(wallet_address.to_string())
        };

        let mut portfolio_result = PortfolioPnLResult {
            wallet_address: wallet_address.to_string(),
            token_results: Vec::new(),
            total_realized_pnl_usd: Decimal::ZERO,
            total_unrealized_pnl_usd: Decimal::ZERO,
            total_pnl_usd: Decimal::ZERO,
            total_trades: 0,
            winning_trades: 0,
            losing_trades: 0,
            overall_win_rate_percentage: Decimal::ZERO,
            avg_hold_time_minutes: Decimal::ZERO,
            tokens_analyzed: 0,
            events_processed: 0,
            analysis_timestamp: Utc::now(),
            total_invested_usd: Decimal::ZERO,
            total_returned_usd: Decimal::ZERO,
            current_winning_streak: 0,
            longest_winning_streak: 0,
            current_losing_streak: 0,
            longest_losing_streak: 0,
            profit_percentage: Decimal::ZERO,
            unique_tokens_count: events_by_token.len() as u32,
            active_days_count: 0, // Will be calculated
        };

        let mut total_events = 0;
        for (token_address, events) in events_by_token {
            debug!("Processing {} events for token: {}", events.len(), token_address);
            total_events += events.len();

            match pnl_engine.calculate_token_pnl(events, None).await {
                Ok(token_result) => {
                    // Update portfolio totals
                    portfolio_result.total_realized_pnl_usd += token_result.total_realized_pnl_usd;
                    portfolio_result.total_unrealized_pnl_usd += token_result.total_unrealized_pnl_usd;
                    portfolio_result.total_trades += token_result.total_trades;
                    portfolio_result.winning_trades += token_result.winning_trades;
                    portfolio_result.losing_trades += token_result.losing_trades;
                    portfolio_result.total_invested_usd += token_result.total_invested_usd;
                    portfolio_result.total_returned_usd += token_result.total_returned_usd;
                    
                    portfolio_result.token_results.push(token_result);
                }
                Err(e) => {
                    warn!("❌ Failed to calculate P&L for token {}: {}. Skipping token.", token_address, e);
                }
            }
        }

        // Calculate final portfolio metrics
        portfolio_result.total_pnl_usd = portfolio_result.total_realized_pnl_usd + portfolio_result.total_unrealized_pnl_usd;
        portfolio_result.tokens_analyzed = portfolio_result.token_results.len() as u32;
        portfolio_result.events_processed = total_events as u32;

        if portfolio_result.total_trades > 0 {
            portfolio_result.overall_win_rate_percentage = Decimal::from(portfolio_result.winning_trades) 
                / Decimal::from(portfolio_result.total_trades) * Decimal::from(100);
        }

        if portfolio_result.total_invested_usd > Decimal::ZERO {
            portfolio_result.profit_percentage = (portfolio_result.total_pnl_usd / portfolio_result.total_invested_usd) * Decimal::from(100);
        }

        // Calculate average hold time from token results
        if !portfolio_result.token_results.is_empty() {
            let total_avg_hold_time: Decimal = portfolio_result.token_results.iter()
                .map(|token| token.avg_hold_time_minutes)
                .sum();
            portfolio_result.avg_hold_time_minutes = total_avg_hold_time / Decimal::from(portfolio_result.token_results.len());
        }

        // Calculate winning/losing streaks from token results
        for token_result in &portfolio_result.token_results {
            if token_result.current_winning_streak > portfolio_result.current_winning_streak {
                portfolio_result.current_winning_streak = token_result.current_winning_streak;
            }
            if token_result.longest_winning_streak > portfolio_result.longest_winning_streak {
                portfolio_result.longest_winning_streak = token_result.longest_winning_streak;
            }
            if token_result.current_losing_streak > portfolio_result.current_losing_streak {
                portfolio_result.current_losing_streak = token_result.current_losing_streak;
            }
            if token_result.longest_losing_streak > portfolio_result.longest_losing_streak {
                portfolio_result.longest_losing_streak = token_result.longest_losing_streak;
            }
        }

        info!("✅ History P&L algorithm completed for wallet: {}", wallet_address);
        info!("📊 Final results: {} tokens, {} events, total P&L: ${}", 
              portfolio_result.tokens_analyzed, 
              portfolio_result.events_processed, 
              portfolio_result.total_pnl_usd);

        Ok(portfolio_result)
    }

    /// Process a single wallet using GoldRush API for EVM chains (captures swaps AND sends as sells)
    async fn process_single_wallet_with_goldrush(
        &self,
        wallet_address: &str,
        chain: &str,
        _max_transactions: Option<u32>,
    ) -> Result<PortfolioPnLResult> {
        info!("🚀 Starting GoldRush P&L analysis (Balances + Transfers approach):");
        info!("  Wallet: {}", wallet_address);
        info!("  Chain: {}", chain);

        // Check if GoldRush client is available
        let goldrush_client = match &self.goldrush_client {
            Some(client) => {
                info!("✅ GoldRush client is available");
                client
            },
            None => {
                error!("❌ GoldRush client not initialized!");
                return Err(OrchestratorError::GoldRush(
                    "GoldRush client not available. Please check configuration".to_string()
                ));
            }
        };

        // Validate wallet address for EVM
        info!("🔍 Validating EVM wallet address format...");
        match GoldRushClient::validate_wallet_address(wallet_address) {
            Ok(_) => info!("✅ Wallet address format is valid"),
            Err(e) => {
                error!("❌ Invalid wallet address: {}", e);
                return Err(e.into());
            }
        }

        // Convert chain string to GoldRush chain enum
        let goldrush_chain = match chain.to_lowercase().as_str() {
            "ethereum" => {
                info!("📍 Using Ethereum mainnet");
                GoldRushChain::Ethereum
            },
            "base" => {
                info!("📍 Using Base mainnet");
                GoldRushChain::Base
            },
            "bsc" => {
                info!("📍 Using BSC mainnet");
                GoldRushChain::Bsc
            },
            _ => {
                error!("❌ Unsupported chain: {}", chain);
                return Err(OrchestratorError::Config(format!("Unsupported GoldRush chain: {}", chain)));
            }
        };

        // Step 1: Get all tokens the wallet has interacted with
        info!("📋 STEP 1: Fetching token balances to discover all tokens...");
        let balance_start = std::time::Instant::now();
        let token_balances = match goldrush_client
            .get_wallet_balances(wallet_address, goldrush_chain)
            .await {
                Ok(balances) => {
                    let balance_elapsed = balance_start.elapsed();
                    info!("✅ Found {} token balances in {:.2}s", balances.len(), balance_elapsed.as_secs_f64());
                    
                    // Log significant balances with USD values
                    let significant_balances: Vec<_> = balances.iter()
                        .filter(|b| !b.is_spam.unwrap_or(false) && b.balance.as_deref().unwrap_or("0") != "0")
                        .collect();
                    
                    info!("💰 Significant tokens (non-spam, non-zero): {}", significant_balances.len());
                    for (i, balance) in significant_balances.iter().take(5).enumerate() {
                        let ticker_symbol = balance.contract_ticker_symbol.as_deref().unwrap_or("Unknown");
                        let usd_value = balance.pretty_quote.as_deref().unwrap_or("$0.00");
                        info!("    {}. {} - {} - {}", i + 1, ticker_symbol, usd_value, &balance.contract_address[..10]);
                    }
                    
                    balances
                },
                Err(e) => {
                    error!("❌ Failed to fetch token balances: {}", e);
                    return Err(e.into());
                }
            };

        if token_balances.is_empty() {
            warn!("No token balances found for wallet: {}", wallet_address);
            return Ok(PortfolioPnLResult {
                wallet_address: wallet_address.to_string(),
                token_results: Vec::new(),
                total_realized_pnl_usd: Decimal::ZERO,
                total_unrealized_pnl_usd: Decimal::ZERO,
                total_pnl_usd: Decimal::ZERO,
                total_trades: 0,
                winning_trades: 0,
                losing_trades: 0,
                overall_win_rate_percentage: Decimal::ZERO,
                avg_hold_time_minutes: Decimal::ZERO,
                tokens_analyzed: 0,
                events_processed: 0,
                analysis_timestamp: Utc::now(),
                total_invested_usd: Decimal::ZERO,
                total_returned_usd: Decimal::ZERO,
                current_winning_streak: 0,
                longest_winning_streak: 0,
                current_losing_streak: 0,
                longest_losing_streak: 0,
                profit_percentage: Decimal::ZERO,
                unique_tokens_count: 0,
                active_days_count: 0,
            });
        }

        // Step 2: Get ALL wallet transactions with logs (OPTIMIZED - Single API call!)
        info!("🚀 STEP 2: Fetching ALL wallet transactions with logs (OPTIMIZED APPROACH)");
        info!("  ⚡ This replaces {} individual token API calls with 1 efficient call!", token_balances.len());
        let transactions_start = std::time::Instant::now();
        
        let transactions = match goldrush_client
            .get_wallet_transactions_with_logs(wallet_address, goldrush_chain, Some(20))
            .await {
                Ok(txs) => txs,
                Err(e) => {
                    error!("❌ Failed to fetch wallet transactions: {}", e);
                    return Err(OrchestratorError::GoldRush(format!(
                        "Failed to fetch wallet transactions: {}", e
                    )));
                }
            };
            
        let transactions_elapsed = transactions_start.elapsed();
        info!("✅ STEP 2 Complete: Fetched {} transactions in {:.2}s", transactions.len(), transactions_elapsed.as_secs_f64());
        info!("  ⚡ Performance improvement: ~{}x faster than individual token calls!", 
              if token_balances.len() > 0 { token_balances.len() } else { 1 });
              
        if transactions.is_empty() {
            warn!("No transactions found for wallet: {}", wallet_address);
            return Ok(PortfolioPnLResult {
                wallet_address: wallet_address.to_string(),
                token_results: Vec::new(),
                total_realized_pnl_usd: Decimal::ZERO,
                total_unrealized_pnl_usd: Decimal::ZERO,
                total_pnl_usd: Decimal::ZERO,
                total_trades: 0,
                winning_trades: 0,
                losing_trades: 0,
                overall_win_rate_percentage: Decimal::ZERO,
                avg_hold_time_minutes: Decimal::ZERO,
                tokens_analyzed: 0,
                events_processed: 0,
                analysis_timestamp: Utc::now(),
                total_invested_usd: Decimal::ZERO,
                total_returned_usd: Decimal::ZERO,
                current_winning_streak: 0,
                longest_winning_streak: 0,
                current_losing_streak: 0,
                longest_losing_streak: 0,
                profit_percentage: Decimal::ZERO,
                unique_tokens_count: 0,
                active_days_count: 0,
            });
        }

        // Step 2a: Extract token transfers from transaction logs
        info!("🔍 STEP 2a: Extracting token transfers from {} transactions...", transactions.len());
        let log_parsing_start = std::time::Instant::now();
        let all_transfers = self.extract_transfers_from_transaction_logs(&transactions, wallet_address)?;
        let log_parsing_elapsed = log_parsing_start.elapsed();
        
        info!("✅ STEP 2a Complete: Extracted {} transfers in {:.3}s", all_transfers.len(), log_parsing_elapsed.as_secs_f64());
        
        if all_transfers.is_empty() {
            warn!("No token transfers found in transaction logs for wallet: {}", wallet_address);
            return Ok(PortfolioPnLResult {
                wallet_address: wallet_address.to_string(),
                token_results: Vec::new(),
                total_realized_pnl_usd: Decimal::ZERO,
                total_unrealized_pnl_usd: Decimal::ZERO,
                total_pnl_usd: Decimal::ZERO,
                total_trades: 0,
                winning_trades: 0,
                losing_trades: 0,
                overall_win_rate_percentage: Decimal::ZERO,
                avg_hold_time_minutes: Decimal::ZERO,
                tokens_analyzed: 0,
                events_processed: 0,
                analysis_timestamp: Utc::now(),
                total_invested_usd: Decimal::ZERO,
                total_returned_usd: Decimal::ZERO,
                current_winning_streak: 0,
                longest_winning_streak: 0,
                current_losing_streak: 0,
                longest_losing_streak: 0,
                profit_percentage: Decimal::ZERO,
                unique_tokens_count: 0,
                active_days_count: 0,
            });
        }

        // Step 3: Convert transfers to unified financial events with USD prices
        info!("🔄 STEP 3: Converting {} transfers to financial events with USD prices...", all_transfers.len());
        let conversion_start = std::time::Instant::now();
        let converter = GoldRushEventConverter::new(wallet_address.to_string(), chain.to_string());
        let unified_events = converter.convert_token_transfers(all_transfers);
        let conversion_elapsed = conversion_start.elapsed();

        info!("✅ Generated {} financial events with USD data in {:.2}s", unified_events.len(), conversion_elapsed.as_secs_f64());
        
        // Log event type breakdown
        let buy_events = unified_events.iter().filter(|e| matches!(e.event_type, goldrush_client::UnifiedEventType::Buy)).count();
        let sell_events = unified_events.iter().filter(|e| matches!(e.event_type, goldrush_client::UnifiedEventType::Sell)).count();
        info!("📈 Event breakdown: {} BUY events, {} SELL events", buy_events, sell_events);
        
        // Log total USD values
        let total_usd_value: rust_decimal::Decimal = unified_events.iter()
            .map(|e| e.usd_value)
            .sum();
        info!("💲 Total USD value of all events: ${:.2}", total_usd_value);

        // Step 4: Convert to NewFinancialEvent format for P&L engine
        info!("🔧 STEP 4: Converting to P&L engine format...");
        let financial_events: Vec<NewFinancialEvent> = unified_events
            .into_iter()
            .map(|event| event.into())
            .collect();

        info!("✅ Converted {} events to P&L engine format", financial_events.len());

        // Step 5: Calculate P&L using the standard P&L engine
        info!("🧮 STEP 5: Running P&L calculations...");
        let pnl_start = std::time::Instant::now();
        let result = self.calculate_pnl_with_goldrush_events(
            wallet_address,
            chain,
            financial_events,
        ).await;
        let pnl_elapsed = pnl_start.elapsed();
        
        match &result {
            Ok(pnl_result) => {
                info!("✅ P&L calculation completed in {:.2}s", pnl_elapsed.as_secs_f64());
                info!("💰 Final Results:");
                info!("  📊 Total P&L: ${:.2}", pnl_result.total_pnl_usd);
                info!("  📈 Realized P&L: ${:.2}", pnl_result.total_realized_pnl_usd);  
                info!("  📉 Unrealized P&L: ${:.2}", pnl_result.total_unrealized_pnl_usd);
                info!("  🔢 Total trades: {}", pnl_result.total_trades);
                info!("  🪙 Tokens analyzed: {}", pnl_result.tokens_analyzed);
                info!("  ⚡ Events processed: {}", pnl_result.events_processed);
            }
            Err(e) => {
                error!("❌ P&L calculation failed in {:.2}s: {}", pnl_elapsed.as_secs_f64(), e);
            }
        }
        
        result
    }

    /// Calculate P&L using GoldRush financial events
    async fn calculate_pnl_with_goldrush_events(
        &self,
        wallet_address: &str,
        _chain: &str,
        events: Vec<NewFinancialEvent>,
    ) -> Result<PortfolioPnLResult> {
        if events.is_empty() {
            warn!("No financial events to process for wallet: {}", wallet_address);
            return Ok(PortfolioPnLResult {
                wallet_address: wallet_address.to_string(),
                token_results: Vec::new(),
                total_realized_pnl_usd: Decimal::ZERO,
                total_unrealized_pnl_usd: Decimal::ZERO,
                total_pnl_usd: Decimal::ZERO,
                total_trades: 0,
                winning_trades: 0,
                losing_trades: 0,
                overall_win_rate_percentage: Decimal::ZERO,
                avg_hold_time_minutes: Decimal::ZERO,
                tokens_analyzed: 0,
                events_processed: 0,
                analysis_timestamp: Utc::now(),
                total_invested_usd: Decimal::ZERO,
                total_returned_usd: Decimal::ZERO,
                current_winning_streak: 0,
                longest_winning_streak: 0,
                current_losing_streak: 0,
                longest_losing_streak: 0,
                profit_percentage: Decimal::ZERO,
                unique_tokens_count: 0,
                active_days_count: 0,
            });
        }

        info!("🧮 Processing {} financial events for P&L calculation", events.len());

        // Group events by token
        let mut events_by_token: HashMap<String, Vec<NewFinancialEvent>> = HashMap::new();
        for event in events {
            events_by_token
                .entry(event.token_address.clone())
                .or_insert_with(Vec::new)
                .push(event);
        }

        info!("🪙 Grouped events for {} unique tokens", events_by_token.len());
        
        // Log events per token
        for (_token_address, token_events) in &events_by_token {
            let unknown = "Unknown".to_string();
            let symbol = token_events.first().map(|e| &e.token_symbol).unwrap_or(&unknown);
            let buy_count = token_events.iter().filter(|e| e.event_type == pnl_core::NewEventType::Buy).count();
            let sell_count = token_events.iter().filter(|e| e.event_type == pnl_core::NewEventType::Sell).count();
            info!("  📊 {}: {} events ({} BUY, {} SELL)", symbol, token_events.len(), buy_count, sell_count);
        }

        let mut token_results = Vec::new();
        let mut total_realized = Decimal::ZERO;
        let mut total_unrealized = Decimal::ZERO;
        let mut total_volume = Decimal::ZERO;

        // Calculate P&L for each token
        for (_token_address, token_events) in events_by_token {
            let pnl_engine = NewPnLEngine::new(wallet_address.to_string());
            
            // For GoldRush events, we already have USD prices embedded
            // Use a current price of zero since we don't have portfolio API for EVM chains yet
            let result = pnl_engine.calculate_token_pnl(token_events, Some(Decimal::ZERO)).await?;

            total_realized += result.total_realized_pnl_usd;
            total_unrealized += result.total_unrealized_pnl_usd;
            total_volume += result.total_invested_usd;

            token_results.push(result);
        }

        let total_pnl = total_realized + total_unrealized;
        let profitable_tokens = token_results
            .iter()
            .filter(|result| (result.total_realized_pnl_usd + result.total_unrealized_pnl_usd) > Decimal::ZERO)
            .count();
        let losing_tokens = token_results
            .iter()
            .filter(|result| (result.total_realized_pnl_usd + result.total_unrealized_pnl_usd) < Decimal::ZERO)
            .count();
        
        let win_rate = if token_results.is_empty() {
            Decimal::ZERO
        } else {
            Decimal::from(profitable_tokens) / Decimal::from(token_results.len()) * Decimal::from(100)
        };

        let total_trades: u32 = token_results.iter().map(|r| r.total_trades).sum();
        let total_events: u32 = token_results.iter().map(|r| r.matched_trades.len() as u32).sum();
        let tokens_count = token_results.len() as u32;

        info!(
            "P&L calculation complete for wallet {}: Total P&L: ${}, Realized: ${}, Unrealized: ${}, Win Rate: {:.1}%",
            wallet_address, total_pnl, total_realized, total_unrealized, win_rate
        );

        Ok(PortfolioPnLResult {
            wallet_address: wallet_address.to_string(),
            token_results,
            total_realized_pnl_usd: total_realized,
            total_unrealized_pnl_usd: total_unrealized,
            total_pnl_usd: total_pnl,
            total_trades,
            winning_trades: profitable_tokens as u32,
            losing_trades: losing_tokens as u32,
            overall_win_rate_percentage: win_rate,
            avg_hold_time_minutes: Decimal::ZERO, // TODO: Calculate from events
            tokens_analyzed: tokens_count,
            events_processed: total_events,
            analysis_timestamp: Utc::now(),
            total_invested_usd: total_volume, // Approximation
            total_returned_usd: total_volume + total_pnl, // Approximation
            current_winning_streak: 0, // TODO: Calculate
            longest_winning_streak: 0, // TODO: Calculate
            current_losing_streak: 0, // TODO: Calculate
            longest_losing_streak: 0, // TODO: Calculate
            profit_percentage: if total_volume > Decimal::ZERO { (total_pnl / total_volume) * Decimal::from(100) } else { Decimal::ZERO },
            unique_tokens_count: tokens_count,
            active_days_count: 0, // TODO: Calculate from transaction dates
        })
    }

    /// Calculate P&L using transaction history with current portfolio prices for accurate unrealized P&L
    /// This method handles both swaps AND send transactions and uses real-time token prices
    async fn calculate_pnl_with_history_algorithm_with_prices(
        &self,
        wallet_address: &str,
        _chain: &str,
        enriched_transactions: Vec<EnrichedTransaction>,
        current_prices: HashMap<String, Decimal>,
    ) -> Result<PortfolioPnLResult> {
        info!("🚀 Starting history P&L algorithm with {} current prices for wallet: {} with {} enriched transactions", 
              current_prices.len(), wallet_address, enriched_transactions.len());
        
        // Step 1: Parse enriched transactions using HistoryTransactionParser
        let history_parser = HistoryTransactionParser::new(wallet_address.to_string());
        let financial_events = history_parser.parse_enriched_transactions(enriched_transactions).await
            .map_err(|e| OrchestratorError::JobExecution(format!("Failed to parse enriched transactions: {}", e)))?;
        
        info!("📊 Parsed enriched transactions into {} financial events (including send transactions)", 
              financial_events.len());
        
        // Step 2: Group events by token for P&L processing
        let events_by_token = HistoryTransactionParser::group_events_by_token(financial_events);
        
        info!("📊 Grouped events into {} token groups", events_by_token.len());
        
        // Show token breakdown for debugging
        if !events_by_token.is_empty() {
            info!("🔍 Token groups breakdown:");
            for (token_addr, events) in events_by_token.iter().take(5) {
                info!("   {} events for token: {}", events.len(), token_addr);
            }
            if events_by_token.len() > 5 {
                info!("   ... and {} more tokens", events_by_token.len() - 5);
            }
        }
        
        // Step 3: Calculate P&L using the new engine with current prices for accurate unrealized P&L
        info!("🧮 Initializing P&L engine for per-token calculations...");
        let pnl_engine = NewPnLEngine::new(wallet_address.to_string());

        let mut portfolio_result = PortfolioPnLResult {
            wallet_address: wallet_address.to_string(),
            token_results: Vec::new(),
            total_realized_pnl_usd: Decimal::ZERO,
            total_unrealized_pnl_usd: Decimal::ZERO,
            total_pnl_usd: Decimal::ZERO,
            total_trades: 0,
            winning_trades: 0,
            losing_trades: 0,
            overall_win_rate_percentage: Decimal::ZERO,
            avg_hold_time_minutes: Decimal::ZERO,
            tokens_analyzed: 0,
            events_processed: 0,
            analysis_timestamp: Utc::now(),
            total_invested_usd: Decimal::ZERO,
            total_returned_usd: Decimal::ZERO,
            current_winning_streak: 0,
            longest_winning_streak: 0,
            current_losing_streak: 0,
            longest_losing_streak: 0,
            profit_percentage: Decimal::ZERO,
            unique_tokens_count: events_by_token.len() as u32,
            active_days_count: 0, // Will be calculated
        };

        let mut total_events = 0;
        let total_tokens = events_by_token.len();
        info!("🔄 Starting per-token P&L calculations for {} tokens...", total_tokens);
        
        for (token_index, (token_address, events)) in events_by_token.into_iter().enumerate() {
            info!("🔍 Processing token {}/{}: {} ({} financial events)", 
                  token_index + 1, total_tokens, token_address, events.len());
            total_events += events.len();

            // Get current price for this token from portfolio data
            let current_price = current_prices.get(&token_address).cloned();
            
            if let Some(price) = current_price {
                info!("💲 Token {} has current price: ${:.6} (will be used for unrealized P&L calculation)", 
                      token_address, price);
            } else {
                info!("⚠️ Token {} has NO current price available - unrealized P&L will be $0.00", token_address);
            }

            info!("🧮 Calling P&L engine for token {} calculations...", token_address);
            match pnl_engine.calculate_token_pnl(events, current_price).await {
                Ok(token_result) => {
                    info!("✅ Token {}/{} P&L calculated successfully: {}", 
                          token_index + 1, total_tokens, token_result.token_symbol);
                    info!("   📊 Realized P&L: ${:.2}", token_result.total_realized_pnl_usd);
                    info!("   📊 Unrealized P&L: ${:.2}", token_result.total_unrealized_pnl_usd);
                    info!("   📊 Total P&L: ${:.2}", token_result.total_pnl_usd);
                    match &token_result.remaining_position {
                        Some(pos) => info!("   📊 Remaining position: {:.6} tokens", pos.quantity),
                        None => info!("   📊 Remaining position: 0.000000 tokens"),
                    }
                    
                    // Update portfolio totals
                    portfolio_result.total_realized_pnl_usd += token_result.total_realized_pnl_usd;
                    portfolio_result.total_unrealized_pnl_usd += token_result.total_unrealized_pnl_usd;
                    portfolio_result.total_trades += token_result.total_trades;
                    portfolio_result.winning_trades += token_result.winning_trades;
                    portfolio_result.losing_trades += token_result.losing_trades;
                    portfolio_result.total_invested_usd += token_result.total_invested_usd;
                    portfolio_result.total_returned_usd += token_result.total_returned_usd;
                    
                    // Log unrealized P&L calculation details
                    if token_result.total_unrealized_pnl_usd != Decimal::ZERO {
                        if let (Some(price), Some(pos)) = (current_price, &token_result.remaining_position) {
                            let position_value = pos.quantity * price;
                            info!("💰 Token {} UNREALIZED P&L CALCULATION:", token_result.token_symbol);
                            info!("   💲 Current price: ${:.6} per token", price);
                            info!("   📦 Remaining position: {:.6} tokens", pos.quantity);
                            info!("   💵 Position value: {:.6} * ${:.6} = ${:.2}", 
                                  pos.quantity, price, position_value);
                            info!("   💰 Unrealized P&L: ${:.2}", token_result.total_unrealized_pnl_usd);
                            info!("   📈 Cost basis: ${:.6} per token (avg cost)", pos.avg_cost_basis_usd);
                        } else {
                            warn!("⚠️ Token {} has unrealized P&L ${:.2} but no current price or position - this shouldn't happen!", 
                                  token_result.token_symbol, token_result.total_unrealized_pnl_usd);
                        }
                    } else {
                        info!("💰 Token {} has NO unrealized P&L (no remaining position or no current price)", 
                              token_result.token_symbol);
                    }
                    
                    portfolio_result.token_results.push(token_result);
                }
                Err(e) => {
                    warn!("❌ Failed to calculate P&L for token {}: {}. Skipping token.", token_address, e);
                }
            }
        }

        // Calculate final portfolio metrics
        portfolio_result.total_pnl_usd = portfolio_result.total_realized_pnl_usd + portfolio_result.total_unrealized_pnl_usd;
        portfolio_result.tokens_analyzed = portfolio_result.token_results.len() as u32;
        portfolio_result.events_processed = total_events as u32;

        // Calculate win rate
        if portfolio_result.total_trades > 0 {
            portfolio_result.overall_win_rate_percentage = 
                (Decimal::from(portfolio_result.winning_trades) / Decimal::from(portfolio_result.total_trades)) * Decimal::from(100);
        }

        // Calculate profit percentage
        if portfolio_result.total_invested_usd > Decimal::ZERO {
            portfolio_result.profit_percentage = (portfolio_result.total_pnl_usd / portfolio_result.total_invested_usd) * Decimal::from(100);
        }

        // Calculate average hold time from token results
        if !portfolio_result.token_results.is_empty() {
            let total_avg_hold_time: Decimal = portfolio_result.token_results.iter()
                .map(|token| token.avg_hold_time_minutes)
                .sum();
            portfolio_result.avg_hold_time_minutes = total_avg_hold_time / Decimal::from(portfolio_result.token_results.len());
        }

        // Calculate winning/losing streaks from token results
        let mut current_win_streak = 0;
        let mut current_lose_streak = 0;
        let mut max_win_streak = 0;
        let mut max_lose_streak = 0;
        
        for token in &portfolio_result.token_results {
            if token.total_pnl_usd > Decimal::ZERO {
                current_win_streak += 1;
                current_lose_streak = 0;
                max_win_streak = max_win_streak.max(current_win_streak);
            } else if token.total_pnl_usd < Decimal::ZERO {
                current_lose_streak += 1;
                current_win_streak = 0;
                max_lose_streak = max_lose_streak.max(current_lose_streak);
            }
        }
        
        portfolio_result.current_winning_streak = current_win_streak;
        portfolio_result.longest_winning_streak = max_win_streak;
        portfolio_result.current_losing_streak = current_lose_streak;
        portfolio_result.longest_losing_streak = max_lose_streak;

        info!("✅ P&L calculation completed with {} tokens analyzed. Total P&L: ${:.2} (Realized: ${:.2}, Unrealized: ${:.2})", 
              portfolio_result.tokens_analyzed, 
              portfolio_result.total_pnl_usd,
              portfolio_result.total_realized_pnl_usd,
              portfolio_result.total_unrealized_pnl_usd);

        Ok(portfolio_result)
    }

    /// LEGACY: Calculate P&L using the old swap-only algorithm (kept for compatibility)
    /// This method will be deprecated once all callers are updated to use history algorithm
    async fn calculate_pnl_with_new_algorithm(
        &self,
        wallet_address: &str,
        chain: &str,
        transactions: Vec<GeneralTraderTransaction>,
    ) -> Result<PortfolioPnLResult> {
        info!("🚀 Starting new P&L algorithm for wallet: {}", wallet_address);
        
        // Step 0: Preprocessing - Consolidate duplicate transaction hashes
        let original_count = transactions.len();
        let consolidated_transactions = Self::consolidate_duplicate_hashes(transactions);
        let consolidated_count = consolidated_transactions.len();
        
        if original_count != consolidated_count {
            info!("📝 Preprocessing: {} transactions → {} consolidated transactions", 
                  original_count, consolidated_count);
        }
        
        // Step 1: Data Preparation & Parsing
        let parser = NewTransactionParser::new(wallet_address.to_string());
        let financial_events = parser.parse_transactions(consolidated_transactions).await
            .map_err(|e| OrchestratorError::JobExecution(format!("Failed to parse transactions: {}", e)))?;
        
        info!("📊 Parsed {} consolidated transactions into {} financial events", 
              consolidated_count, financial_events.len());
        
        // Step 2: Group events by token for P&L processing
        let events_by_token = NewTransactionParser::group_events_by_token(financial_events);
        
        info!("📊 Grouped events into {} token groups", events_by_token.len());
        
        // Step 3: Calculate P&L using the new engine with balance API integration
        let balance_fetcher = if self.config.birdeye.balance_api_enabled {
            BalanceFetcher::new(
                self.config.birdeye.api_key.clone(), 
                Some(self.config.birdeye.api_base_url.clone())
            )
        } else {
            // Create a dummy fetcher - balance API disabled in config
            BalanceFetcher::new("disabled".to_string(), None)
        };
        
        let pnl_engine = if self.config.birdeye.balance_api_enabled {
            NewPnLEngine::with_balance_fetcher(wallet_address.to_string(), balance_fetcher)
        } else {
            NewPnLEngine::new(wallet_address.to_string())
        };
        
        // Fetch current prices for unrealized P&L calculations
        let current_prices = self.fetch_current_prices_for_tokens(&events_by_token, chain).await?;
        
        let portfolio_result = pnl_engine.calculate_portfolio_pnl(events_by_token, current_prices.clone()).await
            .map_err(|e| OrchestratorError::JobExecution(format!("Failed to calculate P&L: {}", e)))?;
        
        info!("✅ New P&L algorithm completed - Total P&L: ${}, Trades: {}, Win Rate: {}%",
              portfolio_result.total_pnl_usd,
              portfolio_result.total_trades,
              portfolio_result.overall_win_rate_percentage);
        
        // Step 4: Return rich PortfolioPnLResult directly (breaking change)
        // No longer converting to legacy format - using rich data throughout system
        Ok(portfolio_result)
    }
    
    
    /// Fetch current prices for all tokens in the analysis
    async fn fetch_current_prices_for_tokens(
        &self,
        events_by_token: &HashMap<String, Vec<NewFinancialEvent>>,
        chain: &str,
    ) -> Result<Option<HashMap<String, Decimal>>> {
        let token_addresses: Vec<String> = events_by_token.keys().cloned().collect();
        
        if token_addresses.is_empty() {
            return Ok(None);
        }
        
        info!("Fetching current prices for {} tokens", token_addresses.len());
        
        // Use BirdEye client to get current prices
        // Use the chain parameter for multichain price fetching
        match self.birdeye_client.get_current_prices(&token_addresses, chain).await {
            Ok(prices_f64) => {
                // Convert f64 prices to Decimal
                let mut prices: HashMap<String, Decimal> = HashMap::new();
                for (token, price) in prices_f64 {
                    match Decimal::try_from(price) {
                        Ok(decimal_price) => {
                            prices.insert(token, decimal_price);
                        }
                        Err(e) => {
                            warn!("Failed to convert price for token: {} - {}", token, e);
                        }
                    }
                }
                info!("Successfully fetched prices for {} tokens", prices.len());
                Ok(Some(prices))
            }
            Err(e) => {
                warn!("Failed to fetch current prices: {}. Continuing without current prices.", e);
                // Return None to use cost basis as current price (conservative approach)
                Ok(None)
            }
        }
    }

    /// Process wallet-token pair using BirdEye API
    async fn process_wallet_token_pair_with_birdeye(
        &self,
        pair: &DiscoveredWalletToken,
        max_transactions: Option<u32>,
    ) -> Result<Vec<GeneralTraderTransaction>> {
        // Fetch all trading transactions for the wallet using BirdEye with pagination
        let max_total_transactions = max_transactions
            .unwrap_or(self.config.birdeye.default_max_transactions);
        
        // No time bounds filtering - get all transactions (filter in frontend)
        debug!("Fetching up to {} BirdEye transactions for wallet-token pair {}", 
               max_total_transactions, pair.wallet_address);
        
        let transactions = self
            .birdeye_client
            .get_all_trader_transactions_paginated(&pair.wallet_address, &pair.chain, None, None, max_total_transactions)
            .await?;

        if transactions.is_empty() {
            return Err(OrchestratorError::JobExecution(format!(
                "No BirdEye transactions found for wallet: {} on token: {}",
                pair.wallet_address, pair.token_symbol
            )));
        }

        info!("📊 Found {} BirdEye transactions for {} trading {}", 
              transactions.len(), pair.wallet_address, pair.token_symbol);

        Ok(transactions)
    }

    // =====================================
    // Token Analysis Methods
    // =====================================

    /// Submit a token analysis job
    pub async fn submit_token_analysis_job(
        &self,
        token_addresses: Vec<String>,
        chain: String,
        max_transactions: Option<u32>,
    ) -> Result<Uuid> {
        let job = TokenAnalysisJob::new(token_addresses.clone(), chain.clone(), max_transactions);
        let job_id = job.id;

        info!("🚀 Starting token analysis job {} for {} tokens on {}", 
              job_id, token_addresses.len(), chain);

        // Store job in persistence and memory
        self.persistence_client.store_token_analysis_job(&job).await
            .map_err(|e| OrchestratorError::Persistence(e.to_string()))?;
        
        {
            let mut running_jobs = self.running_token_analysis_jobs.lock().await;
            running_jobs.insert(job_id, job);
        }

        // Spawn background task for token analysis
        let orchestrator_clone = self.clone();
        let job_id_clone = job_id;
        tokio::spawn(async move {
            if let Err(e) = orchestrator_clone.process_token_analysis_job(job_id_clone).await {
                error!("Token analysis job {} failed: {}", job_id_clone, e);
                
                // Update job status to failed
                let mut running_jobs = orchestrator_clone.running_token_analysis_jobs.lock().await;
                if let Some(job) = running_jobs.get_mut(&job_id_clone) {
                    job.status = JobStatus::Failed;
                    job.completed_at = Some(Utc::now());
                    
                    // Update in persistence
                    if let Err(persist_err) = orchestrator_clone.persistence_client.update_token_analysis_job(job).await {
                        error!("Failed to update failed job {} in persistence: {}", job_id_clone, persist_err);
                    }
                }
            }
        });

        Ok(job_id)
    }

    /// Process a token analysis job
    async fn process_token_analysis_job(&self, job_id: Uuid) -> Result<()> {
        info!("🔄 Processing token analysis job {}", job_id);

        // Get job from running jobs and update status
        let (token_addresses, chain, max_transactions) = {
            let mut running_jobs = self.running_token_analysis_jobs.lock().await;
            let job = running_jobs.get_mut(&job_id)
                .ok_or_else(|| OrchestratorError::JobExecution(format!("Token analysis job {} not found", job_id)))?;
            
            job.status = JobStatus::Running;
            job.started_at = Some(Utc::now());
            
            // Update in persistence
            self.persistence_client.update_token_analysis_job(job).await
                .map_err(|e| OrchestratorError::Persistence(e.to_string()))?;
            
            (job.token_addresses.clone(), job.chain.clone(), job.get_max_transactions())
        };

        let mut discovered_wallet_addresses = std::collections::HashSet::new();

        // Step 1: Discover top traders for each token
        for token_address in &token_addresses {
            info!("🔍 Discovering top traders for token: {}", token_address);
            
            match self.birdeye_client.get_top_traders_paginated(token_address, &chain).await {
                Ok(traders) => {
                    info!("📊 Found {} top traders for token {}", traders.len(), token_address);
                    for trader in traders {
                        discovered_wallet_addresses.insert(trader.owner);
                    }
                }
                Err(e) => {
                    warn!("Failed to get top traders for token {}: {}", token_address, e);
                }
            }
        }

        let discovered_wallets: Vec<String> = discovered_wallet_addresses.into_iter().collect();
        info!("🎯 Discovered {} unique wallets across {} tokens", 
              discovered_wallets.len(), token_addresses.len());

        // Update job with discovered wallets
        {
            let mut running_jobs = self.running_token_analysis_jobs.lock().await;
            if let Some(job) = running_jobs.get_mut(&job_id) {
                job.discovered_wallets = discovered_wallets.clone();
            }
        }

        // Step 2: Analyze discovered wallets (similar to batch job processing)
        let mut analyzed_wallets = Vec::new();
        let mut failed_wallets = Vec::new();

        // Process wallets in parallel batches (like batch jobs)
        let batch_size = self.config.system.pnl_parallel_batch_size.unwrap_or(10);
        for wallet_chunk in discovered_wallets.chunks(batch_size) {
            let futures: Vec<_> = wallet_chunk.iter().map(|wallet| {
                let wallet = wallet.clone();
                let chain = chain.clone();
                let max_transactions = max_transactions;
                async move {
                    let result = self.process_single_wallet_for_token_analysis(&wallet, &chain, max_transactions).await;
                    (wallet, result)
                }
            }).collect();

            let results = join_all(futures).await;
            
            for (wallet, result) in results {
                match result {
                    Ok(_) => {
                        analyzed_wallets.push(wallet);
                    }
                    Err(e) => {
                        warn!("Failed to analyze wallet {} for token analysis: {}", wallet, e);
                        failed_wallets.push(wallet);
                    }
                }
            }
        }

        // Update final job status
        {
            let mut running_jobs = self.running_token_analysis_jobs.lock().await;
            if let Some(job) = running_jobs.get_mut(&job_id) {
                job.analyzed_wallets = analyzed_wallets.clone();
                job.failed_wallets = failed_wallets.clone();
                job.status = JobStatus::Completed;
                job.completed_at = Some(Utc::now());
                
                // Update in persistence
                if let Err(e) = self.persistence_client.update_token_analysis_job(job).await {
                    error!("Failed to update completed job {} in persistence: {}", job_id, e);
                }
            }
        }

        // Clean up completed job from memory after a delay to allow API calls to fetch final status
        let orchestrator_clone = self.clone();
        let job_id_clone = job_id;
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(60)).await; // Keep in memory for 1 minute
            let mut running_jobs = orchestrator_clone.running_token_analysis_jobs.lock().await;
            if let Some(job) = running_jobs.get(&job_id_clone) {
                if job.status == JobStatus::Completed || job.status == JobStatus::Failed {
                    running_jobs.remove(&job_id_clone);
                    debug!("Cleaned up completed job {} from memory", job_id_clone);
                }
            }
        });

        info!("✅ Token analysis job {} completed: {}/{} wallets successfully analyzed", 
              job_id, analyzed_wallets.len(), discovered_wallets.len());

        Ok(())
    }

    /// Process a single wallet for token analysis
    async fn process_single_wallet_for_token_analysis(
        &self,
        wallet_address: &str,
        chain: &str,
        max_transactions: Option<u32>,
    ) -> Result<()> {
        // Use the same P&L processing logic as batch jobs, but store with "token_analysis" source
        let transactions = self.birdeye_client
            .get_all_trader_transactions(wallet_address, chain, None, None, max_transactions)
            .await?;

        if transactions.is_empty() {
            warn!("No transactions found for wallet {}", wallet_address);
            return Err(OrchestratorError::JobExecution(
                format!("No transactions found for wallet {}", wallet_address)
            ));
        }

        // Step 0: Preprocessing - Consolidate duplicate transaction hashes (DEX aggregation fix)
        let original_count = transactions.len();
        let consolidated_transactions = Self::consolidate_duplicate_hashes(transactions);
        let consolidated_count = consolidated_transactions.len();
        
        if original_count != consolidated_count {
            info!("🔄 Token analysis preprocessing: {} transactions → {} consolidated transactions", 
                  original_count, consolidated_count);
        }

        let parser = NewTransactionParser::new(wallet_address.to_string());
        let events = parser.parse_transactions(consolidated_transactions).await
            .map_err(|e| OrchestratorError::PnL(e))?;

        if events.is_empty() {
            warn!("No financial events found for wallet {}", wallet_address);
            return Err(OrchestratorError::JobExecution(
                format!("No financial events found for wallet {}", wallet_address)
            ));
        }

        // Group events by token for P&L processing
        let events_by_token = NewTransactionParser::group_events_by_token(events);
        
        // Create P&L engine with balance API integration for token analysis
        let balance_fetcher = if self.config.birdeye.balance_api_enabled {
            BalanceFetcher::new(
                self.config.birdeye.api_key.clone(), 
                Some(self.config.birdeye.api_base_url.clone())
            )
        } else {
            BalanceFetcher::new("disabled".to_string(), None)
        };
        
        let engine = if self.config.birdeye.balance_api_enabled {
            NewPnLEngine::with_balance_fetcher(wallet_address.to_string(), balance_fetcher)
        } else {
            NewPnLEngine::new(wallet_address.to_string())
        };
        let portfolio_result = engine.calculate_portfolio_pnl(events_by_token, None).await
            .map_err(|e| OrchestratorError::PnL(e))?;

        // Store with "token_analysis" source
        self.persistence_client
            .store_pnl_result_with_source(wallet_address, chain, &portfolio_result, "token_analysis")
            .await
            .map_err(|e| OrchestratorError::Persistence(e.to_string()))?;

        debug!("✅ Analyzed wallet {} for token analysis", wallet_address);
        Ok(())
    }

    /// Get token analysis job status
    pub async fn get_token_analysis_job_status(&self, job_id: Uuid) -> Option<TokenAnalysisJob> {
        // First check in-memory jobs (for running jobs)
        {
            let running_jobs = self.running_token_analysis_jobs.lock().await;
            if let Some(job) = running_jobs.get(&job_id) {
                return Some(job.clone());
            }
        }
        
        // If not in memory, check persistence (for completed/failed jobs)
        match self.persistence_client.get_token_analysis_job(&job_id.to_string()).await {
            Ok(job) => job,
            Err(e) => {
                error!("Failed to get job {} from persistence: {}", job_id, e);
                None
            }
        }
    }

    /// Get all running token analysis jobs
    pub async fn get_all_token_analysis_jobs(&self) -> Vec<TokenAnalysisJob> {
        let running_jobs = self.running_token_analysis_jobs.lock().await;
        running_jobs.values().cloned().collect()
    }

    /// Get all token analysis jobs from persistence with pagination
    pub async fn get_all_token_analysis_jobs_from_persistence(&self, limit: usize, offset: usize) -> Result<(Vec<TokenAnalysisJob>, usize)> {
        self.persistence_client.get_all_token_analysis_jobs(limit, offset).await
            .map_err(|e| OrchestratorError::Persistence(e.to_string()))
    }

    /// Extract token transfers from transaction logs (optimized approach)
    /// This replaces the need to make individual API calls for each token
    fn extract_transfers_from_transaction_logs(
        &self,
        transactions: &[GoldRushTransaction],
        wallet_address: &str,
    ) -> Result<Vec<TokenTransfer>> {
        
        info!("🔍 Extracting transfers from {} transactions...", transactions.len());
        let start_time = std::time::Instant::now();
        
        let mut transfers = Vec::new();
        let mut transactions_with_logs = 0;
        let mut total_log_events = 0;
        let mut erc20_transfers = 0;
        
        for tx in transactions {
            if let Some(ref log_events) = tx.log_events {
                if !log_events.is_empty() {
                    transactions_with_logs += 1;
                    total_log_events += log_events.len();
                    
                    // Process each log event looking for token transfers
                    for log_event in log_events {
                        // Look for ERC20 Transfer events (topic[0] = Transfer signature)
                        if !log_event.raw_log_topics.is_empty() && 
                           log_event.raw_log_topics[0] == "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef" {
                            // This is an ERC20 Transfer event
                            if let Some(transfer) = self.parse_erc20_transfer_from_log(
                                log_event, tx, wallet_address
                            ) {
                                transfers.push(transfer);
                                erc20_transfers += 1;
                            }
                        }
                    }
                }
            }
        }
        
        let elapsed = start_time.elapsed();
        
        info!("✅ Transfer extraction complete in {:.3}s:", elapsed.as_secs_f64());
        info!("  📋 {} transactions with logs processed", transactions_with_logs);
        info!("  🏷️  {} total log events analyzed", total_log_events);
        info!("  💰 {} ERC20 transfers extracted", erc20_transfers);
        info!("  📊 {} transfers relevant to wallet", transfers.len());
        
        if transfers.is_empty() {
            warn!("No relevant token transfers found for wallet {}", wallet_address);
        }
        
        Ok(transfers)
    }

    /// Parse an ERC20 transfer event from a transaction log
    fn parse_erc20_transfer_from_log(
        &self,
        log_event: &LogEvent,
        transaction: &GoldRushTransaction,
        wallet_address: &str,
    ) -> Option<TokenTransfer> {
        
        // ERC20 Transfer event has 3 topics: signature, from, to
        if log_event.raw_log_topics.len() < 3 {
            return None;
        }
        
        let from_address_hex = &log_event.raw_log_topics[1];
        let to_address_hex = &log_event.raw_log_topics[2];
        
        // Extract 20-byte addresses from 32-byte topics (remove padding zeros)
        let from_address = from_address_hex.trim_start_matches("0x000000000000000000000000").to_lowercase();
        let to_address = to_address_hex.trim_start_matches("0x000000000000000000000000").to_lowercase();
        let wallet_lower = wallet_address.trim_start_matches("0x").to_lowercase();
        
        // Determine if this transfer involves our wallet
        let transfer_type = if from_address == wallet_lower {
            "OUT"
        } else if to_address == wallet_lower {
            "IN"
        } else {
            return None; // Transfer doesn't involve our wallet
        };
        
        // Parse the value from raw log data (first 32 bytes after removing 0x)
        let raw_log_data = log_event.raw_log_data.as_ref()?;
        let value_hex = if raw_log_data.len() >= 66 {
            &raw_log_data[2..66] // Remove 0x and take first 32 bytes (64 hex chars)
        } else {
            return None;
        };
        
        // Convert hex to decimal
        let value_wei = u128::from_str_radix(value_hex, 16).ok()?;
        
        // Use simple rate calculation from transaction value if available
        let (quote_rate, delta_quote) = if let Some(value_quote) = transaction.value_quote {
            // Convert Decimal to f64 for compatibility with TokenTransfer
            let rate_f64 = value_quote.to_string().parse::<f64>().unwrap_or(0.0);
            (Some(rate_f64), Some(rate_f64))
        } else {
            (None, None)
        };
        
        Some(TokenTransfer {
            block_signed_at: log_event.block_signed_at,
            tx_hash: log_event.tx_hash.clone(),
            from_address: format!("0x{}", from_address),
            from_address_label: None,
            to_address: format!("0x{}", to_address),
            to_address_label: None,
            contract_decimals: log_event.sender_contract_decimals,
            contract_name: log_event.sender_name.clone(),
            contract_ticker_symbol: log_event.sender_contract_ticker_symbol.clone(),
            contract_address: log_event.sender_address.clone(),
            logo_url: log_event.sender_logo_url.clone(),
            transfer_type: transfer_type.to_string(),
            delta: Some(value_wei.to_string()),
            balance: None,
            quote_rate,
            delta_quote,
            pretty_delta_quote: None,
            balance_quote: None,
            method_calls: None,
            explorers: Some(transaction.explorers.clone()),
        })
    }

}

// Clone implementation for JobOrchestrator
impl Clone for JobOrchestrator {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            birdeye_client: self.birdeye_client.clone(),
            goldrush_client: self.goldrush_client.clone(),
            persistence_client: self.persistence_client.clone(),
            running_jobs: self.running_jobs.clone(),
            running_token_analysis_jobs: self.running_token_analysis_jobs.clone(),
            instance_id: self.instance_id.clone(),
        }
    }
}

/// System status information
#[derive(Debug, Serialize, Deserialize)]
pub struct OrchestratorStatus {
    pub discovery_queue_size: u64,
    pub running_jobs_count: u64,
    pub batch_jobs_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pnl_job_creation() {
        let wallet = "test_wallet".to_string();
        let max_transactions = Some(500u32);

        let job = PnLJob::new(wallet.clone(), "solana".to_string(), max_transactions);
        assert_eq!(job.wallet_address, wallet);
        assert_eq!(job.status, JobStatus::Pending);
    }

    #[test]
    fn test_batch_job_creation() {
        let wallets = vec!["wallet1".to_string(), "wallet2".to_string()];
        let chain = "solana".to_string();
        let max_transactions = Some(500u32);

        let batch_job = BatchJob::new(wallets.clone(), chain.clone(), max_transactions);
        assert_eq!(batch_job.wallet_addresses, wallets);
        assert_eq!(batch_job.chain, chain);
        assert_eq!(batch_job.status, JobStatus::Pending);
    }
}