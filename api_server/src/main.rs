use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use config_manager::{SystemConfig, ConfigurationError};
use job_orchestrator::{JobOrchestrator, OrchestratorError};
use pnl_core::PnLError;
use dex_client::BirdEyeClient;
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tracing::info;

mod handlers;
mod middleware;
mod types;
mod service_manager;
mod v2;

use handlers::*;
use types::*;
use service_manager::ServiceManager;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub config: SystemConfig,
    pub orchestrator: Arc<JobOrchestrator>,
    pub service_manager: Arc<ServiceManager>,
    pub persistence_client: Arc<persistence_layer::PersistenceClient>,
    // API v2 dependencies
    pub birdeye_client: Arc<BirdEyeClient>,
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
    #[error("Service management error: {0}")]
    ServiceManager(String),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Internal server error: {0}")]
    InternalServerError(String),
    #[error("Internal server error: {0}")]
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            ApiError::Config(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            ApiError::Orchestrator(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            ApiError::PnL(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            ApiError::ServiceManager(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            ApiError::Validation(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            ApiError::NotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            ApiError::InternalServerError(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
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

    // Initialize persistence client for shared database access
    let persistence_client = Arc::new(
        persistence_layer::PersistenceClient::new(
            &config.redis.url,
            &config.database.postgres_url
        ).await?
    );
    info!("Persistence client initialized with connection pool");

    // Initialize job orchestrator with shared persistence client
    let orchestrator = Arc::new(JobOrchestrator::new(config.clone(), persistence_client.clone()).await?);
    info!("Job orchestrator initialized");

    // Initialize service manager (but don't start any services yet)
    let service_manager = Arc::new(ServiceManager::new(config.clone(), orchestrator.clone()));
    info!("Service manager initialized");

    // Initialize clients for API v2
    let birdeye_client = Arc::new(BirdEyeClient::new(config.birdeye.clone())?);
    info!("API v2 clients initialized");

    // Create application state
    let app_state = AppState {
        config: config.clone(),
        orchestrator,
        service_manager,
        persistence_client,
        birdeye_client,
    };

    // Build the application router
    let app = create_router(app_state.clone()).await;
    
    info!("🎯 API Server ready - services can be controlled via API endpoints");
    info!("📋 Available endpoints:");
    info!("   • POST /api/services/control - Universal service control with optional config");
    info!("   • POST /api/services/config - Configure services");
    info!("   • POST /api/services/discovery/start - Start wallet discovery");
    info!("   • POST /api/services/discovery/stop - Stop wallet discovery");
    info!("   • POST /api/services/pnl/start - Start P&L analysis");
    info!("   • POST /api/services/pnl/stop - Stop P&L analysis");
    info!("   • GET /api/services/status - Get service status");
    info!("   • GET /health - Health check");
    info!("🚀 API v2 endpoints (Enhanced Copy Trading Analysis):");
    info!("   • GET /api/v2/wallets/:address/analysis - Comprehensive wallet analysis");
    info!("   • GET /api/v2/wallets/:address/trades - Individual trade details");
    info!("   • GET /api/v2/wallets/:address/positions - Current positions tracking");
    info!("   • POST /api/v2/pnl/batch/run - Enhanced batch analysis");

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
        .route("/health/detailed", get(enhanced_health_check))
        
        // Service management endpoints
        .route("/api/services/status", get(get_services_status))
        .route("/api/services/config", get(get_services_config))
        .route("/api/services/config", post(update_services_config))
        .route("/api/services/control", post(control_service))  // NEW: Universal service control with optional config
        .route("/api/services/discovery/start", post(start_wallet_discovery))
        .route("/api/services/discovery/stop", post(stop_wallet_discovery))
        .route("/api/services/discovery/trigger", post(trigger_discovery_cycle))
        .route("/api/services/pnl/start", post(start_pnl_analysis))
        .route("/api/services/pnl/stop", post(stop_pnl_analysis))
        
        // Results retrieval endpoints
        .route("/api/results", get(get_all_results))
        .route("/api/results/:wallet_address/:token_address", get(get_detailed_result))
        .route("/api/results/:wallet_address/favorite", post(toggle_wallet_favorite))
        .route("/api/results/:wallet_address/archive", post(toggle_wallet_archive))
        .route("/api/results/backfill-metrics", post(backfill_advanced_filtering_metrics))
        
        // Legacy system endpoints (kept for compatibility)
        .route("/api/status", get(get_system_status))
        .route("/api/logs", get(get_system_logs))
        
        // Configuration endpoints (legacy)
        .route("/api/config", get(get_config))
        .route("/api/config", post(update_config))
        
        // Batch P&L analysis endpoints
        .route("/api/pnl/batch/run", post(submit_batch_job))
        .route("/api/pnl/batch/status/:job_id", get(get_batch_job_status))
        .route("/api/pnl/batch/results/:job_id", get(get_batch_job_results))
        .route("/api/pnl/batch/results/:job_id/export.csv", get(export_batch_results_csv))
        .route("/api/pnl/batch/results/:job_id/traders", get(filter_copy_traders))
        .route("/api/pnl/batch/history", get(get_batch_job_history))
        
        // Continuous mode endpoints
        .route("/api/pnl/continuous/discovered-wallets", get(get_discovered_wallets))
        .route("/api/pnl/continuous/discovered-wallets/:wallet_address/details", get(get_wallet_details))
        
        // API v2 - Enhanced P&L analysis for copy trading
        .nest("/api/v2", v2::create_v2_routes())
        
        // Add CORS middleware
        .layer(
            ServiceBuilder::new()
                .layer(CorsLayer::permissive())
                .into_inner(),
        )
        .with_state(state)
}