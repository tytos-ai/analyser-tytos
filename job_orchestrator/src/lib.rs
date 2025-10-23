use chrono::Utc;
use config_manager::{SystemConfig, normalize_chain_for_zerion, normalize_chain_for_birdeye};
use dex_client::{
    GeneralTraderTransaction,
    TokenTransactionSide,
    WalletTokenBalance,
};
use futures::future::join_all;
use persistence_layer::{
    DiscoveredWalletToken, JobStatus, PersistenceClient, PersistenceError, TokenAnalysisJob,
};
use zerion_client::{ZerionClient, ZerionError};
// New algorithm imports (primary P&L system)
use pnl_core::{
    NewFinancialEvent, NewPnLEngine, PortfolioPnLResult,
    ZerionBalanceFetcher,
};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{Mutex, Semaphore};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

// Legacy trending orchestrator module - kept for compatibility
pub mod birdeye_trending_orchestrator;

pub use birdeye_trending_orchestrator::{
    BirdEyeTrendingOrchestrator, DiscoveryStats, ProcessedSwap,
};

#[derive(Error, Debug, Clone)]
pub enum OrchestratorError {
    #[error("Persistence error: {0}")]
    Persistence(String),
    #[error("P&L calculation error: {0}")]
    PnL(String),
    #[error("Legacy price client error: {0}")]
    LegacyPrice(String),
    #[error("Zerion client error: {0}")]
    Zerion(String),
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
    #[error("Price fetch failed: {0}")]
    PriceFetchFailed(String),
    #[error("Historical enrichment failed: {0}")]
    EnrichmentFailed(String),
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


impl From<ZerionError> for OrchestratorError {
    fn from(err: ZerionError) -> Self {
        OrchestratorError::Zerion(err.to_string())
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
    // Failure tracking (backward compatible with serde default)
    #[serde(default)]
    pub successful_wallets: Vec<String>,
    #[serde(default)]
    pub failed_wallets: Vec<String>,
    #[serde(default)]
    pub error_summary: Option<String>,
    // Results stored in PostgreSQL, not in memory
}

impl BatchJob {
    pub fn new(
        wallet_addresses: Vec<String>,
        chain: String,
        time_range: Option<String>,
        max_transactions: Option<u32>,
    ) -> Self {
        // Store max_transactions and time_range in filters JSON for PostgreSQL storage
        let filters = serde_json::json!({
            "max_transactions": max_transactions,
            "time_range": time_range
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
            successful_wallets: Vec::new(),
            failed_wallets: Vec::new(),
            error_summary: None,
        }
    }

    /// Get max_transactions from filters JSON
    pub fn get_max_transactions(&self) -> Option<u32> {
        self.filters
            .get("max_transactions")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
    }

    pub fn get_time_range(&self) -> Option<String> {
        self.filters
            .get("time_range")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
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
            successful_wallets: self.successful_wallets.clone(),
            failed_wallets: self.failed_wallets.clone(),
            error_summary: self.error_summary.clone(),
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
            successful_wallets: persistent_job.successful_wallets,
            failed_wallets: persistent_job.failed_wallets,
            error_summary: persistent_job.error_summary,
        })
    }
}

/// Job orchestrator for managing P&L analysis tasks
pub struct JobOrchestrator {
    config: SystemConfig,
    zerion_client: Option<ZerionClient>,
    zerion_balance_fetcher: Option<ZerionBalanceFetcher>,
    birdeye_client: Option<dex_client::BirdEyeClient>,
    persistence_client: Arc<PersistenceClient>,
    running_jobs: Arc<Mutex<HashMap<Uuid, PnLJob>>>,
    running_token_analysis_jobs: Arc<Mutex<HashMap<Uuid, TokenAnalysisJob>>>,
    // batch_jobs stored in PostgreSQL via persistence_client
    instance_id: String, // Unique identifier for this orchestrator instance
    wallet_semaphore: Arc<Semaphore>, // Limit concurrent wallet processing to prevent resource exhaustion
}

impl JobOrchestrator {
    pub async fn new(
        config: SystemConfig,
        persistence_client: Arc<PersistenceClient>,
    ) -> Result<Self> {
        // Always initialize Zerion client if API key is provided
        let zerion_client = if !config.zerion.api_key.is_empty() {
            match ZerionClient::new(
                config.zerion.api_base_url.clone(),
                config.zerion.api_key.clone(),
                config.zerion.page_size,
                config.zerion.operation_types.clone(),
                config.zerion.chain_ids.clone(),
                config.zerion.trash_filter.clone(),
            ) {
                Ok(client) => {
                    info!("Zerion client initialized successfully");
                    Some(client)
                }
                Err(e) => {
                    error!("Failed to initialize Zerion client: {}", e);
                    return Err(OrchestratorError::Config(format!(
                        "Zerion client initialization failed: {}",
                        e
                    )));
                }
            }
        } else {
            error!("Zerion API key not provided - cannot initialize Zerion client");
            return Err(OrchestratorError::Config(
                "Zerion API key is required".to_string(),
            ));
        };

        // Initialize Zerion balance fetcher for EVM portfolio fetching
        let zerion_balance_fetcher = if !config.zerion.api_key.is_empty() {
            Some(ZerionBalanceFetcher::new(
                config.zerion.api_key.clone(),
                Some(config.zerion.api_base_url.clone()),
            ))
        } else {
            None
        };

        // Initialize BirdEye client for historical price enrichment
        let birdeye_client = if !config.birdeye.api_key.is_empty() {
            match dex_client::BirdEyeClient::new(config.birdeye.clone()) {
                Ok(client) => {
                    info!("BirdEye client initialized successfully for historical price enrichment");
                    Some(client)
                }
                Err(e) => {
                    warn!("Failed to initialize BirdEye client: {} - historical price enrichment will be unavailable", e);
                    None
                }
            }
        } else {
            warn!("BirdEye API key not provided - historical price enrichment will be unavailable");
            None
        };

        // Generate unique instance ID using hostname and process ID
        let instance_id = format!(
            "{}:{}:{}",
            hostname::get().unwrap_or_default().to_string_lossy(),
            std::process::id(),
            Uuid::new_v4().to_string()[..8].to_string() // First 8 chars of UUID for uniqueness
        );

        info!("Job orchestrator instance ID: {}", instance_id);

        Ok(Self {
            config,
            zerion_client,
            zerion_balance_fetcher,
            birdeye_client,
            persistence_client,
            running_jobs: Arc::new(Mutex::new(HashMap::new())),
            running_token_analysis_jobs: Arc::new(Mutex::new(HashMap::new())),
            instance_id,
            wallet_semaphore: Arc::new(Semaphore::new(5)), // Limit to 5 concurrent wallet processing tasks
        })
    }

