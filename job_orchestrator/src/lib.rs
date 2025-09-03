use chrono::Utc;
use config_manager::SystemConfig;
use futures::future::join_all;
use dex_client::{BirdEyeClient, BirdEyeError, GeneralTraderTransaction, TokenTransactionSide};
use persistence_layer::{PersistenceError, PersistenceClient, DiscoveredWalletToken, TokenAnalysisJob, JobStatus};
// New algorithm imports (primary P&L system)
use pnl_core::{NewTransactionParser, NewPnLEngine, PortfolioPnLResult, NewFinancialEvent, BalanceFetcher};
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
    pub max_transactions: Option<u32>, // Simple filter - max transactions to fetch
    pub result: Option<PortfolioPnLResult>,
}

impl PnLJob {
    pub fn new(wallet_address: String, max_transactions: Option<u32>) -> Self {
        Self {
            id: Uuid::new_v4(),
            wallet_address,
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
        // Load job from PostgreSQL and update status to Running
        let (wallet_addresses, chain, max_transactions) = {
            let persistence_client = &self.persistence_client;
            let persistent_job = persistence_client.get_batch_job(&job_id.to_string()).await?
                .ok_or_else(|| OrchestratorError::JobExecution(format!("Batch job {} not found", job_id)))?;
            
            let mut job = BatchJob::from_persistence_batch_job(persistent_job)?;
            job.status = JobStatus::Running;
            job.started_at = Some(Utc::now());

            // Update status in PostgreSQL
            let updated_persistent_job = job.to_persistence_batch_job()?;
            persistence_client.update_batch_job(&updated_persistent_job).await?;

            (job.wallet_addresses.clone(), job.chain.clone(), job.get_max_transactions())
        };

        info!("Executing batch job {} for {} wallets", job_id, wallet_addresses.len());

        // Process wallets in parallel with timeout
        let futures = wallet_addresses.iter().map(|wallet| {
            let wallet_clone = wallet.clone();
            let chain_clone = chain.clone();
            async move {
                // Add timeout for each wallet processing (10 minutes max)
                let timeout_duration = Duration::from_secs(600);
                let result = match tokio::time::timeout(
                    timeout_duration, 
                    self.process_single_wallet(&wallet_clone, &chain_clone, max_transactions)
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

        // Store individual P&L results in main PostgreSQL table and batch results
        let successful_count = {
            let mut success_count = 0;
            
            // Store each successful wallet's rich P&L result in main pnl_results table
            for (wallet, result) in &results {
                if let Ok(portfolio_result) = result {
                    // Store rich portfolio result in main table for individual wallet queries
                    let persistence_client = &self.persistence_client;
                    // Use the chain from the batch job
                    match persistence_client.store_pnl_result_with_source(wallet, &chain, portfolio_result, "batch").await {
                        Ok(_) => {
                            debug!("Stored rich P&L result for wallet {} from batch job {}", wallet, job_id);
                            success_count += 1;
                        }
                        Err(e) => {
                            warn!("Failed to store P&L result for wallet {} from batch job {}: {}", wallet, job_id, e);
                        }
                    }
                }
            }
            
            // Update batch job status to completed in PostgreSQL
            let persistence_client = &self.persistence_client;
            let persistent_job = persistence_client.get_batch_job(&job_id.to_string()).await?
                .ok_or_else(|| OrchestratorError::JobExecution(format!("Batch job {} not found", job_id)))?;
            
            let mut job = BatchJob::from_persistence_batch_job(persistent_job)?;
            job.status = JobStatus::Completed;
            job.completed_at = Some(Utc::now());

            // Note: Individual P&L results are already stored in pnl_results table above
            // No need to store results in batch job - they're retrieved from PostgreSQL when needed

            // Update final status
            let updated_persistent_job = job.to_persistence_batch_job()?;
            persistence_client.update_batch_job(&updated_persistent_job).await?;

            success_count
        };

        info!("Batch job {} completed: {}/{} wallets successful", 
              job_id, successful_count, wallet_addresses.len());

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
        debug!("Using BirdEye for P&L analysis of wallet: {}", wallet_address);
        self.process_single_wallet_with_birdeye(wallet_address, chain, max_transactions).await
    }

    /// Process a single wallet using BirdEye data
    async fn process_single_wallet_with_birdeye(
        &self,
        wallet_address: &str,
        chain: &str,
        max_transactions: Option<u32>,
    ) -> Result<PortfolioPnLResult> {
        debug!("Starting P&L analysis for wallet: {} using BirdEye API", wallet_address);

        // Fetch all trading transactions for the wallet using BirdEye with pagination
        let max_total_transactions = max_transactions
            .unwrap_or(self.config.birdeye.default_max_transactions);
        
        // No time bounds filtering - get all transactions (filter in frontend)
        debug!("Fetching up to {} transactions for wallet {}", 
               max_total_transactions, wallet_address);
        
        let transactions = self
            .birdeye_client
            .get_all_trader_transactions_paginated(wallet_address, chain, None, None, max_total_transactions)
            .await?;

        if transactions.is_empty() {
            return Err(OrchestratorError::JobExecution(format!(
                "No transactions found for wallet: {}",
                wallet_address
            )));
        }

        info!("📊 Found {} BirdEye transactions for wallet {}", 
              transactions.len(), wallet_address);

        // --- Start of New P&L Algorithm ---
        // Use the new algorithm strictly following the documentation
        let report = self.calculate_pnl_with_new_algorithm(wallet_address, chain, transactions).await?;
        // --- End of New P&L Algorithm ---

        debug!("✅ P&L analysis completed for wallet: {} using BirdEye data", wallet_address);

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
    fn consolidate_duplicate_hashes(transactions: Vec<GeneralTraderTransaction>) -> Vec<GeneralTraderTransaction> {
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
    fn consolidate_duplicate_entries(entries: Vec<GeneralTraderTransaction>) -> GeneralTraderTransaction {
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

    /// Calculate P&L using the new algorithm as specified in the documentation
    /// This method strictly follows the algorithm description in pnl_algorithm_documentation.md
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

        let parser = NewTransactionParser::new(wallet_address.to_string());
        let events = parser.parse_transactions(transactions).await
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

}

// Clone implementation for JobOrchestrator
impl Clone for JobOrchestrator {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            birdeye_client: self.birdeye_client.clone(),
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
        let filters = PnLFilters {
            min_capital_sol: Decimal::ZERO,
            min_hold_minutes: Decimal::ZERO,
            min_trades: 1,
            min_win_rate: Decimal::ZERO,
            max_signatures: None,
            max_transactions_to_fetch: None,
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
            max_transactions_to_fetch: None,
            timeframe_filter: None,
        };

        let batch_job = BatchJob::new(wallets.clone(), filters);
        assert_eq!(batch_job.wallet_addresses, wallets);
        assert_eq!(batch_job.status, JobStatus::Pending);
    }
}