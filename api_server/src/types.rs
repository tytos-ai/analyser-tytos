use chrono::{DateTime, Utc};
use config_manager::denormalize_chain_for_frontend;
use job_orchestrator::{BatchJob, OrchestratorStatus};
use persistence_layer::JobStatus;
use pnl_core::PortfolioPnLResult;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Standard API error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub timestamp: DateTime<Utc>,
}

/// Standard API success response
#[derive(Debug, Serialize)]
pub struct SuccessResponse<T> {
    pub data: T,
    pub timestamp: DateTime<Utc>,
}

impl<T> SuccessResponse<T> {
    pub fn new(data: T) -> Self {
        Self {
            data,
            timestamp: Utc::now(),
        }
    }
}

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_seconds: u64,
}

/// System status response
#[derive(Debug, Serialize)]
pub struct SystemStatusResponse {
    pub orchestrator: OrchestratorStatus,
    pub dex_client: DexClientStatus,
    pub config: ConfigSummary,
}

/// DexClient status
#[derive(Debug, Serialize)]
pub struct DexClientStatus {
    pub enabled: bool,
    pub connected: bool,
    pub last_activity: Option<DateTime<Utc>>,
    pub processed_pairs: u64,
    pub discovered_wallets: u64,
}

/// Configuration summary (safe subset for API responses)
#[derive(Debug, Serialize)]
pub struct ConfigSummary {
    pub birdeye_api_configured: bool,
    pub zerion_api_configured: bool,
    pub architecture: String, // "Zerion+BirdEye Hybrid"
    pub parallel_batch_size: usize,
}

/// Request to submit a batch P&L job
#[derive(Debug, Deserialize)]
pub struct BatchJobRequest {
    pub wallet_addresses: Vec<String>,
    pub chain: String,
    pub max_transactions: Option<u32>,
    /// Time range for transaction filtering (e.g., "1h", "7d", "1m")
    /// When provided, fetches ALL transactions within this period (ignores max_transactions)
    pub time_range: Option<String>,
}

/// Response for batch job submission
#[derive(Debug, Serialize)]
pub struct BatchJobResponse {
    pub job_id: Uuid,
    pub wallet_count: usize,
    pub status: JobStatus,
    pub submitted_at: DateTime<Utc>,
}

/// Batch job status response
#[derive(Debug, Serialize)]
pub struct BatchJobStatusResponse {
    pub job_id: Uuid,
    pub status: JobStatus,
    pub wallet_count: usize,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub progress: JobProgress,
}

/// Job progress information
#[derive(Debug, Serialize)]
pub struct JobProgress {
    pub total_wallets: usize,
    pub completed_wallets: usize,
    pub successful_wallets: usize,
    pub failed_wallets: usize,
    pub progress_percentage: f64,
}

/// Batch job results response
#[derive(Debug, Serialize)]
pub struct BatchJobResultsResponse {
    pub job_id: Uuid,
    pub status: JobStatus,
    pub chain: String,
    pub summary: BatchResultsSummary,
    pub results: HashMap<String, WalletResult>,
}

/// Batch results summary
#[derive(Debug, Serialize)]
pub struct BatchResultsSummary {
    pub total_wallets: usize,
    pub successful_analyses: usize,
    pub failed_analyses: usize,
    pub total_pnl_usd: Decimal,
    pub average_pnl_usd: Decimal,
    pub profitable_wallets: usize,
    #[serde(default)]
    pub total_incomplete_trades: u32,  // Count of trades with only OUT transfers (no IN side) across all analyzed wallets
}

/// Individual wallet result
#[derive(Debug, Serialize)]
pub struct WalletResult {
    pub wallet_address: String,
    pub status: String,
    pub pnl_report: Option<PortfolioPnLResult>,
    pub error_message: Option<String>,
    #[serde(default)]
    pub incomplete_trades_count: u32,  // Count of trades with missing IN or OUT sides for this wallet
}

/// Query parameters for discovered wallets endpoint
#[derive(Debug, Deserialize)]
pub struct DiscoveredWalletsQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub chain: Option<String>,
}

/// Response for discovered wallets endpoint
#[derive(Debug, Serialize)]
pub struct DiscoveredWalletsResponse {
    pub wallets: Vec<DiscoveredWalletSummary>,
    pub pagination: PaginationInfo,
    pub summary: DiscoveredWalletsSummary,
}

/// Summary of a discovered wallet
#[derive(Debug, Serialize)]
pub struct DiscoveredWalletSummary {
    pub wallet_address: String,
    pub chain: String,
    pub discovered_at: DateTime<Utc>,
    pub analyzed_at: Option<DateTime<Utc>>,
    pub pnl_usd: Option<Decimal>,
    pub win_rate: Option<Decimal>,
    pub trade_count: Option<u32>,
    pub avg_hold_time_minutes: Option<Decimal>,
    pub unique_tokens_count: Option<u32>,
    pub active_days_count: Option<u32>,
    pub status: String,
    #[serde(default)]
    pub incomplete_trades_count: u32,  // Count of trades with missing IN or OUT sides for this wallet
}