    /// Get a reference to the Zerion client for direct API access
    pub fn get_zerion_client(&self) -> Option<&ZerionClient> {
        self.zerion_client.as_ref()
    }

    /// Get effective continuous mode filters
    async fn get_effective_continuous_filters(&self) -> Option<u32> {
        // Simple max_transactions filter for continuous mode - using Zerion default
        Some(self.config.zerion.default_max_transactions)
    }

    /// Convert Zerion token balances to WalletTokenBalance format for unified portfolio handling
    pub fn convert_zerion_to_wallet_token_balance(
        &self,
        zerion_balances: &HashMap<String, pnl_core::TokenBalance>,
        chain_id: &str,
    ) -> Vec<WalletTokenBalance> {
        zerion_balances
            .values()
            .map(|balance| WalletTokenBalance {
                address: balance.address.clone(),
                decimals: balance.decimals as u32,
                balance: balance.balance.to_string().parse().unwrap_or(0),
                ui_amount: balance.ui_amount.to_f64().unwrap_or(0.0),
                chain_id: chain_id.to_string(),
                name: Some(balance.name.clone()),
                symbol: Some(balance.symbol.clone()),
                icon: None, // Zerion TokenBalance doesn't include icon field
                logo_uri: None, // Zerion TokenBalance doesn't include logo_uri field
                price_usd: balance.price_usd.map(|p| p.to_f64().unwrap_or(0.0)).unwrap_or(0.0),
                value_usd: balance.value_usd.map(|v| v.to_f64().unwrap_or(0.0)).unwrap_or(0.0),
                multiplier: None, // Not applicable for Zerion balances
                is_scaled_ui_token: true, // Zerion provides scaled amounts
            })
            .collect()
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
            if let Err(e) = redis.cleanup_stale_processing_locks(600).await {
                // 10 minutes max age
                warn!("Failed to cleanup stale processing locks: {}", e);
            }
        }

        // Claim a batch of work for this instance
        let batch_size = self.config.system.pnl_parallel_batch_size.unwrap_or(10);
        let (claimed_batch, batch_id) = {
            let redis = &self.persistence_client;
            redis
                .claim_wallet_batch(&self.instance_id, batch_size)
                .await?
        };

        if claimed_batch.is_empty() {
            debug!("No work available for instance {}", self.instance_id);
            return Ok(());
        }

