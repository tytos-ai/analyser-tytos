use crate::{ApiError, AppState};
use crate::types::*;
use serde_json::Value;
use axum::{
    extract::{Path, Query, State},
    http::header,
    response::{IntoResponse, Json},
};
use chrono::Utc;
use csv::Writer;
use job_orchestrator::JobStatus;
use rust_decimal::Decimal;
use std::io::Cursor;
use tracing::{info, warn};
use uuid::Uuid;
use pnl_core::{TraderFilter, generate_trader_summary};

/// Health check endpoint
pub async fn health_check() -> impl IntoResponse {
    Json(SuccessResponse::new(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: 0, // TODO: Track actual uptime
    }))
}

/// Get system status
pub async fn get_system_status(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let orchestrator_status = state.orchestrator.get_status().await?;
    
    let dex_status = {
        let dex_guard = state.dex_client.lock().await;
        if let Some(_) = dex_guard.as_ref() {
            DexClientStatus {
                enabled: true,
                connected: true, // TODO: Get actual connection status
                last_activity: None, // TODO: Track last activity
                processed_pairs: 0, // TODO: Get actual metrics
                discovered_wallets: orchestrator_status.discovery_queue_size,
            }
        } else {
            DexClientStatus {
                enabled: false,
                connected: false,
                last_activity: None,
                processed_pairs: 0,
                discovered_wallets: 0,
            }
        }
    };

    let config_summary = ConfigSummary {
        redis_mode: state.config.system.redis_mode,
        solana_rpc_url: state.config.solana.rpc_url.clone(),
        max_signatures: state.config.solana.max_signatures,
        pnl_filters: PnLFiltersSummary {
            timeframe_mode: state.config.pnl.timeframe_mode.clone(),
            min_capital_sol: Decimal::from_f64_retain(state.config.pnl.wallet_min_capital).unwrap_or(Decimal::ZERO),
            min_trades: state.config.pnl.amount_trades,
            win_rate: Decimal::from_f64_retain(state.config.pnl.win_rate).unwrap_or(Decimal::ZERO),
        },
    };

    Ok(Json(SuccessResponse::new(SystemStatusResponse {
        orchestrator: orchestrator_status,
        dex_client: dex_status,
        config: config_summary,
    })))
}

/// Get system logs
pub async fn get_system_logs(
    Query(_query): Query<LogsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    // For now, return empty logs as we don't have a log storage system
    // In a real implementation, you'd integrate with your logging infrastructure
    let logs = LogsResponse {
        logs: vec![],
        total_count: 0,
    };

    Ok(Json(SuccessResponse::new(logs)))
}

/// Get current configuration
pub async fn get_config(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(SuccessResponse::new(state.config.clone())))
}

/// Update configuration
pub async fn update_config(
    State(_state): State<AppState>,
    Json(_request): Json<ConfigUpdateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // For now, return success but don't actually update config
    // In a real implementation, you'd validate and apply the configuration changes
    warn!("Configuration update requested but not implemented");
    
    Ok(Json(SuccessResponse::new(
        "Configuration update not yet implemented".to_string()
    )))
}

/// Submit a batch P&L job
pub async fn submit_batch_job(
    State(state): State<AppState>,
    Json(request): Json<BatchJobRequest>,
) -> Result<impl IntoResponse, ApiError> {
    info!("Batch job submitted for {} wallets", request.wallet_addresses.len());

    // Validate wallet addresses
    if request.wallet_addresses.is_empty() {
        return Err(ApiError::Validation("No wallet addresses provided".to_string()));
    }

    if request.wallet_addresses.len() > 1000 {
        return Err(ApiError::Validation("Too many wallet addresses (max 1000)".to_string()));
    }

    // Submit the job
    let job_id = state
        .orchestrator
        .submit_batch_job(request.wallet_addresses.clone(), request.filters)
        .await?;

    let response = BatchJobResponse {
        job_id,
        wallet_count: request.wallet_addresses.len(),
        status: JobStatus::Pending,
        submitted_at: Utc::now(),
    };

    Ok(Json(SuccessResponse::new(response)))
}

/// Get batch job status
pub async fn get_batch_job_status(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let job = state
        .orchestrator
        .get_batch_job_status(job_id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("Job {} not found", job_id)))?;

    let response: BatchJobStatusResponse = job.into();
    Ok(Json(SuccessResponse::new(response)))
}

/// Get batch job results
pub async fn get_batch_job_results(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let job = state
        .orchestrator
        .get_batch_job_status(job_id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("Job {} not found", job_id)))?;

    if job.status != JobStatus::Completed {
        return Err(ApiError::Validation(format!(
            "Job {} is not completed (status: {:?})",
            job_id, job.status
        )));
    }

    let response: BatchJobResultsResponse = job.into();
    Ok(Json(SuccessResponse::new(response)))
}

