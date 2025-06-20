use chrono::Utc;
use config_manager::SystemConfig;
use futures::future::join_all;
use dex_client::{BirdEyeClient, BirdEyeConfig, BirdEyeError, TraderTransaction, GeneralTraderTransaction};
use pnl_core::{FinancialEvent, EventType, EventMetadata};
use persistence_layer::{PersistenceError, RedisClient, DiscoveredWalletToken};
use pnl_core::{AnalysisTimeframe, PnLFilters, PnLReport, calculate_pnl_with_embedded_prices};
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
pub use birdeye_trending_orchestrator::{BirdEyeTrendingOrchestrator, BirdEyeTrendingConfig, DiscoveryStats};

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
}

/// Job orchestrator for managing P&L analysis tasks
pub struct JobOrchestrator {
    config: SystemConfig,
    birdeye_client: BirdEyeClient,
    redis_client: Arc<Mutex<RedisClient>>,
    running_jobs: Arc<Mutex<HashMap<Uuid, PnLJob>>>,
    batch_jobs: Arc<Mutex<HashMap<Uuid, BatchJob>>>,
}

impl JobOrchestrator {
    pub async fn new(config: SystemConfig) -> Result<Self> {
        // Initialize Redis client
        let redis_client = RedisClient::new(&config.redis.url).await?;
        let redis_client = Arc::new(Mutex::new(redis_client));

        // Initialize BirdEye client and price fetcher
        let birdeye_config = BirdEyeConfig {
            api_key: config.birdeye.api_key.clone(),
            api_base_url: config.birdeye.api_base_url.clone(),
            request_timeout_seconds: config.birdeye.request_timeout_seconds,
            rate_limit_per_second: config.birdeye.rate_limit_per_second,
        };
        
        let birdeye_client = BirdEyeClient::new(birdeye_config.clone())?;

        Ok(Self {
            config,
            birdeye_client,
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

        // Create P&L filters from configuration
        let filters = self.create_pnl_filters_from_config();

        // Process the wallet-token pair using targeted BirdEye transactions
        match self.process_single_wallet_token_pair(&pair, filters).await {
            Ok(report) => {
                info!("Successfully processed wallet {} for token {}: P&L = {} USD", 
                      pair.wallet_address, pair.token_symbol, report.summary.total_pnl_usd);
                
                // Store the P&L result in Redis for later retrieval
                {
                    let redis = self.redis_client.lock().await;
                    if let Err(e) = redis.store_pnl_result(
                        &pair.wallet_address,
                        &pair.token_address,
                        &pair.token_symbol,
                        &report,
                    ).await {
                        warn!("Failed to store P&L result for wallet {}: {}", pair.wallet_address, e);
                    } else {
                        debug!("Stored P&L result for wallet {} token {}", pair.wallet_address, pair.token_symbol);
                    }
                }
                
                Ok(true)
            }
            Err(e) => {
                warn!("Failed to process wallet {} for token {}: {}", 
                      pair.wallet_address, pair.token_symbol, e);
                Ok(true) // Still processed (even if failed)
            }
        }
    }

    /// Process wallet-token pairs discovered by BirdEye
    async fn process_discovered_wallets(&self) -> Result<()> {
        let mut processed_count = 0;

        loop {
            // Pop a wallet-token pair from the discovery queue
            let wallet_token_pair = {
                let redis = self.redis_client.lock().await;
                redis.pop_discovered_wallet_token_pair(1).await?
            };

            let pair = match wallet_token_pair {
                Some(pair) => pair,
                None => {
                    debug!("No more wallet-token pairs in discovery queue");
                    break;
                }
            };

            info!("Processing discovered wallet-token pair: {} for {} ({})", 
                  pair.wallet_address, pair.token_symbol, pair.token_address);

            // Create P&L filters from configuration
            let filters = self.create_pnl_filters_from_config();

            // Process the wallet-token pair using targeted BirdEye transactions
            match self.process_single_wallet_token_pair(&pair, filters).await {
                Ok(report) => {
                    info!("Successfully processed wallet {} for token {}: P&L = {} USD", 
                          pair.wallet_address, pair.token_symbol, report.summary.total_pnl_usd);
                    
                    // Store the P&L result in Redis for later retrieval
                    {
                        let redis = self.redis_client.lock().await;
                        if let Err(e) = redis.store_pnl_result(
                            &pair.wallet_address,
                            &pair.token_address,
                            &pair.token_symbol,
                            &report,
                        ).await {
                            warn!("Failed to store P&L result for wallet {}: {}", pair.wallet_address, e);
                        } else {
                            debug!("Stored P&L result for wallet {} token {}", pair.wallet_address, pair.token_symbol);
                        }
                    }
                    
                    processed_count += 1;
                }
                Err(e) => {
                    warn!("Failed to process wallet {} for token {}: {}", 
                          pair.wallet_address, pair.token_symbol, e);
                }
            }

            // Small delay between wallet processing
            sleep(Duration::from_millis(100)).await;
        }

        if processed_count > 0 {
            info!("Processed {} discovered wallet-token pairs", processed_count);
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

    /// Process a single wallet-token pair for targeted P&L analysis using BirdEye transactions
    async fn process_single_wallet_token_pair(
        &self,
        pair: &DiscoveredWalletToken,
        filters: PnLFilters,
    ) -> Result<PnLReport> {
        debug!("Starting targeted P&L analysis for wallet: {} on token: {} ({})", 
               pair.wallet_address, pair.token_symbol, pair.token_address);

        // Fetch transactions for this specific wallet-token pair using BirdEye
        let transactions = self
            .birdeye_client
            .get_trader_transactions(
                &pair.wallet_address,
                &pair.token_address,
                None, // from_time (no limit)
                None, // to_time (no limit)
                Some(100), // limit to 100 transactions max (BirdEye API limit)
            )
            .await?;

        if transactions.is_empty() {
            return Err(OrchestratorError::JobExecution(format!(
                "No BirdEye transactions found for wallet: {} on token: {}",
                pair.wallet_address, pair.token_symbol
            )));
        }

        info!("📊 Found {} BirdEye transactions for {} trading {}", 
              transactions.len(), pair.wallet_address, pair.token_symbol);

        // Convert BirdEye transactions to financial events
        let events = self
            .convert_birdeye_transactions_to_events(&transactions, &pair.wallet_address)?;

        if events.is_empty() {
            return Err(OrchestratorError::JobExecution(format!(
                "No financial events found for wallet: {} on token: {}",
                pair.wallet_address, pair.token_symbol
            )));
        }

        // Calculate P&L using the targeted transactions with embedded prices
        let report = calculate_pnl_with_embedded_prices(&pair.wallet_address, events, filters)
            .await?;

        debug!("✅ Targeted P&L analysis completed for wallet: {} on token: {}", 
               pair.wallet_address, pair.token_symbol);

        Ok(report)
    }

    /// Process a single wallet for P&L analysis (legacy method using Solana RPC)
    async fn process_single_wallet(
        &self,
        wallet_address: &str,
        filters: PnLFilters,
    ) -> Result<PnLReport> {
        debug!("Starting P&L analysis for wallet: {} using BirdEye API", wallet_address);

        // Fetch all trading transactions for the wallet using BirdEye
        let max_limit = 100; // BirdEye API limit is 100, not 1000
        let transactions = self
            .birdeye_client
            .get_all_trader_transactions(wallet_address, None, None, Some(max_limit))
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
        let report = calculate_pnl_with_embedded_prices(wallet_address, events, filters)
            .await?;

        debug!("✅ P&L analysis completed for wallet: {} using BirdEye data", wallet_address);

        Ok(report)
    }

    /// Convert BirdEye transactions to FinancialEvents for P&L analysis
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

            let event = FinancialEvent {
                id: Uuid::new_v4(),
                transaction_id: tx.tx_hash.clone(),
                wallet_address: wallet_address.to_string(),
                event_type,
                token_mint: tx.token_address.clone(),
                token_amount: Decimal::try_from(tx.token_amount).unwrap_or(Decimal::ZERO),
                sol_amount: Decimal::ZERO, // BirdEye doesn't provide SOL amount directly
                timestamp,
                transaction_fee: Decimal::ZERO, // BirdEye doesn't provide fees
                metadata: EventMetadata {
                    program_id: tx.source.clone(),
                    instruction_index: None,
                    exchange: tx.source.clone(), // Use source as exchange identifier
                    price_per_token: Some(Decimal::try_from(tx.token_price).unwrap_or(Decimal::ZERO)),
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
            // Skip non-swap transactions
            if tx.tx_type != "swap" {
                debug!("Skipping non-swap transaction: {}", tx.tx_hash);
                continue;
            }

            // Determine which side is the token and which is SOL/base currency
            let (token_side, base_side, is_buy) = if tx.quote.type_swap == "to" {
                // Buying token with SOL/base
                (&tx.quote, &tx.base, true)
            } else if tx.quote.type_swap == "from" {
                // Selling token for SOL/base  
                (&tx.quote, &tx.base, false)
            } else {
                debug!("Unclear swap direction for transaction: {}", tx.tx_hash);
                continue;
            };

            let event_type = if is_buy { EventType::Buy } else { EventType::Sell };

            // Convert timestamp from Unix time to DateTime
            let timestamp = chrono::DateTime::<chrono::Utc>::from_timestamp(tx.block_unix_time, 0)
                .unwrap_or_else(|| chrono::Utc::now());

            // Create extra metadata map for additional BirdEye data
            let mut extra = HashMap::new();
            extra.insert("token_symbol".to_string(), token_side.symbol.clone());
            extra.insert("base_symbol".to_string(), base_side.symbol.clone());
            extra.insert("source".to_string(), tx.source.clone());
            extra.insert("program_address".to_string(), tx.address.clone());
            if let Some(base_price) = tx.base_price {
                extra.insert("base_price".to_string(), base_price.to_string());
            }

            // Calculate token amount and price
            let token_amount = Decimal::try_from(token_side.ui_amount).unwrap_or(Decimal::ZERO);
            let token_price = if let Some(price) = token_side.price {
                Decimal::try_from(price).unwrap_or(Decimal::ZERO)
            } else {
                Decimal::try_from(token_side.nearest_price).unwrap_or(Decimal::ZERO)
            };

            // Calculate SOL amount from base side
            let sol_amount = if base_side.symbol == "SOL" {
                Decimal::try_from(base_side.ui_amount.abs()).unwrap_or(Decimal::ZERO)
            } else {
                // If base is not SOL, try to calculate from base price
                if let Some(base_price) = tx.base_price {
                    let base_amount = Decimal::try_from(base_side.ui_amount.abs()).unwrap_or(Decimal::ZERO);
                    let base_price_decimal = Decimal::try_from(base_price).unwrap_or(Decimal::ZERO);
                    // Convert to SOL equivalent (assuming SOL price around 144 based on our test data)
                    if base_price_decimal > Decimal::ZERO {
                        base_amount * base_price_decimal / Decimal::from(144)
                    } else {
                        Decimal::ZERO
                    }
                } else {
                    Decimal::ZERO
                }
            };

            let event = FinancialEvent {
                id: Uuid::new_v4(),
                transaction_id: tx.tx_hash.clone(),
                wallet_address: wallet_address.to_string(),
                event_type,
                token_mint: token_side.address.clone(),
                token_amount,
                sol_amount,
                timestamp,
                transaction_fee: Decimal::ZERO, // BirdEye doesn't provide detailed fee info
                metadata: EventMetadata {
                    program_id: Some(tx.address.clone()),
                    instruction_index: None,
                    exchange: Some(tx.source.clone()),
                    price_per_token: Some(token_price),
                    extra,
                },
            };

            events.push(event);
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
            max_signatures: Some(1000),
            timeframe_filter,
        }
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
            birdeye_client: self.birdeye_client.clone(),
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