/// Pagination information
#[derive(Debug, Serialize)]
pub struct PaginationInfo {
    pub total_count: u64,
    pub limit: u32,
    pub offset: u32,
    pub has_more: bool,
}

/// Summary of all discovered wallets
#[derive(Debug, Serialize)]
pub struct DiscoveredWalletsSummary {
    pub total_discovered: u64,
    pub analyzed_count: u64,
    pub profitable_count: u64,
    pub average_pnl_usd: Decimal,
    pub total_pnl_usd: Decimal,
    #[serde(default)]
    pub total_incomplete_trades: u32,  // Count of trades with only OUT transfers (no IN side) across all discovered wallets
}

/// Configuration update request
#[derive(Debug, Deserialize)]
pub struct ConfigUpdateRequest {
    // Empty for now - config update not implemented
}

/// System logs query parameters
#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    // Empty for now - log querying not implemented
}

/// System logs response
#[derive(Debug, Serialize)]
pub struct LogsResponse {
    pub logs: Vec<LogEntry>,
    pub total_count: u64,
}

/// Individual log entry
#[derive(Debug, Serialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: String,
    pub target: String,
    pub message: String,
    pub fields: HashMap<String, serde_json::Value>,
}

/// CSV export metadata
#[derive(Debug, Serialize)]
pub struct CsvExportInfo {
    pub filename: String,
    pub row_count: usize,
    pub generated_at: DateTime<Utc>,
    pub job_id: Uuid,
}

// Utility functions for type conversions

impl From<BatchJob> for BatchJobStatusResponse {
    fn from(job: BatchJob) -> Self {
        let total_wallets = job.wallet_addresses.len();
        // Results are now stored in PostgreSQL, not in memory
        // Use individual_jobs to track progress
        let completed_wallets = job.individual_jobs.len();
        // Without access to results here, we can't determine success/failure counts
        // This should be fetched from PostgreSQL when needed
        let successful_wallets = if job.status == JobStatus::Completed {
            completed_wallets
        } else {
            0
        };
        let failed_wallets = 0; // Unknown without querying results

        let progress_percentage = if total_wallets > 0 {
            (completed_wallets as f64 / total_wallets as f64) * 100.0
        } else {
            0.0
        };

        Self {
            job_id: job.id,
            status: job.status,
            wallet_count: total_wallets,
            created_at: job.created_at,
            started_at: job.started_at,
            completed_at: job.completed_at,
            progress: JobProgress {
                total_wallets,
                completed_wallets,
                successful_wallets,
                failed_wallets,
                progress_percentage,
            },
        }
    }
}

impl From<BatchJob> for BatchJobResultsResponse {
    fn from(job: BatchJob) -> Self {
        // Results are now stored in PostgreSQL, not in BatchJob
        // This conversion can't produce meaningful results without DB access
        // The actual results should be fetched from PostgreSQL in the handler
        let wallet_results = HashMap::new();
        let total_pnl = Decimal::ZERO;
        let successful_count = 0;

        let total_wallets = job.wallet_addresses.len();
        let average_pnl = Decimal::ZERO;
        let profitable_wallets = 0;

        Self {
            job_id: job.id,
            status: job.status,
            chain: denormalize_chain_for_frontend(&job.chain),
            summary: BatchResultsSummary {
                total_wallets,
                successful_analyses: successful_count,
                failed_analyses: total_wallets - successful_count,
                total_pnl_usd: total_pnl,
                average_pnl_usd: average_pnl,
                profitable_wallets,
                total_incomplete_trades: 0,  // Will be populated by handler from database
            },
            results: wallet_results,
        }
    }
}

/// Trader summaries response for batch job results
#[derive(Debug, Serialize)]
pub struct TraderFilterResponse {
    pub job_id: Uuid,
    pub total_analyzed: usize,
    pub qualified_traders: usize, // Note: Currently returns count of all successful analyses, not filtered count
    pub traders: Vec<QualifiedTrader>,
    pub summary: String,
}

/// Trader summary formatted for copy trading analysis
#[derive(Debug, Serialize)]
pub struct QualifiedTrader {
    pub wallet_address: String,
    pub score: f64,
    pub risk_level: String,
    pub trading_style: String,
    pub pnl_summary: TraderPnLSummary,
    pub strengths: Vec<String>,
    pub concerns: Vec<String>,
    pub copy_trade_recommended: bool,
}

/// P&L summary for qualified trader
#[derive(Debug, Serialize)]
pub struct TraderPnLSummary {
    pub total_pnl_usd: String,
    pub realized_pnl_usd: String,
    pub roi_percentage: String,
    pub win_rate: String,
    pub total_trades: u32,
    pub winning_trades: u32,
    pub capital_deployed_sol: String,
}

// =====================================
// Service Management Types
// =====================================

/// Service control request
#[derive(Debug, Deserialize)]
pub struct ServiceControlRequest {
    pub action: String,                             // "start" | "stop" | "restart"
    pub service: String,                            // "wallet_discovery" | "pnl_analysis"
    pub config_override: Option<serde_json::Value>, // Optional runtime configuration as JSON
}

