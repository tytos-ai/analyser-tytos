use anyhow::Result;
use job_orchestrator::{JobOrchestrator, BirdEyeTrendingOrchestrator, BirdEyeTrendingConfig};
use dex_client::TopTraderFilter;
use config_manager::SystemConfig;
use persistence_layer::RedisClient;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{info, error, debug};
use serde::{Serialize, Deserialize};

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
    /// BirdEye trending configuration
    pub birdeye_config: BirdEyeTrendingServiceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BirdEyeTrendingServiceConfig {
    /// Maximum trending tokens to analyze per cycle
    pub max_trending_tokens: usize,
    /// Maximum traders to discover per token
    pub max_traders_per_token: usize,
    /// Discovery cycle interval in seconds
    pub cycle_interval_seconds: u64,
    /// Minimum volume threshold for traders (USD)
    pub min_trader_volume_usd: f64,
    /// Minimum trades threshold for traders
    pub min_trader_trades: u32,
    /// Enable debug logging
    pub debug_mode: bool,
}

impl Default for BirdEyeTrendingServiceConfig {
    fn default() -> Self {
        Self {
            max_trending_tokens: 20,
            max_traders_per_token: 10,
            cycle_interval_seconds: 300, // 5 minutes
            min_trader_volume_usd: 1000.0,
            min_trader_trades: 5,
            debug_mode: false,
        }
    }
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            enable_wallet_discovery: false,
            enable_pnl_analysis: false,
            birdeye_config: BirdEyeTrendingServiceConfig::default(),
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
        let config = self.config.read().await;
        if !config.enable_wallet_discovery {
            return Err(anyhow::anyhow!("Wallet discovery service is disabled in configuration"));
        }

        let mut state = self.wallet_discovery_state.write().await;
        if *state == ServiceState::Running {
            return Err(anyhow::anyhow!("Wallet discovery service is already running"));
        }

        *state = ServiceState::Starting;
        drop(state);

        info!("🚀 Starting BirdEye wallet discovery service");

        // Create BirdEye trending orchestrator
        let birdeye_config = BirdEyeTrendingConfig {
            api_key: self.system_config.birdeye.api_key.clone(),
            api_base_url: self.system_config.birdeye.api_base_url.clone(),
            chain: "solana".to_string(),
            top_trader_filter: TopTraderFilter {
                min_volume_usd: config.birdeye_config.min_trader_volume_usd,
                min_trades: config.birdeye_config.min_trader_trades,
                min_win_rate: None, // BirdEye doesn't provide this
                max_last_trade_hours: None, // BirdEye doesn't provide this
                max_traders: Some(config.birdeye_config.max_traders_per_token),
            },
            max_trending_tokens: config.birdeye_config.max_trending_tokens,
            max_traders_per_token: config.birdeye_config.max_traders_per_token,
            cycle_interval_seconds: config.birdeye_config.cycle_interval_seconds,
            debug_mode: config.birdeye_config.debug_mode,
        };

        let redis_client = RedisClient::new(&self.system_config.redis.url).await?;
        let orchestrator = BirdEyeTrendingOrchestrator::new(birdeye_config, Some(redis_client))?;

        // Store the orchestrator instance
        {
            let mut birdeye_orch = self.birdeye_orchestrator.lock().await;
            *birdeye_orch = Some(orchestrator);
        }

        // Start the discovery service in background
        let birdeye_orch_clone = self.birdeye_orchestrator.clone();
        let state_clone = self.wallet_discovery_state.clone();
        let _stats_clone = self.stats.clone();
        let cycle_interval = config.birdeye_config.cycle_interval_seconds;

        let handle = tokio::spawn(async move {
            // Update state to running
            {
                let mut state = state_clone.write().await;
                *state = ServiceState::Running;
            }

            info!("✅ BirdEye wallet discovery service is now running - starting discovery loop");

            // Get the orchestrator and start the discovery loop
            loop {
                let should_continue = {
                    let guard = birdeye_orch_clone.lock().await;
                    if let Some(ref orch) = *guard {
                        // Execute a single discovery cycle
                        match orch.execute_discovery_cycle().await {
                            Ok(discovered_count) => {
                                if discovered_count > 0 {
                                    info!("🔍 Discovery cycle completed: {} wallets discovered", discovered_count);
                                } else {
                                    debug!("🔍 Discovery cycle completed: no new wallets discovered");
                                }
                                true // Continue running
                            }
                            Err(e) => {
                                error!("❌ Discovery cycle failed: {}", e);
                                true // Continue despite errors
                            }
                        }
                    } else {
                        error!("❌ BirdEye orchestrator instance lost");
                        false // Stop the loop
                    }
                };

                if !should_continue {
                    break;
                }

                // Wait for the configured cycle interval from config
                tokio::time::sleep(tokio::time::Duration::from_secs(cycle_interval)).await;
            }

            // Update state to error if we exit the loop
            {
                let mut state = state_clone.write().await;
                *state = ServiceState::Error("Discovery loop exited unexpectedly".to_string());
            }
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

        // Stop the BirdEye orchestrator if running
        {
            let guard = self.birdeye_orchestrator.lock().await;
            if let Some(ref orchestrator) = *guard {
                orchestrator.stop().await;
            }
        }

        // Cancel the background task
        {
            let mut handle_guard = self.wallet_discovery_handle.lock().await;
            if let Some(handle) = handle_guard.take() {
                handle.abort();
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
            return Err(anyhow::anyhow!("P&L analysis service is disabled in configuration"));
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