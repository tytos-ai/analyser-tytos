use chrono::{DateTime, Utc};
use job_orchestrator::{BatchJob, JobStatus, OrchestratorStatus};
use pnl_core::{PnLFilters, PnLReport};
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
    pub redis_mode: bool,
    pub birdeye_api_configured: bool,
    pub pnl_filters: PnLFiltersSummary,
}

/// P&L filters summary for API responses
#[derive(Debug, Serialize)]
pub struct PnLFiltersSummary {
    pub timeframe_mode: String,
    pub min_capital_sol: Decimal,
    pub min_trades: u32,
    pub win_rate: Decimal,
}

/// Request to submit a batch P&L job
#[derive(Debug, Deserialize)]
pub struct BatchJobRequest {
    pub wallet_addresses: Vec<String>,
    pub filters: Option<PnLFilters>,
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
}

/// Individual wallet result
#[derive(Debug, Serialize)]
pub struct WalletResult {
    pub wallet_address: String,
    pub status: String,
    pub pnl_report: Option<PnLReport>,
    pub error_message: Option<String>,
}

/// Query parameters for discovered wallets endpoint
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct DiscoveredWalletsQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub min_pnl: Option<Decimal>,
    pub max_pnl: Option<Decimal>,
    pub order_by: Option<String>, // "pnl", "discovered_at", "win_rate"
    pub order_direction: Option<String>, // "asc", "desc"
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
    pub discovered_at: DateTime<Utc>,
    pub analyzed_at: Option<DateTime<Utc>>,
    pub pnl_usd: Option<Decimal>,
    pub win_rate: Option<Decimal>,
    pub trade_count: Option<u32>,
    pub status: String,
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
}

/// Configuration update request
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ConfigUpdateRequest {
    pub pnl_filters: Option<PnLFiltersUpdate>,
    pub system_settings: Option<SystemSettingsUpdate>,
}

/// P&L filters update
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct PnLFiltersUpdate {
    pub timeframe_mode: Option<String>,
    pub timeframe_general: Option<String>,
    pub timeframe_specific: Option<String>,
    pub wallet_min_capital: Option<Decimal>,
    pub aggregator_min_hold_minutes: Option<u32>,
    pub amount_trades: Option<u32>,
    pub win_rate: Option<Decimal>,
}

/// System settings update
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct SystemSettingsUpdate {
    pub max_signatures: Option<u32>,
    pub process_loop_ms: Option<u64>,
    pub redis_mode: Option<bool>,
}


/// System logs query parameters
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct LogsQuery {
    pub level: Option<String>, // "error", "warn", "info", "debug"
    pub limit: Option<u32>,
    pub since: Option<DateTime<Utc>>,
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
        let completed_wallets = job.results.len();
        let successful_wallets = job.results.values().filter(|r| r.is_ok()).count();
        let failed_wallets = completed_wallets - successful_wallets;
        
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
        let mut wallet_results = HashMap::new();
        let mut total_pnl = Decimal::ZERO;
        let mut successful_count = 0;

        for (wallet, result) in &job.results {
            let wallet_result = match result {
                Ok(report) => {
                    total_pnl += report.summary.total_pnl_usd;
                    successful_count += 1;
                    WalletResult {
                        wallet_address: wallet.clone(),
                        status: "success".to_string(),
                        pnl_report: Some(report.clone()),
                        error_message: None,
                    }
                }
                Err(e) => WalletResult {
                    wallet_address: wallet.clone(),
                    status: "failed".to_string(),
                    pnl_report: None,
                    error_message: Some(e.to_string()),
                },
            };
            wallet_results.insert(wallet.clone(), wallet_result);
        }

        let total_wallets = job.wallet_addresses.len();
        let average_pnl = if successful_count > 0 {
            total_pnl / Decimal::from(successful_count)
        } else {
            Decimal::ZERO
        };

        let profitable_wallets = job
            .results
            .values()
            .filter(|r| {
                r.as_ref()
                    .map(|report| report.summary.total_pnl_usd > Decimal::ZERO)
                    .unwrap_or(false)
            })
            .count();

        Self {
            job_id: job.id,
            status: job.status,
            summary: BatchResultsSummary {
                total_wallets,
                successful_analyses: successful_count,
                failed_analyses: total_wallets - successful_count,
                total_pnl_usd: total_pnl,
                average_pnl_usd: average_pnl,
                profitable_wallets,
            },
            results: wallet_results,
        }
    }
}

/// Trader filtering response
#[derive(Debug, Serialize)]
pub struct TraderFilterResponse {
    pub job_id: Uuid,
    pub total_analyzed: usize,
    pub qualified_traders: usize,
    pub traders: Vec<QualifiedTrader>,
    pub summary: String,
}

/// Qualified trader summary for copy trading
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
    pub sort_by: Option<String>, // "pnl", "analyzed_at", "wallet_address"
    pub order: Option<String>,   // "asc", "desc"
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
    pub token_address: String,
    pub token_symbol: String,
    pub total_pnl_usd: Decimal,
    pub realized_pnl_usd: Decimal,
    pub unrealized_pnl_usd: Decimal,
    pub roi_percentage: Decimal,
    pub total_trades: u32,
    pub win_rate: Decimal,
    pub analyzed_at: DateTime<Utc>,
}

/// Summary of all P&L results
#[derive(Debug, Serialize)]
pub struct AllResultsSummary {
    pub total_wallets: u64,
    pub profitable_wallets: u64,
    pub total_pnl_usd: Decimal,
    pub average_pnl_usd: Decimal,
    pub total_trades: u64,
    pub profitability_rate: f64,
    pub last_updated: DateTime<Utc>,
}

/// Detailed P&L result response
#[derive(Debug, Serialize)]
pub struct DetailedPnLResultResponse {
    pub wallet_address: String,
    pub token_address: String,
    pub token_symbol: String,
    pub pnl_report: pnl_core::PnLReport,
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