/// Simple message response
#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub message: String,
}

/// Discovery cycle response
#[derive(Debug, Serialize)]
pub struct DiscoveryCycleResponse {
    pub message: String,
    pub discovered_wallets: u64,
}

// =====================================
// Results Retrieval Types
// =====================================

/// Query parameters for getting all P&L results
#[derive(Debug, Deserialize)]
pub struct AllResultsQuery {
    pub offset: Option<usize>,
    pub limit: Option<usize>,
    pub chain: Option<String>,
}

/// Response for all P&L results
#[derive(Debug, Serialize)]
pub struct AllResultsResponse {
    pub results: Vec<StoredPnLResultSummary>,
    pub pagination: PaginationInfo,
    pub summary: AllResultsSummary,
}

/// Simplified P&L result for list view
#[derive(Debug, Serialize)]
pub struct StoredPnLResultSummary {
    pub wallet_address: String,
    pub chain: String,
    pub token_address: String,
    pub token_symbol: String,
    pub total_pnl_usd: f64,
    pub realized_pnl_usd: f64,
    pub unrealized_pnl_usd: f64,
    pub roi_percentage: f64,
    pub total_trades: u32,
    pub win_rate: f64,
    pub avg_hold_time_minutes: f64,
    pub unique_tokens_count: Option<u32>,
    pub active_days_count: Option<u32>,
    pub analyzed_at: DateTime<Utc>,
    pub is_favorited: bool,
    pub is_archived: bool,
    #[serde(default)]
    pub incomplete_trades_count: u32,  // Count of trades with missing IN or OUT sides for this wallet
}

/// Summary of all P&L results
#[derive(Debug, Serialize)]
pub struct AllResultsSummary {
    pub total_wallets: u64,
    pub profitable_wallets: u64,
    pub total_pnl_usd: f64,
    pub average_pnl_usd: f64,
    pub total_trades: u64,
    pub profitability_rate: f64,
    pub last_updated: DateTime<Utc>,
}

/// Detailed P&L result response
#[derive(Debug, Serialize)]
pub struct DetailedPnLResultResponse {
    pub wallet_address: String,
    pub chain: String,
    pub portfolio_result: pnl_core::PortfolioPnLResult,
    pub analyzed_at: DateTime<Utc>,
}

/// Enhanced health response with component status
#[derive(Debug, Serialize)]
pub struct EnhancedHealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_seconds: u64,
    pub components: ComponentHealthStatus,
}

/// Component health status
#[derive(Debug, Serialize)]
pub struct ComponentHealthStatus {
    pub redis: RedisComponentHealth,
    pub birdeye_api: ApiComponentHealth,
    pub services: ServicesComponentHealth,
}

/// Redis component health
#[derive(Debug, Serialize)]
pub struct RedisComponentHealth {
    pub connected: bool,
    pub latency_ms: u64,
    pub error: Option<String>,
}

/// API component health  
#[derive(Debug, Serialize)]
pub struct ApiComponentHealth {
    pub accessible: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

/// Services component health
#[derive(Debug, Serialize)]
pub struct ServicesComponentHealth {
    pub wallet_discovery: String, // "Running", "Stopped", "Error"
    pub pnl_analysis: String,     // "Running", "Stopped", "Error"
}

// =====================================
// Batch Job History Types
// =====================================

/// Query parameters for batch job history listing
#[derive(Debug, Deserialize)]
pub struct BatchJobHistoryQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Summary of a batch job for history listing
#[derive(Debug, Serialize)]
pub struct BatchJobSummary {
    pub id: Uuid,
    pub wallet_count: usize,
    pub chain: String,
    pub status: JobStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub success_count: usize,
    pub failure_count: usize,
}

/// Batch job history response
#[derive(Debug, Serialize)]
pub struct BatchJobHistoryResponse {
    pub jobs: Vec<BatchJobSummary>,
    pub pagination: PaginationInfo,
    pub summary: BatchJobHistorySummary,
}

/// Summary statistics for batch job history
#[derive(Debug, Serialize)]
pub struct BatchJobHistorySummary {
    pub total_jobs: u64,
    pub pending_jobs: u64,
    pub running_jobs: u64,
    pub completed_jobs: u64,
    pub failed_jobs: u64,
}

// =====================================
// Multichain Validation Helpers
// =====================================

/// Validate that a chain is supported and enabled
pub fn validate_chain(chain: &str, enabled_chains: &[String]) -> Result<(), String> {
    if !enabled_chains.contains(&chain.to_string()) {
        return Err(format!(
            "Unsupported chain: {}. Supported chains: {:?}",
            chain, enabled_chains
        ));
    }
    Ok(())
}

/// Get chain from optional parameter or use default
#[allow(dead_code)]
pub fn get_chain_or_default<'a>(
    chain_param: Option<&'a str>,
    default_chain: &'a str,
    enabled_chains: &[String],
) -> Result<&'a str, String> {
    let chain = chain_param.unwrap_or(default_chain);
    validate_chain(chain, enabled_chains)?;
    Ok(chain)
}
