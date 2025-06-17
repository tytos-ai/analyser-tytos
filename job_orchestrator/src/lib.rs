use chrono::Utc;
use config_manager::SystemConfig;
use futures::future::join_all;
use jprice_client::JupiterPriceClient;
use persistence_layer::{PersistenceError, RedisClient};
use pnl_core::{AnalysisTimeframe, PnLEngine, PnLFilters, PnLReport};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use solana_client::{SolanaClient, SolanaClientConfig};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};
use tx_parser::{ParserConfig, TransactionParser};
use uuid::Uuid;

pub mod trending_orchestrator;
pub use trending_orchestrator::{TrendingOrchestrator, TrendingCycleStats};

#[derive(Error, Debug, Clone)]
pub enum OrchestratorError {
    #[error("Persistence error: {0}")]
    Persistence(String),
    #[error("P&L calculation error: {0}")]
    PnL(String),
    #[error("Solana client error: {0}")]
    SolanaClient(String),
    #[error("Transaction parser error: {0}")]
    TxParser(String),
    #[error("Jupiter client error: {0}")]
    JupiterClient(String),
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

impl From<solana_client::SolanaClientError> for OrchestratorError {
    fn from(err: solana_client::SolanaClientError) -> Self {
        OrchestratorError::SolanaClient(err.to_string())
    }
}

impl From<tx_parser::ParseError> for OrchestratorError {
    fn from(err: tx_parser::ParseError) -> Self {
        OrchestratorError::TxParser(err.to_string())
    }
}

impl From<jprice_client::JupiterClientError> for OrchestratorError {
    fn from(err: jprice_client::JupiterClientError) -> Self {
        OrchestratorError::JupiterClient(err.to_string())
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
}

/// Job orchestrator for managing P&L analysis tasks
pub struct JobOrchestrator {
    config: SystemConfig,
    solana_client: SolanaClient,
    jupiter_client: JupiterPriceClient,
    transaction_parser: TransactionParser,
    pnl_engine: PnLEngine<JupiterPriceClient>,
    redis_client: Arc<Mutex<RedisClient>>,
    running_jobs: Arc<Mutex<HashMap<Uuid, PnLJob>>>,
    batch_jobs: Arc<Mutex<HashMap<Uuid, BatchJob>>>,
}

impl JobOrchestrator {
    pub async fn new(config: SystemConfig) -> Result<Self> {
        // Initialize Solana client
        let solana_config = SolanaClientConfig {
            rpc_url: config.solana.rpc_url.clone(),
            rpc_timeout_seconds: config.solana.rpc_timeout_seconds,
            max_concurrent_requests: config.solana.max_concurrent_requests as usize,
            max_signatures: config.solana.max_signatures as u64,
        };
        let solana_client = SolanaClient::new(solana_config)?;

        // Initialize Redis client
        let redis_client = RedisClient::new(&config.redis.url).await?;
        let redis_client = Arc::new(Mutex::new(redis_client));

        // Initialize Jupiter client
        let jupiter_config = jprice_client::JupiterClientConfig {
            api_url: config.jupiter.api_url.clone(),
            request_timeout_seconds: config.jupiter.request_timeout_seconds,
            price_cache_ttl_seconds: config.jupiter.price_cache_ttl_seconds,
            max_retries: 3,
            rate_limit_delay_ms: 100,
        };
        let jupiter_client = JupiterPriceClient::new(jupiter_config, Some({
            let redis = redis_client.lock().await;
            redis.clone()
        })).await?;

        // Initialize transaction parser
        let parser_config = ParserConfig::default();
        let transaction_parser = TransactionParser::new(parser_config);

        // Initialize P&L engine
        let pnl_engine = PnLEngine::new(jupiter_client.clone());

        Ok(Self {
            config,
            solana_client,
            jupiter_client,
            transaction_parser,
            pnl_engine,
            redis_client,
            running_jobs: Arc::new(Mutex::new(HashMap::new())),
            batch_jobs: Arc::new(Mutex::new(HashMap::new())),
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

    /// Process wallets discovered by DexScreener
    async fn process_discovered_wallets(&self) -> Result<()> {
        let mut processed_count = 0;

        loop {
            // Pop a wallet from the discovery queue
            let wallet = {
                let redis = self.redis_client.lock().await;
                redis.pop_discovered_wallet(1).await?
            };

            let wallet_address = match wallet {
                Some(addr) => addr,
                None => {
                    debug!("No more wallets in discovery queue");
                    break;
                }
            };

            info!("Processing discovered wallet: {}", wallet_address);

            // Create P&L filters from configuration
            let filters = self.create_pnl_filters_from_config();

            // Process the wallet
            match self.process_single_wallet(&wallet_address, filters).await {
                Ok(report) => {
                    info!("Successfully processed wallet {}: P&L = {} USD", 
                          wallet_address, report.summary.total_pnl_usd);
                    processed_count += 1;
                }
                Err(e) => {
                    warn!("Failed to process wallet {}: {}", wallet_address, e);
                }
            }

            // Small delay between wallet processing
            sleep(Duration::from_millis(100)).await;
        }

        if processed_count > 0 {
            info!("Processed {} discovered wallets", processed_count);
        }

        Ok(())
    }

    /// Submit a batch P&L job
    pub async fn submit_batch_job(
        &self,
        wallet_addresses: Vec<String>,
        filters: Option<PnLFilters>,
    ) -> Result<Uuid> {
        let filters = filters.unwrap_or_else(|| self.create_pnl_filters_from_config());
        let batch_job = BatchJob::new(wallet_addresses, filters);
        let job_id = batch_job.id;

        // Store the batch job
        {
            let mut batch_jobs = self.batch_jobs.lock().await;
            batch_jobs.insert(job_id, batch_job);
        }

        // Process in background
        let orchestrator = self.clone();
        tokio::spawn(async move {
            if let Err(e) = orchestrator.execute_batch_job(job_id).await {
                error!("Batch job {} failed: {}", job_id, e);
            }
        });

        info!("Submitted batch job {} for {} wallets", job_id, 
              self.batch_jobs.lock().await.get(&job_id).unwrap().wallet_addresses.len());

        Ok(job_id)
    }

    /// Execute a batch job
    async fn execute_batch_job(&self, job_id: Uuid) -> Result<()> {
        let (wallet_addresses, filters) = {
            let mut batch_jobs = self.batch_jobs.lock().await;
            let job = batch_jobs.get_mut(&job_id).ok_or_else(|| {
                OrchestratorError::JobExecution(format!("Batch job {} not found", job_id))
            })?;

            job.status = JobStatus::Running;
            job.started_at = Some(Utc::now());

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

        // Update batch job with results
        {
            let mut batch_jobs = self.batch_jobs.lock().await;
            let job = batch_jobs.get_mut(&job_id).unwrap();

            job.status = JobStatus::Completed;
            job.completed_at = Some(Utc::now());

            for (wallet, result) in results {
                job.results.insert(wallet, result);
            }
        }

        let successful_count = {
            let batch_jobs = self.batch_jobs.lock().await;
            let job = batch_jobs.get(&job_id).unwrap();
            job.results.values().filter(|r| r.is_ok()).count()
        };

        info!("Batch job {} completed: {}/{} wallets successful", 
              job_id, successful_count, wallet_addresses.len());

        Ok(())
    }

    /// Get batch job status
    pub async fn get_batch_job_status(&self, job_id: Uuid) -> Option<BatchJob> {
        let batch_jobs = self.batch_jobs.lock().await;
        batch_jobs.get(&job_id).cloned()
    }

    /// Process a single wallet for P&L analysis
    async fn process_single_wallet(
        &self,
        wallet_address: &str,
        filters: PnLFilters,
    ) -> Result<PnLReport> {
        debug!("Starting P&L analysis for wallet: {}", wallet_address);

        // Fetch all transactions for the wallet
        let max_signatures = self.config.solana.max_signatures;
        let transactions = self
            .solana_client
            .get_all_transactions_for_address(wallet_address, Some(max_signatures as usize))
            .await?;

        if transactions.is_empty() {
            return Err(OrchestratorError::JobExecution(format!(
                "No transactions found for wallet: {}",
                wallet_address
            )));
        }

        // Parse transactions into financial events
        let events = self
            .transaction_parser
            .parse_transactions(&transactions, wallet_address)?;

        if events.is_empty() {
            return Err(OrchestratorError::JobExecution(format!(
                "No financial events found for wallet: {}",
                wallet_address
            )));
        }

        // Calculate P&L
        let report = self
            .pnl_engine
            .calculate_pnl(wallet_address, events, filters)
            .await?;

        debug!("P&L analysis completed for wallet: {}", wallet_address);

        Ok(report)
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
            max_signatures: Some(self.config.solana.max_signatures),
            timeframe_filter,
        }
    }

    /// Get system status
    pub async fn get_status(&self) -> Result<OrchestratorStatus> {
        // Try to get queue size with timeout, fallback to 0 if Redis unavailable
        let queue_size = {
            use tokio::time::{timeout, Duration};
            match timeout(Duration::from_millis(1000), async {
                let redis = self.redis_client.lock().await;
                redis.get_wallet_queue_size().await
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
        let batch_jobs_count = self.batch_jobs.lock().await.len();

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
            solana_client: self.solana_client.clone(),
            jupiter_client: self.jupiter_client.clone(),
            transaction_parser: TransactionParser::new(ParserConfig::default()),
            pnl_engine: PnLEngine::new(self.jupiter_client.clone()),
            redis_client: self.redis_client.clone(),
            running_jobs: self.running_jobs.clone(),
            batch_jobs: self.batch_jobs.clone(),
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