        info!(
            "Instance {} claimed batch {} with {} wallet-token pairs",
            self.instance_id,
            batch_id,
            claimed_batch.len()
        );

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
                if let Err(return_err) = redis.return_failed_batch(&batch_id, &claimed_batch).await
                {
                    error!(
                        "Failed to return failed batch {} to queue: {}",
                        batch_id, return_err
                    );
                }
            }
        }

        Ok(())
    }

    /// Process a claimed batch of wallet-token pairs
    async fn process_claimed_batch(
        &self,
        claimed_batch: &[DiscoveredWalletToken],
        batch_id: &str,
    ) -> Result<()> {
        info!(
            "🚀 Processing claimed batch {} with {} wallet-token pairs in parallel...",
            batch_id,
            claimed_batch.len()
        );

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
                    Ok(Ok((report, incomplete_count))) => {
                        // Store the rich P&L portfolio result for later retrieval
                        let store_result = {
                            let redis = &self.persistence_client;
                            redis.store_pnl_result_with_source(
                                &pair_clone.wallet_address,
                                &pair_clone.chain,
                                &report,
                                "continuous",
                                incomplete_count,
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
                    info!(
                        "✅ Successfully processed wallet {} for token {}: P&L = {} USD",
                        pair.wallet_address, pair.token_symbol, report.total_pnl_usd
                    );
                }
                Err(e) => {
                    failure_count += 1;
                    error!(
                        "❌ Failed to process wallet {} for token {}: {}",
                        pair.wallet_address, pair.token_symbol, e
                    );
                }
            }
        }

        info!(
            "Batch {} completed: {} successes, {} failures",
            batch_id, success_count, failure_count
        );

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
            debug!(
                "No work available for test cycle on instance {}",
                self.instance_id
            );
            return Ok(false);
        }

        debug!(
            "Test cycle claimed batch {} with {} wallet-token pair",
            batch_id,
            claimed_batch.len()
        );

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
                if let Err(return_err) = redis.return_failed_batch(&batch_id, &claimed_batch).await
                {
                    error!(
                        "Failed to return failed test batch {} to queue: {}",
                        batch_id, return_err
                    );
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
        time_range: Option<String>,
        max_transactions: Option<u32>,
    ) -> Result<Uuid> {
        // Normalize chain parameter for Zerion API compatibility
        let original_chain = chain.clone();
        let normalized_chain = normalize_chain_for_zerion(&chain)
            .map_err(|e| OrchestratorError::Config(e))?;

        if original_chain != normalized_chain {
            info!(
                "Chain normalized in job orchestrator: '{}' -> '{}'",
                original_chain, normalized_chain
            );
        }

        let batch_job = BatchJob::new(wallet_addresses.clone(), normalized_chain, time_range, max_transactions);
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
                if let Err(update_err) = orchestrator
                    .mark_batch_job_as_failed(job_id, &e.to_string())
                    .await
                {
                    error!(
                        "Failed to update batch job {} status to Failed: {}",
                        job_id, update_err
                    );
                }
            }
        });

        info!(
            "Submitted batch job {} for {} wallets",
            job_id,
            wallet_addresses.len()
        );

        Ok(job_id)
    }

    /// Execute a batch job
    async fn execute_batch_job(&self, job_id: Uuid) -> Result<()> {
        let start_time = std::time::Instant::now();
        info!("🚀 Starting batch job execution: {}", job_id);

        // Load job from PostgreSQL and update status to Running
        info!("📋 Loading batch job details from database...");
        let (wallet_addresses, chain, time_range, max_transactions) = {
            let persistence_client = &self.persistence_client;
            let persistent_job = persistence_client
                .get_batch_job(&job_id.to_string())
                .await?
                .ok_or_else(|| {
                    OrchestratorError::JobExecution(format!("Batch job {} not found", job_id))
                })?;

            let mut job = BatchJob::from_persistence_batch_job(persistent_job)?;
            info!(
                "✅ Job loaded successfully - Chain: {}, Wallets: {}, Max Transactions: {:?}",
                job.chain,
                job.wallet_addresses.len(),
                job.get_max_transactions()
            );

            job.status = JobStatus::Running;
            job.started_at = Some(Utc::now());
            info!("🔄 Updating job status to Running...");

            // Update status in PostgreSQL
            let updated_persistent_job = job.to_persistence_batch_job()?;
            persistence_client
                .update_batch_job(&updated_persistent_job)
                .await?;
            info!("✅ Job status updated in database");

            (
                job.wallet_addresses.clone(),
                job.chain.clone(),
                job.get_time_range(),
                job.get_max_transactions(),
            )
        };

        info!(
            "🎯 Starting batch job execution: {} for {} wallets on {} chain",
            job_id,
            wallet_addresses.len(),
            chain
        );

        // Process wallets in parallel with controlled concurrency (max 5 at a time)
        let wallet_count = wallet_addresses.len();
        info!(
            "⚡ Starting controlled parallel processing of {} wallets (max 5 concurrent, timeout: 10 minutes per wallet)...",
            wallet_count
        );

        // Create tasks vector to store spawned futures
        let mut tasks = Vec::new();

        // Process wallets with semaphore control
        for (index, wallet) in wallet_addresses.iter().enumerate() {
            let wallet_clone = wallet.clone();
            let chain_clone = chain.clone();
            let time_range_clone = time_range.clone();
            let semaphore = self.wallet_semaphore.clone();
            let self_clone = self.clone();

            let task = tokio::spawn(async move {
                // Acquire semaphore permit before processing
                let _permit = semaphore.acquire_owned().await.expect("Semaphore closed");

                let wallet_start_time = std::time::Instant::now();
                info!("🔄 Processing wallet {}/{}: {} on {} (acquired processing slot)", index + 1, wallet_count, wallet_clone, chain_clone);

                // Add timeout for each wallet processing (10 minutes max)
                let timeout_duration = Duration::from_secs(600);
                let result = match tokio::time::timeout(
                    timeout_duration,
                    self_clone.process_single_wallet(&wallet_clone, &chain_clone, time_range_clone.as_deref(), max_transactions)
                ).await {
                    Ok(Ok((report, incomplete_count))) => {
                        let elapsed = wallet_start_time.elapsed();
                        info!("✅ Wallet {}/{} completed successfully in {:.2}s: {} (Realized P&L: ${:.2}, Unrealized P&L: ${:.2}, {} incomplete trades)",
                              index + 1, wallet_count, elapsed.as_secs_f64(), wallet_clone,
                              report.total_realized_pnl_usd, report.total_unrealized_pnl_usd, incomplete_count);

                        // Store result immediately to PostgreSQL and drop the large report object
                        let persistence_client = &self_clone.persistence_client;
                        match persistence_client
                            .store_pnl_result_with_source(&wallet_clone, &chain_clone, &report, "batch", incomplete_count)
                            .await
                        {
                            Ok(_) => {
                                info!("✅ Successfully stored P&L result for wallet {} from batch job {}", wallet_clone, job_id);
                                // Drop report immediately to free memory (~2MB)
                                drop(report);
                                Ok(())  // Return unit type instead of full report
                            }
                            Err(e) => {
                                warn!("❌ Failed to store P&L result for wallet {}: {}", wallet_clone, e);
                                drop(report);  // Drop even on storage failure
                                Err(OrchestratorError::Persistence(e.to_string()))
                            }
                        }
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
                // Permit automatically dropped here, releasing slot
                (wallet_clone, result)
            });

            tasks.push(task);
        }

        info!(
            "⏳ Waiting for all {} wallet processing tasks to complete...",
            wallet_count
        );
        let task_results = join_all(tasks).await;

        // Unwrap JoinHandle results - now returns Result<()> instead of Result<PortfolioPnLResult>
        // Results are already stored to PostgreSQL, we just need success/failure tracking
        let results: Vec<(String, Result<()>)> = task_results
            .into_iter()
            .filter_map(|join_result| {
                match join_result {
                    Ok(result) => Some(result),
                    Err(e) => {
                        error!("Task join error: {}", e);
                        None
                    }
                }
            })
            .collect();

        let processing_elapsed = start_time.elapsed();
        info!(
            "🏁 All wallet processing completed in {:.2}s",
            processing_elapsed.as_secs_f64()
        );

        // Count successes and failures - results already stored to PostgreSQL by tasks
        info!("📊 Tallying batch job results...");
        let (successful_count, _successful_wallets, failed_wallets) = {
            let mut success_count = 0;
            let mut successful_wallets = Vec::new();
            let mut failed_wallets = Vec::new();

            let total_results = results.len();
            let successful_results = results.iter().filter(|(_, r)| r.is_ok()).count();
            info!(
                "📊 Processing {} results ({} successful, {} failed)",
                total_results,
                successful_results,
                total_results - successful_results
            );

            // Tally successes and failures (actual storage already happened in tasks)
            for (wallet, result) in &results {
                match result {
                    Ok(()) => {
                        success_count += 1;
                        successful_wallets.push(wallet.clone());
                    }
                    Err(e) => {
                        warn!("⚠️ Wallet {} failed: {}", wallet, e);
                        failed_wallets.push(wallet.clone());
                    }
                }
            }

            // Update batch job status in PostgreSQL with failure tracking
            let persistence_client = &self.persistence_client;
            let persistent_job = persistence_client
                .get_batch_job(&job_id.to_string())
                .await?
                .ok_or_else(|| {
                    OrchestratorError::JobExecution(format!("Batch job {} not found", job_id))
                })?;

            let mut job = BatchJob::from_persistence_batch_job(persistent_job)?;

            // Determine final status: Failed if all wallets failed, Completed otherwise
            if success_count == 0 && total_results > 0 {
                job.status = JobStatus::Failed;
                job.error_summary = Some(format!("All {} wallets failed to process", total_results));
                warn!("❌ Batch job {} marked as Failed: all wallets failed", job_id);
            } else {
                job.status = JobStatus::Completed;
                if !failed_wallets.is_empty() {
                    job.error_summary = Some(format!(
                        "{} of {} wallets failed to process",
                        failed_wallets.len(),
                        total_results
                    ));
                }
                info!("✅ Batch job {} marked as Completed: {}/{} wallets successful",
                      job_id, success_count, total_results);
            }

            job.completed_at = Some(Utc::now());
            job.successful_wallets = successful_wallets.clone();
            job.failed_wallets = failed_wallets.clone();

            // Update final status
            let updated_persistent_job = job.to_persistence_batch_job()?;
            persistence_client
                .update_batch_job(&updated_persistent_job)
                .await?;
            info!("✅ Batch job status updated in database with {} successful, {} failed wallets",
                  success_count, failed_wallets.len());

            (success_count, successful_wallets, failed_wallets)
        };

        // EXPLICIT CLEANUP - Free results vector now that we've tallied everything
        drop(results);  // Free lightweight result metadata

        let total_elapsed = start_time.elapsed();

        if successful_count == 0 && wallet_count > 0 {
            warn!("❌ Batch job {} FAILED in {:.2}s: All {} wallets failed processing",
                  job_id, total_elapsed.as_secs_f64(), wallet_count);
        } else if failed_wallets.is_empty() {
            info!("🎉 Batch job {} COMPLETED successfully in {:.2}s: All {}/{} wallets processed successfully",
                  job_id, total_elapsed.as_secs_f64(), successful_count, wallet_count);
        } else {
            warn!("⚠️ Batch job {} COMPLETED with failures in {:.2}s: {}/{} wallets successful ({:.1}% success rate), {} failed: {:?}",
                  job_id, total_elapsed.as_secs_f64(), successful_count, wallet_count,
                  (successful_count as f64 / wallet_count as f64) * 100.0,
                  failed_wallets.len(), failed_wallets);
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
                    match persistence
                        .get_portfolio_pnl_result(wallet_address, &batch_job.chain)
                        .await?
                    {
                        Some(stored_result) => {
                            results.push(stored_result.portfolio_result);
                        }
                        None => {
                            debug!(
                                "No P&L result found for wallet {} in batch {}",
                                wallet_address, job_id
                            );
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

            info!(
                "Marked batch job {} as Failed due to system error: {}",
                job_id, error_message
            );
        } else {
            warn!("Could not find batch job {} to mark as failed", job_id);
        }

        Ok(())
    }

    /// Get all batch jobs with pagination
    pub async fn get_all_batch_jobs(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<BatchJob>, usize)> {
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
    /// Returns (PortfolioPnLResult, incomplete_trades_count)
    pub async fn process_single_wallet_token_pair(
        &self,
        pair: &DiscoveredWalletToken,
        max_transactions: Option<u32>,
    ) -> Result<(PortfolioPnLResult, u32)> {
        debug!(
            "🎯 Starting targeted P&L analysis for wallet: {} on token: {} ({})",
            pair.wallet_address, pair.token_symbol, pair.token_address
        );

        // Use the same routing logic as batch jobs for consistency
        info!(
            "🔄 Using Zerion for continuous analysis of wallet: {}",
            pair.wallet_address
        );

        // Route based on chain
        let (report, incomplete_count) = match pair.chain.to_lowercase().as_str() {
            "solana" => {
                // Always use Zerion for Solana
                info!(
                    "🦋 Using Zerion for continuous analysis of wallet: {}",
                    pair.wallet_address
                );
                self.process_single_wallet_with_zerion(
                    &pair.wallet_address,
                    &pair.chain,
                    None, // time_range not supported in continuous mode yet
                    max_transactions,
                )
                .await?
            }
            _ => {
                error!(
                    "❌ Chain {} not supported. Only Solana is currently supported.",
                    pair.chain
                );
                return Err(OrchestratorError::Config(format!(
                    "Unsupported chain: {}",
                    pair.chain
                )));
            }
        };

        debug!(
            "✅ Targeted P&L analysis completed for wallet: {} on token: {} ({} incomplete trades)",
            pair.wallet_address, pair.token_symbol, incomplete_count
        );

        Ok((report, incomplete_count))
    }

    /// Process a single wallet for P&L analysis (using rich PortfolioPnLResult format)
    /// Returns (PortfolioPnLResult, incomplete_trades_count)
    pub async fn process_single_wallet(
        &self,
        wallet_address: &str,
        chain: &str,
        time_range: Option<&str>,
        max_transactions: Option<u32>,
    ) -> Result<(PortfolioPnLResult, u32)> {
        info!(
            "🔍 Starting P&L analysis for wallet: {} on chain: {} (max_txs: {:?})",
            wallet_address, chain, max_transactions
        );

        // Route to appropriate client based on chain - all chains now supported
        let result = match chain.to_lowercase().as_str() {
            "solana" => {
                // Use Zerion for Solana
                info!(
                    "🦋 Using Zerion for Solana P&L analysis of wallet: {}",
                    wallet_address
                );
                self.process_single_wallet_with_zerion(wallet_address, chain, time_range, max_transactions)
                    .await
            }
            "ethereum" | "base" | "bsc" | "binance-smart-chain" => {
                // Use Zerion-only for EVM chains (no BirdEye support)
                info!(
                    "🦋 Using Zerion-only for EVM chain {} P&L analysis of wallet: {}",
                    chain, wallet_address
                );
                self.process_single_wallet_with_zerion(wallet_address, chain, time_range, max_transactions)
                    .await
            }
            _ => {
                error!(
                    "❌ Unsupported chain: {}. Supported chains: solana, ethereum, base, bsc, binance-smart-chain",
                    chain
                );
                return Err(OrchestratorError::Config(format!(
                    "Unsupported chain: {}",
                    chain
                )));
            }
        };

        match &result {
            Ok((portfolio, incomplete_count)) => {
                info!("✅ P&L analysis completed for wallet: {} - Realized: ${:.2}, Unrealized: ${:.2}, {} tokens, {} incomplete trades",
                      wallet_address, portfolio.total_realized_pnl_usd,
                      portfolio.total_unrealized_pnl_usd, portfolio.token_results.len(), incomplete_count);
            }
            Err(e) => {
                warn!(
                    "❌ P&L analysis failed for wallet: {} - Error: {}",
                    wallet_address, e
                );
            }
        }

        result
    }


    /// Process a single wallet using Zerion transaction data + Zerion current portfolio
    /// Returns (PortfolioPnLResult, incomplete_trades_count)
    async fn process_single_wallet_with_zerion(
        &self,
        wallet_address: &str,
        chain: &str, // Chain to analyze (solana, ethereum, base, bsc)
        time_range: Option<&str>,
        max_transactions: Option<u32>,
    ) -> Result<(PortfolioPnLResult, u32)> {
        let step_start = std::time::Instant::now();
        info!(
            "🦋 Starting Zerion P&L analysis for wallet: {} on chain: {}",
            wallet_address, chain
        );

        let zerion_client = self.zerion_client.as_ref().ok_or_else(|| {
            OrchestratorError::Config("Zerion client not initialized".to_string())
        })?;

        let max_total_transactions =
            max_transactions.unwrap_or(self.config.zerion.default_max_transactions);
        info!(
            "📋 Configuration: Max transactions = {}, Zerion timeout = {}s",
            max_total_transactions, self.config.zerion.request_timeout_seconds
        );

        // Step 1: Fetch transaction history from Zerion (includes embedded historical prices!)
        info!("📥 Step 1/3: Fetching transaction history from Zerion API...");
        info!("🔄 Requesting up to {} transactions for wallet {} (trades + sends with embedded prices)",
               max_total_transactions, wallet_address);

        let history_start = std::time::Instant::now();
        let zerion_transactions = zerion_client
            .get_wallet_transactions_with_time_range(
                wallet_address,
                "usd",
                time_range,
                Some(max_total_transactions as usize),
                Some(chain),
            )
            .await?;
        let history_elapsed = history_start.elapsed();

        if let Some(time_range) = time_range {
            info!("✅ Step 1 completed in {:.2}s: Found {} transactions for wallet {} within {} time period with embedded prices",
                  history_elapsed.as_secs_f64(), zerion_transactions.len(), wallet_address, time_range);
        } else {
            info!("✅ Step 1 completed in {:.2}s: Found {} transactions for wallet {} with embedded prices",
                  history_elapsed.as_secs_f64(), zerion_transactions.len(), wallet_address);
        }

        if zerion_transactions.is_empty() {
            warn!(
                "❌ No transaction history found for wallet: {}",
                wallet_address
            );
            return Err(OrchestratorError::JobExecution(format!(
                "No transaction history found for wallet: {}",
                wallet_address
            )));
        }


        // Step 2: Fetch current prices for unrealized P&L
        info!("💲 Step 2a/3: Fetching current prices from BirdEye for unrealized P&L...");

        // Fetch current prices from BirdEye and convert f64 to Decimal
        // SCOPED BLOCK for early RAII cleanup of token_addresses vector
        let current_prices = {
            // Extract unique token addresses from transactions
            let mut token_addresses: Vec<String> = Vec::new();
            for tx in &zerion_transactions {
                for transfer in &tx.attributes.transfers {
                    if let Some(fungible_info) = &transfer.fungible_info {
                        // Get the token address for the current chain
                        for implementation in &fungible_info.implementations {
                            if implementation.chain_id.to_lowercase() == chain.to_lowercase() {
                                if let Some(address) = &implementation.address {
                                    if !token_addresses.contains(address) {
                                        token_addresses.push(address.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }

            info!("📊 Found {} unique tokens to fetch prices for", token_addresses.len());

            // Fetch prices and let token_addresses drop at end of this scope
            if let Some(birdeye_client) = &self.birdeye_client {
                match birdeye_client.get_current_prices(&token_addresses, &chain).await {
                    Ok(prices) => {
                        // Convert HashMap<String, f64> to HashMap<String, Decimal>
                        let decimal_prices: HashMap<String, Decimal> = prices
                            .into_iter()
                            .filter_map(|(addr, price)| {
                                Decimal::try_from(price).ok().map(|decimal_price| (addr, decimal_price))
                            })
                            .collect();
                        info!("✅ Successfully fetched {} current prices from BirdEye", decimal_prices.len());
                        decimal_prices
                    }
                    Err(e) => {
                        error!("❌ CRITICAL: BirdEye price fetch failed after all retries: {}", e);
                        return Err(OrchestratorError::PriceFetchFailed(
                            format!("BirdEye current prices unavailable: {}", e)
                        ));
                    }
                }
            } else {
                error!("❌ CRITICAL: BirdEye client not configured, cannot fetch current prices");
                return Err(OrchestratorError::Config(
                    "BirdEye client not available for price fetching".to_string()
                ));
            }
            // token_addresses dropped here automatically at end of scope (~KB freed)
        };

        // Step 2b: Calculate P&L using Zerion transactions
        // Note: Unrealized P&L now uses remaining positions from FIFO matching,
        // not external portfolio data. This ensures we only calculate P&L on analyzed tokens.
        info!(
            "🧮 Step 2b/3: Calculating P&L from {} Zerion transactions with {} current prices...",
            zerion_transactions.len(),
            current_prices.len()
        );

        let pnl_start = std::time::Instant::now();
        let (report, incomplete_trades_count) = self
            .calculate_pnl_with_zerion_transactions(
                wallet_address,
                zerion_transactions,
                current_prices,
                chain,
            )
            .await?;
        let pnl_elapsed = pnl_start.elapsed();

        // Note: token_addresses already dropped at end of scoped block above (~KB freed)
        // Note: current_prices already moved into calculate_pnl_with_zerion_transactions

        let total_elapsed = step_start.elapsed();
        info!(
            "✅ Step 2b completed in {:.2}s: P&L calculation finished",
            pnl_elapsed.as_secs_f64()
        );
        info!("🎉 Zerion P&L analysis completed for wallet {} on chain {} in {:.2}s total - Realized: ${:.2}, Unrealized: ${:.2}, {} tokens, {} incomplete trades",
              wallet_address, chain, total_elapsed.as_secs_f64(), report.total_realized_pnl_usd,
              report.total_unrealized_pnl_usd, report.token_results.len(), incomplete_trades_count);

        Ok((report, incomplete_trades_count))
    }

    /// Calculate P&L using Zerion transactions with embedded prices + Zerion current prices
    /// Returns (PortfolioPnLResult, incomplete_trades_count)
    async fn calculate_pnl_with_zerion_transactions(
        &self,
        wallet_address: &str,
        zerion_transactions: Vec<zerion_client::ZerionTransaction>,
        current_prices: HashMap<String, Decimal>,
        chain: &str, // Chain identifier for BirdEye historical price enrichment
    ) -> Result<(PortfolioPnLResult, u32)> {
        info!("🚀 Starting Zerion P&L calculation with {} current prices for wallet: {} with {} transactions",
              current_prices.len(), wallet_address, zerion_transactions.len());

        let zerion_client = self.zerion_client.as_ref().ok_or_else(|| {
            OrchestratorError::Config("Zerion client not initialized".to_string())
        })?;

        // Step 1: Convert Zerion transactions to financial events
        let conversion_result =
            zerion_client.convert_to_financial_events(&zerion_transactions, wallet_address);
        let mut financial_events = conversion_result.events;
        let incomplete_trades_count = conversion_result.incomplete_trades_count;

        info!("📊 Converted {} Zerion transactions into {} financial events (including send transactions), {} incomplete trades detected",
              zerion_transactions.len(), financial_events.len(), incomplete_trades_count);

        // Step 1.5: Extract and enrich skipped transactions using BirdEye historical prices
        let skipped_txs = zerion_client.extract_skipped_transaction_info(&zerion_transactions, wallet_address);
        if !skipped_txs.is_empty() {
            info!("🔍 Found {} transactions with missing price data - attempting BirdEye historical price enrichment",
                skipped_txs.len());

            match self.enrich_with_birdeye_historical_prices(&skipped_txs, chain).await {
                Ok(enriched_events) => {
                    if !enriched_events.is_empty() {
                        info!("✅ Successfully enriched {} transactions via BirdEye historical prices",
                            enriched_events.len());
                        financial_events.extend(enriched_events);
                        info!("📊 Total financial events after enrichment: {}", financial_events.len());
                    }
                }
                Err(e) => {
                    warn!("⚠️  BirdEye enrichment failed: {} - continuing with non-enriched data", e);
                }
            }
        }

        if financial_events.is_empty() {
            warn!(
                "❌ No financial events generated from Zerion transactions for wallet: {}",
                wallet_address
            );
            return Err(OrchestratorError::JobExecution(format!(
                "No financial events generated from Zerion transactions for wallet: {}",
                wallet_address
            )));
        }

        // Step 2: Group events by token for P&L processing
        let mut events_by_token: HashMap<String, Vec<NewFinancialEvent>> = HashMap::new();
        for event in financial_events {
            events_by_token
                .entry(event.token_address.clone())
                .or_insert_with(Vec::new)
                .push(event);
        }
        info!(
            "🔧 Grouped events into {} unique tokens for P&L calculation",
            events_by_token.len()
        );

        // Step 3: Calculate P&L using the new P&L engine with all events
        let pnl_engine = NewPnLEngine::new(wallet_address.to_string());

        info!(
            "💰 Calculating portfolio P&L for {} tokens",
            events_by_token.len()
        );

        // Step 4: Calculate portfolio totals using the engine's method
        let portfolio_result = pnl_engine
            .calculate_portfolio_pnl(events_by_token, Some(current_prices))
            .map_err(|e| {
                OrchestratorError::PnL(format!("Portfolio P&L calculation failed: {}", e))
            })?;

        // EXPLICIT CLEANUP - Free memory immediately after P&L calculation completes
        drop(zerion_transactions);  // Free original transaction data (~2MB)
        // Note: financial_events already moved into events_by_token loop (line 1372)
        // Note: events_by_token already consumed by calculate_portfolio_pnl
        // Note: current_prices already consumed by calculate_portfolio_pnl

        info!(
            "🎯 Portfolio P&L Summary: Realized ${:.2}, Unrealized ${:.2}, Total ${:.2}, {} incomplete trades",
            portfolio_result.total_realized_pnl_usd,
            portfolio_result.total_unrealized_pnl_usd,
            portfolio_result.total_pnl_usd,
            incomplete_trades_count
        );

        Ok((portfolio_result, incomplete_trades_count))
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
            })
            .await
            {
                Ok(Ok(size)) => size,
                Ok(Err(_)) => {
                    warn!("Redis unavailable for queue size check");
                    0
                }
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
            })
            .await
            {
                Ok(Ok(stats)) => stats.total_jobs,
                Ok(Err(_)) => {
                    warn!("Redis unavailable for batch jobs count");
                    0
                }
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
    pub fn consolidate_duplicate_hashes(
        transactions: Vec<GeneralTraderTransaction>,
    ) -> Vec<GeneralTraderTransaction> {
        use std::collections::HashMap;

        // Group transactions by tx_hash
        let mut tx_groups: HashMap<String, Vec<GeneralTraderTransaction>> = HashMap::new();
        for tx in transactions {
            tx_groups
                .entry(tx.tx_hash.clone())
                .or_insert_with(Vec::new)
                .push(tx);
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
                info!(
                    "🔄 Consolidating {} duplicate entries for tx_hash: {}",
                    entries.len(),
                    tx_hash
                );
                consolidated_transactions.push(Self::consolidate_duplicate_entries(entries));
            }
        }

        if duplicates_found > 0 {
            info!(
                "✅ Consolidated {} duplicate hash entries across {} unique transactions",
                duplicates_found,
                consolidated_transactions.len()
            );
        }

        consolidated_transactions
    }

    /// Consolidate multiple entries with the same tx_hash into a single net transaction
    /// This handles multi-step swaps by calculating net token flows and weighted average pricing
    pub fn consolidate_duplicate_entries(
        entries: Vec<GeneralTraderTransaction>,
    ) -> GeneralTraderTransaction {
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
            if net_flow.abs() < 0.000001 {
                continue;
            } // Skip dust amounts

            if net_flow < 0.0 {
                if primary_outflow.is_none()
                    || net_flow.abs() > primary_outflow.as_ref().unwrap().1.abs()
                {
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
        let total_volume_usd = entries.iter().map(|e| e.volume_usd).sum::<f64>();

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

    pub async fn submit_token_analysis_job(
        &self,
        token_addresses: Vec<String>,
        chain: String,
        max_transactions: Option<u32>,
    ) -> Result<Uuid> {
        // Normalize chain parameter for Zerion API compatibility
        let original_chain = chain.clone();
        let normalized_chain = normalize_chain_for_zerion(&chain)
            .map_err(|e| OrchestratorError::Config(e))?;

        if original_chain != normalized_chain {
            info!(
                "Chain normalized in token analysis job: '{}' -> '{}'",
                original_chain, normalized_chain
            );
        }

        let job = TokenAnalysisJob::new(token_addresses.clone(), normalized_chain.clone(), max_transactions);
        let job_id = job.id;

        info!(
            "🚀 Starting token analysis job {} for {} tokens on {}",
            job_id,
            token_addresses.len(),
            normalized_chain
        );

        // Store job in persistence and memory
        self.persistence_client
            .store_token_analysis_job(&job)
            .await
            .map_err(|e| OrchestratorError::Persistence(e.to_string()))?;

        {
            let mut running_jobs = self.running_token_analysis_jobs.lock().await;
            running_jobs.insert(job_id, job);
        }

        // Spawn background task for token analysis
        let orchestrator_clone = self.clone();
        let job_id_clone = job_id;
        tokio::spawn(async move {
            if let Err(e) = orchestrator_clone
                .process_token_analysis_job(job_id_clone)
                .await
            {
                error!("Token analysis job {} failed: {}", job_id_clone, e);

                // Update job status to failed
                let mut running_jobs = orchestrator_clone.running_token_analysis_jobs.lock().await;
                if let Some(job) = running_jobs.get_mut(&job_id_clone) {
                    job.status = JobStatus::Failed;
                    job.completed_at = Some(Utc::now());

                    // Update in persistence
                    if let Err(persist_err) = orchestrator_clone
                        .persistence_client
                        .update_token_analysis_job(job)
                        .await
                    {
                        error!(
                            "Failed to update failed job {} in persistence: {}",
                            job_id_clone, persist_err
                        );
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
            let job = running_jobs.get_mut(&job_id).ok_or_else(|| {
                OrchestratorError::JobExecution(format!("Token analysis job {} not found", job_id))
            })?;

            job.status = JobStatus::Running;
            job.started_at = Some(Utc::now());

            // Update in persistence
            self.persistence_client
                .update_token_analysis_job(job)
                .await
                .map_err(|e| OrchestratorError::Persistence(e.to_string()))?;

            (
                job.token_addresses.clone(),
                job.chain.clone(),
                job.get_max_transactions(),
            )
        };

        let discovered_wallet_addresses = std::collections::HashSet::new();

        // Step 1: Discover top traders for each token
        // Top traders discovery now uses DexScreener scraping (not implemented in this method yet)
        // For now, token analysis will be done without trader discovery
        warn!("⚠️ Top traders discovery not implemented for token analysis yet - proceeding without trader discovery");

        let discovered_wallets: Vec<String> = discovered_wallet_addresses.into_iter().collect();
        info!(
            "🎯 Discovered {} unique wallets across {} tokens",
            discovered_wallets.len(),
            token_addresses.len()
        );

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
            let futures: Vec<_> = wallet_chunk
                .iter()
                .map(|wallet| {
                    let wallet = wallet.clone();
                    let chain = chain.clone();
                    let max_transactions = max_transactions;
                    async move {
                        let result = self
                            .process_single_wallet_for_token_analysis(
                                &wallet,
                                &chain,
                                max_transactions,
                            )
                            .await;
                        (wallet, result)
                    }
                })
                .collect();

            let results = join_all(futures).await;

            for (wallet, result) in results {
                match result {
                    Ok(_) => {
                        analyzed_wallets.push(wallet);
                    }
                    Err(e) => {
                        warn!(
                            "Failed to analyze wallet {} for token analysis: {}",
                            wallet, e
                        );
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
                    error!(
                        "Failed to update completed job {} in persistence: {}",
                        job_id, e
                    );
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

        info!(
            "✅ Token analysis job {} completed: {}/{} wallets successfully analyzed",
            job_id,
            analyzed_wallets.len(),
            discovered_wallets.len()
        );

        Ok(())
    }

    /// Process a single wallet for token analysis
    async fn process_single_wallet_for_token_analysis(
        &self,
        _wallet_address: &str,
        _chain: &str,
        _max_transactions: Option<u32>,
    ) -> Result<()> {
        // Token analysis is temporarily disabled due to API changes
        // This would need to be reimplemented using Zerion data conversion
        warn!("⚠️ Token analysis temporarily disabled - needs Zerion implementation");
        return Err(OrchestratorError::JobExecution(
            "Token analysis not available - needs Zerion implementation".to_string(),
        ));
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
        match self
            .persistence_client
            .get_token_analysis_job(&job_id.to_string())
            .await
        {
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
    pub async fn get_all_token_analysis_jobs_from_persistence(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<TokenAnalysisJob>, usize)> {
        self.persistence_client
            .get_all_token_analysis_jobs(limit, offset)
            .await
            .map_err(|e| OrchestratorError::Persistence(e.to_string()))
    }

    /// Enrich skipped transactions using BirdEye historical price API
    /// This method fetches historical prices for transactions that had null price/value in Zerion
    async fn enrich_with_birdeye_historical_prices(
        &self,
        skipped: &[zerion_client::SkippedTransactionInfo],
        chain: &str, // Chain parameter for BirdEye API
    ) -> Result<Vec<pnl_core::NewFinancialEvent>> {
        use pnl_core::NewFinancialEvent;
        use rust_decimal::Decimal;

        let birdeye_client = self.birdeye_client.as_ref()
            .ok_or_else(|| OrchestratorError::Config("BirdEye client not initialized".to_string()))?;

        // Normalize chain for BirdEye API (binance-smart-chain -> bsc)
        let birdeye_chain = normalize_chain_for_birdeye(chain)
            .map_err(|e| OrchestratorError::Config(e))?;

        let mut enriched_events = Vec::new();
        let mut success_count = 0u32;
        let mut failure_count = 0u32;

        info!("🔄 Fetching historical prices for {} tokens from BirdEye on chain {}...", skipped.len(), birdeye_chain);

        for skip_info in skipped {
            info!("🔍 Looking up historical price: {} ({}) at timestamp {} on chain {}",
                skip_info.token_symbol, skip_info.token_mint, skip_info.unix_timestamp, birdeye_chain);

            match birdeye_client.get_historical_price_unix(
                &skip_info.token_mint,
                skip_info.unix_timestamp,
                Some(&birdeye_chain),
            ).await {
                Ok(historical_price) => {
                    info!("✅ Found historical price for {}: ${}",
                        skip_info.token_symbol, historical_price);

                    // Convert price to Decimal
                    let usd_price = Decimal::from_f64_retain(historical_price)
                        .unwrap_or(Decimal::ZERO);

                    // Calculate USD value
                    let usd_value = skip_info.token_amount * usd_price;

                    // Create financial event with enriched price data
                    let event = NewFinancialEvent {
                        wallet_address: skip_info.wallet_address.clone(),
                        token_address: skip_info.token_mint.clone(),
                        token_symbol: skip_info.token_symbol.clone(),
                        chain_id: skip_info.chain_id.clone(),
                        event_type: skip_info.event_type.clone(),
                        quantity: skip_info.token_amount,
                        usd_price_per_token: usd_price,
                        usd_value: usd_value,
                        timestamp: skip_info.timestamp,
                        transaction_hash: skip_info.tx_hash.clone(),
                    };

                    info!("✨ Enriched event: {} {} @ ${} = ${}",
                        event.quantity, event.token_symbol, usd_price, usd_value);

                    enriched_events.push(event);
                    success_count += 1;
                }
                Err(e) => {
                    warn!("❌ Failed to fetch historical price for {}: {}",
                        skip_info.token_symbol, e);
                    failure_count += 1;
                }
            }

            // Rate limiting: BirdEye free tier = 100 req/min
            // Sleep 1.2 seconds between requests = ~50 req/min (safe margin)
            tokio::time::sleep(tokio::time::Duration::from_millis(1200)).await;
        }

        info!("📊 BirdEye enrichment results: {} successful, {} failed",
            success_count, failure_count);

        // Fail if more than 50% of enrichment attempts failed
        let total_attempts = success_count + failure_count;
        if total_attempts > 0 {
            let failure_rate = (failure_count as f64) / (total_attempts as f64);
            if failure_rate > 0.5 {
                error!("❌ CRITICAL: Historical enrichment failure rate too high: {:.1}% ({}/{})",
                    failure_rate * 100.0, failure_count, total_attempts);
                return Err(OrchestratorError::EnrichmentFailed(
                    format!("More than 50% of historical price fetches failed ({}/{})", failure_count, total_attempts)
                ));
            }
        }

        Ok(enriched_events)
    }
}

// Clone implementation for JobOrchestrator
impl Clone for JobOrchestrator {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            zerion_client: self.zerion_client.clone(),
            zerion_balance_fetcher: self.zerion_balance_fetcher.clone(),
            birdeye_client: self.birdeye_client.clone(),
            persistence_client: self.persistence_client.clone(),
            running_jobs: self.running_jobs.clone(),
            running_token_analysis_jobs: self.running_token_analysis_jobs.clone(),
            instance_id: self.instance_id.clone(),
            wallet_semaphore: self.wallet_semaphore.clone(),
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