/// Export batch job results as CSV
pub async fn export_batch_results_csv(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let job = state
        .orchestrator
        .get_batch_job_status(job_id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("Job {} not found", job_id)))?;

    if job.status != JobStatus::Completed {
        return Err(ApiError::Validation(format!(
            "Job {} is not completed",
            job_id
        )));
    }

    // Generate CSV content
    let csv_content = generate_batch_results_csv(&job)?;
    let _filename = format!("batch_pnl_results_{}.csv", job_id);

    let headers = [
        (header::CONTENT_TYPE, "text/csv"),
        (header::CONTENT_DISPOSITION, "attachment; filename=\"batch_results.csv\""),
    ];

    Ok((headers, csv_content))
}

/// Get discovered wallets from continuous mode
pub async fn get_discovered_wallets(
    State(_state): State<AppState>,
    Query(query): Query<DiscoveredWalletsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    // For now, return empty results as we don't have a storage mechanism for discovered wallets
    // In a real implementation, you'd query the persistence layer for discovered wallet data
    let response = DiscoveredWalletsResponse {
        wallets: vec![],
        pagination: PaginationInfo {
            total_count: 0,
            limit: query.limit.unwrap_or(50),
            offset: query.offset.unwrap_or(0),
            has_more: false,
        },
        summary: DiscoveredWalletsSummary {
            total_discovered: 0,
            analyzed_count: 0,
            profitable_count: 0,
            average_pnl_usd: Decimal::ZERO,
            total_pnl_usd: Decimal::ZERO,
        },
    };

    Ok(Json(SuccessResponse::new(response)))
}

/// Get detailed P&L for a specific discovered wallet
pub async fn get_wallet_details(
    State(_state): State<AppState>,
    Path(wallet_address): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    // For now, return not found as we don't have stored wallet details
    // In a real implementation, you'd look up the wallet in your persistence layer
    Err::<Json<Value>, ApiError>(ApiError::NotFound(format!(
        "Wallet details not found for address: {}",
        wallet_address
    )))
}

/// Get DexClient status
pub async fn get_dex_status(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let dex_guard = state.dex_client.lock().await;
    let status = if let Some(_) = dex_guard.as_ref() {
        DexClientStatus {
            enabled: true,
            connected: true, // TODO: Get actual connection status from DexClient
            last_activity: None, // TODO: Track last activity
            processed_pairs: 0, // TODO: Get actual metrics
            discovered_wallets: 0, // TODO: Get actual metrics
        }
    } else {
        DexClientStatus {
            enabled: false,
            connected: false,
            last_activity: None,
            processed_pairs: 0,
            discovered_wallets: 0,
        }
    };

    Ok(Json(SuccessResponse::new(status)))
}

/// Control DexClient service (start/stop/restart)
pub async fn control_dex_service(
    State(state): State<AppState>,
    Json(request): Json<DexControlRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let action = request.action.to_lowercase();
    
    match action.as_str() {
        "start" => {
            // TODO: Implement DexClient start logic
            warn!("DexClient start requested but not implemented");
        }
        "stop" => {
            // TODO: Implement DexClient stop logic  
            warn!("DexClient stop requested but not implemented");
        }
        "restart" => {
            // TODO: Implement DexClient restart logic
            warn!("DexClient restart requested but not implemented");
        }
        _ => {
            return Err(ApiError::Validation(format!(
                "Invalid action: {}. Valid actions are: start, stop, restart",
                action
            )));
        }
    }

    let dex_guard = state.dex_client.lock().await;
    let status = if let Some(_) = dex_guard.as_ref() {
        DexClientStatus {
            enabled: true,
            connected: true,
            last_activity: None,
            processed_pairs: 0,
            discovered_wallets: 0,
        }
    } else {
        DexClientStatus {
            enabled: false,
            connected: false,
            last_activity: None,
            processed_pairs: 0,
            discovered_wallets: 0,
        }
    };

    let response = DexControlResponse {
        action: request.action,
        success: true, // TODO: Return actual success status
        message: format!("DexClient {} action completed", action),
        status,
    };

    Ok(Json(SuccessResponse::new(response)))
}

