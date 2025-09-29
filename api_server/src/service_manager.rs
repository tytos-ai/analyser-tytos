use anyhow::Result;
use config_manager::SystemConfig;
use job_orchestrator::{BirdEyeTrendingOrchestrator, JobOrchestrator};
use persistence_layer::RedisClient;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, error, info};

/// Service states
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ServiceState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error(String),
}

/// Service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    /// Enable BirdEye wallet discovery service
    pub enable_wallet_discovery: bool,
    /// Enable P&L analysis service
    pub enable_pnl_analysis: bool,
    // Using SystemConfig.birdeye directly for BirdEye configuration
}

// BirdEyeTrendingServiceConfig removed - using SystemConfig directly

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            enable_wallet_discovery: false,
            enable_pnl_analysis: false,
        }
    }
}

/// Service statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStats {
    pub wallet_discovery: WalletDiscoveryStats,
    pub pnl_analysis: PnLAnalysisStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletDiscoveryStats {
    pub state: ServiceState,
    pub discovered_wallets_total: u64,
    pub queue_size: u64,
    pub last_cycle_wallets: u64,
    pub cycles_completed: u64,
    pub last_activity: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PnLAnalysisStats {
    pub state: ServiceState,
    pub wallets_processed: u64,
    pub wallets_in_progress: u64,
    pub successful_analyses: u64,
    pub failed_analyses: u64,
    pub last_activity: Option<chrono::DateTime<chrono::Utc>>,
}

/// Manages all system services (wallet discovery, P&L analysis)
pub struct ServiceManager {
    config: Arc<RwLock<ServiceConfig>>,
    system_config: SystemConfig,
    orchestrator: Arc<JobOrchestrator>,

    // Service states
    wallet_discovery_state: Arc<RwLock<ServiceState>>,
    pnl_analysis_state: Arc<RwLock<ServiceState>>,

    // Service handles
    wallet_discovery_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    pnl_analysis_handle: Arc<Mutex<Option<JoinHandle<()>>>>,

    // Service instances
    birdeye_orchestrator: Arc<Mutex<Option<BirdEyeTrendingOrchestrator>>>,

    // Statistics
    stats: Arc<RwLock<ServiceStats>>,
}

impl ServiceManager {
    pub fn new(system_config: SystemConfig, orchestrator: Arc<JobOrchestrator>) -> Self {
        let initial_stats = ServiceStats {
            wallet_discovery: WalletDiscoveryStats {
                state: ServiceState::Stopped,
                discovered_wallets_total: 0,
                queue_size: 0,
                last_cycle_wallets: 0,
                cycles_completed: 0,
                last_activity: None,
            },
            pnl_analysis: PnLAnalysisStats {
                state: ServiceState::Stopped,
                wallets_processed: 0,
                wallets_in_progress: 0,
                successful_analyses: 0,
                failed_analyses: 0,
                last_activity: None,
            },
        };

        Self {
            config: Arc::new(RwLock::new(ServiceConfig::default())),
            system_config,
            orchestrator,
            wallet_discovery_state: Arc::new(RwLock::new(ServiceState::Stopped)),
            pnl_analysis_state: Arc::new(RwLock::new(ServiceState::Stopped)),
            wallet_discovery_handle: Arc::new(Mutex::new(None)),
            pnl_analysis_handle: Arc::new(Mutex::new(None)),
            birdeye_orchestrator: Arc::new(Mutex::new(None)),
            stats: Arc::new(RwLock::new(initial_stats)),
        }
    }

    /// Get current service configuration
    pub async fn get_config(&self) -> ServiceConfig {
        self.config.read().await.clone()
    }

    /// Update service configuration
    pub async fn update_config(&self, new_config: ServiceConfig) -> Result<()> {
        info!("Updating service configuration");
        let mut config = self.config.write().await;
        *config = new_config;
        info!("Service configuration updated successfully");
        Ok(())
    }

    /// Get current service statistics
    pub async fn get_stats(&self) -> ServiceStats {
        let mut stats = self.stats.write().await;

        // Update current states
        stats.wallet_discovery.state = self.wallet_discovery_state.read().await.clone();
        stats.pnl_analysis.state = self.pnl_analysis_state.read().await.clone();

        // Update queue size from orchestrator
        if let Ok(orchestrator_status) = self.orchestrator.get_status().await {
            stats.wallet_discovery.queue_size = orchestrator_status.discovery_queue_size;
        }

        stats.clone()
    }

    /// Start wallet discovery service
    pub async fn start_wallet_discovery(&self) -> Result<()> {
        self.start_wallet_discovery_with_config(None).await
    }

    /// Start wallet discovery service with optional runtime configuration
    pub async fn start_wallet_discovery_with_config(
        &self,
        runtime_config: Option<serde_json::Value>,
    ) -> Result<()> {
        let config = self.config.read().await;
        if !config.enable_wallet_discovery {
            return Err(anyhow::anyhow!(
                "Wallet discovery service is disabled in configuration"
            ));
        }

        let mut state = self.wallet_discovery_state.write().await;
        if *state == ServiceState::Running {
            return Err(anyhow::anyhow!(
                "Wallet discovery service is already running"
            ));
        }

        *state = ServiceState::Starting;
        drop(state);

        // Log runtime configuration if provided
        if let Some(config_json) = runtime_config {
            info!("🔧 Starting with runtime configuration: {}", config_json);
        }

        info!("🚀 Starting DexScreener wallet discovery service");

        // Create DexScreener trending orchestrator using SystemConfig directly
        let redis_client = RedisClient::new(&self.system_config.redis.url).await?;
        let orchestrator =
            BirdEyeTrendingOrchestrator::new(self.system_config.clone(), Some(redis_client))?;

        // Store the orchestrator instance
        {
            let mut birdeye_orch = self.birdeye_orchestrator.lock().await;
            *birdeye_orch = Some(orchestrator);
        }

        // Start the discovery service in background
        let birdeye_orch_clone = self.birdeye_orchestrator.clone();
        let state_clone = self.wallet_discovery_state.clone();
        let _stats_clone = self.stats.clone();
        let cycle_interval = self
            .system_config
            .discovery
            .cycle_interval_seconds
            .unwrap_or(60);

        let handle = tokio::spawn(async move {
            // Update state to running
            {
                let mut state = state_clone.write().await;
                *state = ServiceState::Running;
            }

            info!("✅ DexScreener wallet discovery service is now running - starting discovery loop");

            // Get the orchestrator and start the discovery loop
            loop {
                let should_continue = {
                    let guard = birdeye_orch_clone.lock().await;
                    if let Some(ref orch) = *guard {
                        // Execute a single discovery cycle
                        match orch.execute_discovery_cycle().await {
                            Ok(discovered_count) => {
                                if discovered_count > 0 {
                                    info!(
                                        "🔍 Discovery cycle completed: {} wallets discovered",
                                        discovered_count
                                    );
                                } else {
                                    debug!(
                                        "🔍 Discovery cycle completed: no new wallets discovered"
                                    );
                                }
                                true // Continue running
                            }
                            Err(e) => {
                                error!("❌ Discovery cycle failed: {}", e);
                                true // Continue despite errors
                            }
                        }
                    } else {
                        error!("❌ DexScreener orchestrator instance lost");
                        false // Stop the loop
                    }
                };

                if !should_continue {
                    break;
                }

                // Wait for the configured cycle interval
                // This sleep is automatically cancelled when the task handle is aborted
                tokio::time::sleep(tokio::time::Duration::from_secs(cycle_interval)).await;
            }

            // Update state when exiting loop gracefully
            {
                let mut state = state_clone.write().await;
                *state = ServiceState::Stopped;
            }

            debug!("🛑 Discovery loop exited gracefully");
        });

        // Store the handle
        {
            let mut handle_guard = self.wallet_discovery_handle.lock().await;
            *handle_guard = Some(handle);
        }

        // Update state to running
        {
            let mut state = self.wallet_discovery_state.write().await;
            *state = ServiceState::Running;
        }

        info!("✅ Wallet discovery service started successfully");
        Ok(())
    }

    /// Stop wallet discovery service
    pub async fn stop_wallet_discovery(&self) -> Result<()> {
        info!("🛑 Stopping wallet discovery service");

        let mut state = self.wallet_discovery_state.write().await;
        if *state == ServiceState::Stopped {
            return Ok(());
        }

        *state = ServiceState::Stopping;
        drop(state);

        // Stop the BirdEye orchestrator if running and wait for it to stop gracefully
        {
            let guard = self.birdeye_orchestrator.lock().await;
            if let Some(ref orchestrator) = *guard {
                info!("🛑 Requesting orchestrator to stop gracefully");
                orchestrator.stop().await;
            }
        }

        // Wait for the background task to finish gracefully (with timeout)
        {
            let mut handle_guard = self.wallet_discovery_handle.lock().await;
            if let Some(mut handle) = handle_guard.take() {
                info!("🛑 Waiting for discovery task to finish gracefully (10s timeout)");

                // Give the task 10 seconds to stop gracefully, then abort
                match tokio::time::timeout(Duration::from_secs(10), &mut handle).await {
                    Ok(_) => {
                        info!("✅ Discovery task stopped gracefully");
                    }
                    Err(_) => {
                        info!("⚠️ Discovery task timeout - aborting task");
                        handle.abort();
                        // Wait a bit for the abort to take effect
                        let _ = handle.await;
                    }
                }
            }
        }

        // Clear the orchestrator instance
        {
            let mut birdeye_orch = self.birdeye_orchestrator.lock().await;
            *birdeye_orch = None;
        }

        // Update state to stopped
        {
            let mut state = self.wallet_discovery_state.write().await;
            *state = ServiceState::Stopped;
        }

        info!("✅ Wallet discovery service stopped successfully");
        Ok(())
    }

    /// Start P&L analysis service
    pub async fn start_pnl_analysis(&self) -> Result<()> {
        let config = self.config.read().await;
        if !config.enable_pnl_analysis {
            return Err(anyhow::anyhow!(
                "P&L analysis service is disabled in configuration"
            ));
        }

        let mut state = self.pnl_analysis_state.write().await;
        if *state == ServiceState::Running {
            return Err(anyhow::anyhow!("P&L analysis service is already running"));
        }

        *state = ServiceState::Starting;
        drop(state);

        info!("🚀 Starting P&L analysis service");

        // Start the continuous P&L processing
        let orchestrator_clone = self.orchestrator.clone();
        let state_clone = self.pnl_analysis_state.clone();
        let _stats_clone = self.stats.clone();

        let handle = tokio::spawn(async move {
            // Update state to running
            {
                let mut state = state_clone.write().await;
                *state = ServiceState::Running;
            }

            // Start continuous mode
            if let Err(e) = orchestrator_clone.start_continuous_mode().await {
                error!("❌ P&L analysis service failed: {}", e);
                let mut state = state_clone.write().await;
                *state = ServiceState::Error(e.to_string());
            } else {
                info!("✅ P&L analysis service completed");
                let mut state = state_clone.write().await;
                *state = ServiceState::Stopped;
            }
        });

        // Store the handle
        {
            let mut handle_guard = self.pnl_analysis_handle.lock().await;
            *handle_guard = Some(handle);
        }

        // Update state to running
        {
            let mut state = self.pnl_analysis_state.write().await;
            *state = ServiceState::Running;
        }

        info!("✅ P&L analysis service started successfully");
        Ok(())
    }

    /// Stop P&L analysis service
    pub async fn stop_pnl_analysis(&self) -> Result<()> {
        info!("🛑 Stopping P&L analysis service");

        let mut state = self.pnl_analysis_state.write().await;
        if *state == ServiceState::Stopped {
            return Ok(());
        }

        *state = ServiceState::Stopping;
        drop(state);

        // Cancel the background task
        {
            let mut handle_guard = self.pnl_analysis_handle.lock().await;
            if let Some(handle) = handle_guard.take() {
                handle.abort();
            }
        }

        // Update state to stopped
        {
            let mut state = self.pnl_analysis_state.write().await;
            *state = ServiceState::Stopped;
        }

        info!("✅ P&L analysis service stopped successfully");
        Ok(())
    }

    /// Trigger a manual wallet discovery cycle
    pub async fn trigger_discovery_cycle(&self) -> Result<u64> {
        let guard = self.birdeye_orchestrator.lock().await;
        if let Some(ref orchestrator) = *guard {
            let discovered = orchestrator.execute_discovery_cycle().await?;

            // Update stats
            {
                let mut stats = self.stats.write().await;
                stats.wallet_discovery.last_cycle_wallets = discovered as u64;
                stats.wallet_discovery.cycles_completed += 1;
                stats.wallet_discovery.discovered_wallets_total += discovered as u64;
                stats.wallet_discovery.last_activity = Some(chrono::Utc::now());
            }

            Ok(discovered as u64)
        } else {
            Err(anyhow::anyhow!("Wallet discovery service is not running"))
        }
    }
}
