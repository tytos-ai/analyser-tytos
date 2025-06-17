use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use config_manager::{SystemConfig, ConfigurationError};
use dex_client::DexClient;
use job_orchestrator::{JobOrchestrator, OrchestratorError};
use pnl_core::PnLError;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tracing::{error, info, warn};

mod handlers;
mod middleware;
mod types;

use handlers::*;
use types::*;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub config: SystemConfig,
    pub orchestrator: Arc<JobOrchestrator>,
    pub dex_client: Arc<Mutex<Option<DexClient>>>,
}

/// Main application error type
#[derive(thiserror::Error, Debug)]
pub enum ApiError {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigurationError),
    #[error("Orchestrator error: {0}")]
    Orchestrator(#[from] OrchestratorError),
    #[error("P&L calculation error: {0}")]
    PnL(#[from] PnLError),
    #[error("DexClient error: {0}")]
    DexClient(String),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Internal server error: {0}")]
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            ApiError::Config(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            ApiError::Orchestrator(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            ApiError::PnL(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            ApiError::DexClient(_) => (StatusCode::BAD_GATEWAY, self.to_string()),
            ApiError::Validation(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            ApiError::NotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            ApiError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };

        let body = Json(ErrorResponse {
            error: error_message,
            timestamp: chrono::Utc::now(),
        });

        (status, body).into_response()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,api_server=debug".into()),
        )
        .init();

    info!("Starting P&L Tracker API Server...");

    // Load configuration
    let config = SystemConfig::load()?;
    info!("Configuration loaded successfully");

    // Initialize job orchestrator
    let orchestrator = Arc::new(JobOrchestrator::new(config.clone()).await?);
    info!("Job orchestrator initialized");

    // Initialize dex client if enabled
    let dex_client = if config.system.redis_mode {
        let dex_config = dex_client::DexClientConfig {
            api_base_url: config.dexscreener.api_base_url.clone(),
            ws_url: config.dexscreener.websocket_url.clone(),
            http_base_url: config.dexscreener.http_base_url.clone(),
            request_timeout_seconds: 30, // Default timeout
            debug: false,
            trending_criteria: dex_client::TrendingCriteria {
                min_volume_24h: config.dexscreener.trending.min_volume_24h,
                min_txns_24h: config.dexscreener.trending.min_txns_24h,
                min_liquidity_usd: config.dexscreener.trending.min_liquidity_usd,
                min_price_change_24h: config.dexscreener.trending.min_price_change_24h,
                max_pair_age_hours: config.dexscreener.trending.max_pair_age_hours,
            },
        };
        match DexClient::new(dex_config, None).await {
            Ok(client) => {
                info!("DexClient initialized successfully");
                Arc::new(Mutex::new(Some(client)))
            }
            Err(e) => {
                warn!("Failed to initialize DexClient: {}", e);
                Arc::new(Mutex::new(None))
            }
        }
    } else {
        Arc::new(Mutex::new(None))
    };

    // Create application state
    let app_state = AppState {
        config: config.clone(),
        orchestrator,
        dex_client,
    };

    // Build the application router
    let app = create_router(app_state.clone()).await;

    // Start continuous mode if enabled
    if config.system.redis_mode {
        let orchestrator_clone = app_state.orchestrator.clone();
        tokio::spawn(async move {
            info!("Starting continuous mode in background...");
            if let Err(e) = orchestrator_clone.start_continuous_mode().await {
                error!("Continuous mode failed: {}", e);
            }
        });
    }

    // Start DexClient monitoring if enabled
    if config.system.redis_mode {
        let dex_client_clone = app_state.dex_client.clone();
        tokio::spawn(async move {
            info!("Starting DexClient monitoring in background...");
            let mut dex_guard = dex_client_clone.lock().await;
            if let Some(ref mut dex_client) = dex_guard.as_mut() {
                if let Err(e) = dex_client.start_monitoring().await {
                    error!("DexClient monitoring failed: {}", e);
                }
            }
        });
    }

    // Bind and serve
    let bind_addr = format!("{}:{}", config.api.host, config.api.port);
    info!("Starting server on {}", bind_addr);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    info!("Server listening on {}", bind_addr);

    axum::serve(listener, app).await?;

    Ok(())
}

/// Create the main application router
async fn create_router(state: AppState) -> Router {
    Router::new()
        // Health check
        .route("/health", get(health_check))
        
        // System endpoints
        .route("/api/status", get(get_system_status))
        .route("/api/logs", get(get_system_logs))
        
        // Configuration endpoints
        .route("/api/config", get(get_config))
        .route("/api/config", post(update_config))
        
        // Batch P&L analysis endpoints
        .route("/api/pnl/batch/run", post(submit_batch_job))
        .route("/api/pnl/batch/status/:job_id", get(get_batch_job_status))
        .route("/api/pnl/batch/results/:job_id", get(get_batch_job_results))
        .route("/api/pnl/batch/results/:job_id/export.csv", get(export_batch_results_csv))
        .route("/api/pnl/batch/results/:job_id/traders", get(filter_copy_traders))
        
        // Continuous mode endpoints
        .route("/api/pnl/continuous/discovered-wallets", get(get_discovered_wallets))
        .route("/api/pnl/continuous/discovered-wallets/:wallet_address/details", get(get_wallet_details))
        
        // Dex monitoring endpoints
        .route("/api/dex/status", get(get_dex_status))
        .route("/api/dex/control", post(control_dex_service))
        
        // Add CORS middleware
        .layer(
            ServiceBuilder::new()
                .layer(CorsLayer::permissive())
                .into_inner(),
        )
        .with_state(state)
}