/// Generate CSV content for batch job results
fn generate_batch_results_csv(job: &job_orchestrator::BatchJob) -> Result<String, ApiError> {
    let mut wtr = Writer::from_writer(Cursor::new(Vec::new()));

    // Write CSV headers
    wtr.write_record(&[
        "wallet_address",
        "status",
        "total_pnl_usd",
        "realized_pnl_usd",
        "unrealized_pnl_usd",
        "total_trades",
        "winning_trades",
        "losing_trades",
        "win_rate",
        "total_volume_usd",
        "total_fees_usd",
        "first_trade_time",
        "last_trade_time",
        "error_message",
    ])
    .map_err(|e| ApiError::Internal(format!("CSV header error: {}", e)))?;

    // Write data rows
    for (wallet, result) in &job.results {
        let row = match result {
            Ok(report) => vec![
                wallet.clone(),
                "success".to_string(),
                report.summary.total_pnl_usd.to_string(),
                report.summary.realized_pnl_usd.to_string(),
                report.summary.unrealized_pnl_usd.to_string(),
                report.summary.total_trades.to_string(),
                report.summary.winning_trades.to_string(),
                report.summary.losing_trades.to_string(),
                format!("{:.2}%", report.summary.win_rate * Decimal::from(100)),
                "0.00".to_string(), // total_volume_usd field doesn't exist
                report.summary.total_fees_usd.to_string(),
                "".to_string(), // first_trade_time field doesn't exist
                "".to_string(), // last_trade_time field doesn't exist
                String::new(),
            ],
            Err(e) => vec![
                wallet.clone(),
                "failed".to_string(),
                "0".to_string(),
                "0".to_string(),
                "0".to_string(),
                "0".to_string(),
                "0".to_string(),
                "0".to_string(),
                "0%".to_string(),
                "0".to_string(),
                "0".to_string(),
                String::new(),
                String::new(),
                e.to_string(),
            ],
        };

        wtr.write_record(&row)
            .map_err(|e| ApiError::Internal(format!("CSV write error: {}", e)))?;
    }

    let data = wtr.into_inner()
        .map_err(|e| ApiError::Internal(format!("CSV finalization error: {}", e)))?
        .into_inner();

    String::from_utf8(data)
        .map_err(|e| ApiError::Internal(format!("CSV encoding error: {}", e)))
}

/// Filter traders for copy trading from batch job results
pub async fn filter_copy_traders(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    // Get the completed batch job
    let job = state
        .orchestrator
        .get_batch_job_status(job_id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("Job {} not found", job_id)))?;

    if job.status != JobStatus::Completed {
        return Err(ApiError::Validation(format!(
            "Job {} is not completed (status: {:?})",
            job_id, job.status
        )));
    }

    info!("🔍 Filtering traders for copy trading from job {}", job_id);

    // Extract successful P&L reports
    let mut pnl_reports = Vec::new();
    for (wallet, result) in &job.results {
        match result {
            Ok(report) => pnl_reports.push(report.clone()),
            Err(e) => {
                warn!("Skipping failed wallet {}: {}", wallet, e);
            }
        }
    }

    if pnl_reports.is_empty() {
        return Ok(Json(SuccessResponse::new(TraderFilterResponse {
            job_id,
            total_analyzed: job.results.len(),
            qualified_traders: 0,
            traders: Vec::new(),
            summary: "No successful P&L analyses to filter".to_string(),
        })));
    }

    // Create trader filter from config
    let trader_filter = TraderFilter::new(&state.config.trader_filter);

    // Filter traders
    let qualified_traders = trader_filter.filter_traders(pnl_reports)?;
    
    // Convert to response format
    let trader_summaries: Vec<QualifiedTrader> = qualified_traders
        .iter()
        .map(|(report, quality)| QualifiedTrader {
            wallet_address: report.wallet_address.clone(),
            score: quality.score,
            risk_level: format!("{:?}", quality.risk_level),
            trading_style: format!("{:?}", quality.trading_style),
            pnl_summary: TraderPnLSummary {
                total_pnl_usd: report.summary.total_pnl_usd.to_string(),
                realized_pnl_usd: report.summary.realized_pnl_usd.to_string(),
                roi_percentage: report.summary.roi_percentage.to_string(),
                win_rate: report.summary.win_rate.to_string(),
                total_trades: report.summary.total_trades,
                winning_trades: report.summary.winning_trades,
                capital_deployed_sol: report.summary.total_capital_deployed_sol.to_string(),
            },
            strengths: quality.strengths.clone(),
            concerns: quality.concerns.clone(),
            copy_trade_recommended: quality.copy_trade_recommended,
        })
        .collect();

    // Generate summary text
    let summary = generate_trader_summary(&qualified_traders);

    info!("✅ Found {} qualified traders for copy trading out of {} analyzed", 
        qualified_traders.len(), job.results.len());

    let response = TraderFilterResponse {
        job_id,
        total_analyzed: job.results.len(),
        qualified_traders: qualified_traders.len(),
        traders: trader_summaries,
        summary,
    };

    Ok(Json(SuccessResponse::new(response)))
}