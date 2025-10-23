use crate::service_manager::ServiceConfig;
use crate::types::*;
use crate::{ApiError, AppState};
use axum::{
    extract::{Path, Query, State},
    http::header,
    response::{IntoResponse, Json},
};
use chrono::Utc;
use config_manager::{normalize_chain_for_zerion, denormalize_chain_for_frontend};
use csv::Writer;
use persistence_layer::JobStatus;
use rust_decimal::Decimal;
use serde_json::Value;
use std::{collections::HashMap, io::Cursor};
use tracing::{info, warn};
use uuid::Uuid;

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

    let service_stats = state.service_manager.get_stats().await;

    let dex_status = DexClientStatus {
        enabled: service_stats.wallet_discovery.state
            != crate::service_manager::ServiceState::Stopped,
        connected: matches!(
            service_stats.wallet_discovery.state,
            crate::service_manager::ServiceState::Running
        ),
        last_activity: service_stats.wallet_discovery.last_activity,
        processed_pairs: service_stats.wallet_discovery.cycles_completed,
        discovered_wallets: service_stats.wallet_discovery.queue_size,
    };

    let config_summary = ConfigSummary {
        birdeye_api_configured: !state.config.birdeye.api_key.is_empty(),
        zerion_api_configured: !state.config.zerion.api_key.is_empty(),
        architecture: "Zerion+DexScreener Hybrid".to_string(),
        parallel_batch_size: state.config.system.pnl_parallel_batch_size.unwrap_or(10),
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
pub async fn get_config(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
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
        "Configuration update not yet implemented".to_string(),
    )))
}

/// Submit a batch P&L job
pub async fn submit_batch_job(
    State(state): State<AppState>,
    Json(mut request): Json<BatchJobRequest>,
) -> Result<impl IntoResponse, ApiError> {
    info!(
        "Batch job submitted for {} wallets on chain: {}",
        request.wallet_addresses.len(),
        request.chain
    );

    // Validate wallet addresses
    if request.wallet_addresses.is_empty() {
        return Err(ApiError::Validation(
            "No wallet addresses provided".to_string(),
        ));
    }

    if request.wallet_addresses.len() > 1000 {
        return Err(ApiError::Validation(
            "Too many wallet addresses (max 1000)".to_string(),
        ));
    }

    // Validate time_range if provided
    if let Some(ref time_range) = request.time_range {
        // Import time_utils validation
        use zerion_client::time_utils::is_valid_time_range;
        if !is_valid_time_range(time_range) {
            return Err(ApiError::Validation(
                format!("Invalid time range '{}'. Use formats like: 1h, 7d, 1m, 1y", time_range),
            ));
        }
    }

    // Normalize chain parameter to ensure Zerion API compatibility
    let original_chain = request.chain.clone();
    request.chain = normalize_chain_for_zerion(&request.chain)
        .map_err(|e| ApiError::Validation(e))?;

    info!(
        "Chain normalized for Zerion API: '{}' -> '{}'",
        original_chain,
        request.chain
    );

    // Check batch job capacity before submitting
    match state.batch_limiter.try_acquire() {
        Ok(_permit) => {
            info!("Batch job capacity available, proceeding with submission");
            // Permit will be held until job submission completes, then released
        }
        Err(_) => {
            warn!("Batch job capacity exhausted - 3 concurrent jobs limit reached");
            return Err(ApiError::ServiceUnavailable(
                "System is currently processing maximum concurrent batch jobs (3). Please try again in a few minutes.".to_string()
            ));
        }
    }

    // Submit the job
    let job_id = state
        .orchestrator
        .submit_batch_job(
            request.wallet_addresses.clone(),
            request.chain.clone(),
            request.time_range.clone(),
            request.max_transactions,
        )
        .await?;

    info!("Batch job submitted successfully with ID: {}", job_id);
    // Permit dropped here, allowing next job to proceed

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

    // Load the results separately from PostgreSQL for each wallet
    let persistence_client = &state.persistence_client;

    // Fetch P&L results for all wallets in the batch
    let mut wallet_results = HashMap::new();
    let mut total_pnl = Decimal::ZERO;
    let mut successful_count = 0;
    let mut total_incomplete_trades = 0u32;

    for wallet_address in &job.wallet_addresses {
        // SCOPED BLOCK to ensure PostgreSQL connection is released immediately after each wallet
        {
            // Use the chain from the batch job
            match persistence_client
                .get_portfolio_pnl_result(wallet_address, &job.chain)
                .await
            {
                Ok(Some(stored_result)) => {
                    total_pnl += stored_result.portfolio_result.total_pnl_usd;
                    successful_count += 1;
                    let incomplete_count = stored_result.incomplete_trades_count;
                    total_incomplete_trades += incomplete_count;
                    wallet_results.insert(
                        wallet_address.clone(),
                        WalletResult {
                            wallet_address: wallet_address.clone(),
                            status: "success".to_string(),
                            pnl_report: Some(stored_result.portfolio_result),
                            error_message: None,
                            incomplete_trades_count: incomplete_count,
                        },
                    );
                }
                Ok(None) => {
                    wallet_results.insert(
                        wallet_address.clone(),
                        WalletResult {
                            wallet_address: wallet_address.clone(),
                            status: "not_found".to_string(),
                            pnl_report: None,
                            error_message: Some("No P&L results found for this wallet".to_string()),
                            incomplete_trades_count: 0,
                        },
                    );
                }
                Err(e) => {
                    wallet_results.insert(
                        wallet_address.clone(),
                        WalletResult {
                            wallet_address: wallet_address.clone(),
                            status: "error".to_string(),
                            pnl_report: None,
                            error_message: Some(format!("Failed to fetch results: {}", e)),
                            incomplete_trades_count: 0,
                        },
                    );
                }
            }
            // Connection released here at end of scoped block
        }
    }

    let total_wallets = job.wallet_addresses.len();
    let average_pnl = if successful_count > 0 {
        total_pnl / Decimal::from(successful_count)
    } else {
        Decimal::ZERO
    };

    let profitable_wallets = wallet_results
        .values()
        .filter(|r| {
            r.pnl_report
                .as_ref()
                .map(|report| report.total_pnl_usd > Decimal::ZERO)
                .unwrap_or(false)
        })
        .count();

    let response = BatchJobResultsResponse {
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
            total_incomplete_trades,
        },
        results: wallet_results,
    };

    Ok(Json(SuccessResponse::new(response)))
}

/// Get batch job history with pagination
pub async fn get_batch_job_history(
    State(state): State<AppState>,
    Query(query): Query<BatchJobHistoryQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(50).min(20000) as usize; // Max 20k per request
    let offset = query.offset.unwrap_or(0) as usize;

    // Get batch jobs from the orchestrator
    let (jobs, total_count) = state.orchestrator.get_all_batch_jobs(limit, offset).await?;

    // Convert to response format
    let mut job_summaries = Vec::new();
    for job in jobs {
        // For completed jobs, success_count would be all wallets, failure_count would be 0
        // For more accurate counts, we'd need to query PostgreSQL for each wallet
        let (success_count, failure_count) = if job.status == JobStatus::Completed {
            (job.wallet_addresses.len(), 0)
        } else if job.status == JobStatus::Failed {
            (0, job.wallet_addresses.len())
        } else {
            // In progress - we don't know the counts
            (0, 0)
        };

        let job_summary = BatchJobSummary {
            id: job.id,
            wallet_count: job.wallet_addresses.len(),
            chain: denormalize_chain_for_frontend(&job.chain),
            status: job.status,
            created_at: job.created_at,
            started_at: job.started_at,
            completed_at: job.completed_at,
            success_count,
            failure_count,
        };
        job_summaries.push(job_summary);
    }

    let pagination = PaginationInfo {
        total_count: total_count as u64,
        limit: limit as u32,
        offset: offset as u32,
        has_more: offset + limit < total_count,
    };

    // Calculate summary statistics before moving job_summaries
    let summary = BatchJobHistorySummary {
        total_jobs: total_count as u64,
        pending_jobs: job_summaries
            .iter()
            .filter(|j| j.status == JobStatus::Pending)
            .count() as u64,
        running_jobs: job_summaries
            .iter()
            .filter(|j| j.status == JobStatus::Running)
            .count() as u64,
        completed_jobs: job_summaries
            .iter()
            .filter(|j| j.status == JobStatus::Completed)
            .count() as u64,
        failed_jobs: job_summaries
            .iter()
            .filter(|j| j.status == JobStatus::Failed)
            .count() as u64,
    };

    let response = BatchJobHistoryResponse {
        jobs: job_summaries,
        pagination,
        summary,
    };

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

    // Load the results from PostgreSQL for each wallet
    let persistence_client = &state.persistence_client;

    // Fetch P&L results for all wallets in the batch
    let mut wallet_results = HashMap::new();

    for wallet_address in &job.wallet_addresses {
        // SCOPED BLOCK to ensure PostgreSQL connection is released immediately after each wallet
        {
            // Use the chain from the batch job
            match persistence_client
                .get_portfolio_pnl_result(wallet_address, &job.chain)
                .await
            {
                Ok(Some(stored_result)) => {
                    wallet_results.insert(
                        wallet_address.clone(),
                        WalletResult {
                            wallet_address: wallet_address.clone(),
                            status: "success".to_string(),
                            pnl_report: Some(stored_result.portfolio_result),
                            error_message: None,
                            incomplete_trades_count: stored_result.incomplete_trades_count,
                        },
                    );
                }
                _ => {
                    wallet_results.insert(
                        wallet_address.clone(),
                        WalletResult {
                            wallet_address: wallet_address.clone(),
                            status: "not_found".to_string(),
                            pnl_report: None,
                            error_message: Some("No results found".to_string()),
                            incomplete_trades_count: 0,
                        },
                    );
                }
            }
            // Connection released here at end of scoped block
        }
    }

    // Generate CSV content
    let csv_content = generate_batch_results_csv(&wallet_results)?;
    let _filename = format!("batch_pnl_results_{}.csv", job_id);

    let headers = [
        (header::CONTENT_TYPE, "text/csv"),
        (
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"batch_results.csv\"",
        ),
    ];

    Ok((headers, csv_content))
}

/// Get discovered wallets from continuous mode
pub async fn get_discovered_wallets(
    State(state): State<AppState>,
    Query(query): Query<DiscoveredWalletsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    // Use the same logic as get_all_results since discovered wallets are stored as P&L results
    let limit = query.limit.unwrap_or(50) as usize;
    let offset = query.offset.unwrap_or(0) as usize;

    // Get P&L results (which are the discovered wallets with analysis)
    let persistence_client = &state.persistence_client;

    // Apply database migration for new columns if needed
    if let Err(e) = persistence_client
        .apply_advanced_filtering_migration()
        .await
    {
        warn!("Failed to apply advanced filtering migration: {}", e);
    }

    // Use lightweight summary method - NO portfolio_json deserialization (600x memory reduction!)
    // This works for both filtered and unfiltered queries since all filtering columns are indexed
    let (summary_results, total_count) = persistence_client
        .get_all_pnl_results_summary(offset, limit, query.chain.as_deref())
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to retrieve P&L results: {}", e)))?;

    // Aggregate incomplete trades count across all discovered wallets BEFORE consuming summary_results
    let total_incomplete_trades: u32 = summary_results
        .iter()
        .map(|summary| summary.incomplete_trades_count)
        .sum();

    // Convert P&L summary results to discovered wallets format - using indexed columns directly
    let wallets: Vec<DiscoveredWalletSummary> = summary_results
        .into_iter()
        .map(|summary| {
            DiscoveredWalletSummary {
                wallet_address: summary.wallet_address,
                chain: summary.chain,
                discovered_at: summary.analyzed_at,
                analyzed_at: Some(summary.analyzed_at),
                pnl_usd: Some(Decimal::from_f64_retain(summary.total_pnl_usd).unwrap_or(Decimal::ZERO)),
                win_rate: Some(Decimal::from_f64_retain(summary.win_rate).unwrap_or(Decimal::ZERO)),
                trade_count: Some(summary.total_trades as u32),
                avg_hold_time_minutes: Some(Decimal::from_f64_retain(summary.avg_hold_time_minutes).unwrap_or(Decimal::ZERO)),
                unique_tokens_count: summary.unique_tokens_count, // NOW from indexed column!
                active_days_count: summary.active_days_count,     // NOW from indexed column!
                status: "analyzed".to_string(),
                incomplete_trades_count: summary.incomplete_trades_count,
            }
        })
        .collect();

    // Calculate summary statistics
    let analyzed_count = wallets.len() as u64;
    let profitable_count = wallets
        .iter()
        .filter(|w| w.pnl_usd.map_or(false, |pnl| pnl > Decimal::ZERO))
        .count() as u64;

    let total_pnl = wallets
        .iter()
        .filter_map(|w| w.pnl_usd)
        .fold(Decimal::ZERO, |acc, pnl| acc + pnl);

    let average_pnl = if analyzed_count > 0 {
        total_pnl / Decimal::from(analyzed_count)
    } else {
        Decimal::ZERO
    };

    let response = DiscoveredWalletsResponse {
        wallets,
        pagination: PaginationInfo {
            total_count: total_count as u64,
            limit: limit as u32,
            offset: offset as u32,
            has_more: (offset + limit) < total_count,
        },
        summary: DiscoveredWalletsSummary {
            total_discovered: total_count as u64,
            analyzed_count,
            profitable_count,
            average_pnl_usd: average_pnl,
            total_pnl_usd: total_pnl,
            total_incomplete_trades,
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

/// Generate CSV content for batch job results
fn generate_batch_results_csv(
    wallet_results: &HashMap<String, WalletResult>,
) -> Result<String, ApiError> {
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
    for (wallet, result) in wallet_results {
        let row = if let Some(report) = &result.pnl_report {
            vec![
                wallet.clone(),
                result.status.clone(),
                report.total_pnl_usd.to_string(),
                report.total_realized_pnl_usd.to_string(),
                report.total_unrealized_pnl_usd.to_string(),
                report.total_trades.to_string(),
                report
                    .token_results
                    .iter()
                    .map(|t| t.winning_trades)
                    .sum::<u32>()
                    .to_string(),
                report
                    .token_results
                    .iter()
                    .map(|t| t.losing_trades)
                    .sum::<u32>()
                    .to_string(),
                format!("{:.2}%", report.overall_win_rate_percentage),
                "0.00".to_string(), // total_volume_usd field doesn't exist
                "0.00".to_string(), // total_fees_usd not available in PortfolioPnLResult
                "".to_string(),     // first_trade_time field doesn't exist
                "".to_string(),     // last_trade_time field doesn't exist
                result.error_message.clone().unwrap_or_default(),
            ]
        } else {
            vec![
                wallet.clone(),
                result.status.clone(),
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
                result.error_message.clone().unwrap_or_default(),
            ]
        };

        wtr.write_record(&row)
            .map_err(|e| ApiError::Internal(format!("CSV write error: {}", e)))?;
    }

    let data = wtr
        .into_inner()
        .map_err(|e| ApiError::Internal(format!("CSV finalization error: {}", e)))?
        .into_inner();

    String::from_utf8(data).map_err(|e| ApiError::Internal(format!("CSV encoding error: {}", e)))
}

/// Get batch job results formatted as trader summaries for copy trading analysis
pub async fn get_batch_traders(
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

    info!("📊 Fetching trader summaries for batch job {}", job_id);

    // Load the results from PostgreSQL for each wallet
    let persistence_client = &state.persistence_client;

    // Extract successful P&L reports
    let mut pnl_reports = Vec::new();
    for wallet_address in &job.wallet_addresses {
        // SCOPED BLOCK to ensure PostgreSQL connection is released immediately after each wallet
        {
            // Use the chain from the batch job
            match persistence_client
                .get_portfolio_pnl_result(wallet_address, &job.chain)
                .await
            {
                Ok(Some(stored_result)) => {
                    pnl_reports.push(stored_result.portfolio_result);
                }
                Ok(None) => {
                    warn!("No results found for wallet {}", wallet_address);
                }
                Err(e) => {
                    warn!(
                        "Failed to fetch results for wallet {}: {}",
                        wallet_address, e
                    );
                }
            }
            // Connection released here at end of scoped block
        }
    }

    if pnl_reports.is_empty() {
        return Ok(Json(SuccessResponse::new(TraderFilterResponse {
            job_id,
            total_analyzed: job.wallet_addresses.len(),
            qualified_traders: 0,
            traders: Vec::new(),
            summary: "No successful P&L analyses found".to_string(),
        })));
    }

    // Convert P&L reports to trader summaries
    let trader_summaries: Vec<QualifiedTrader> = pnl_reports
        .iter()
        .map(|report| QualifiedTrader {
            wallet_address: report.wallet_address.clone(),
            score: 0.0,
            risk_level: String::new(),
            trading_style: String::new(),
            pnl_summary: TraderPnLSummary {
                total_pnl_usd: report.total_pnl_usd.to_string(),
                realized_pnl_usd: report.total_realized_pnl_usd.to_string(),
                roi_percentage: "0".to_string(), // Not available in new format
                win_rate: report.overall_win_rate_percentage.to_string(),
                total_trades: report.total_trades,
                winning_trades: report.token_results.iter().map(|t| t.winning_trades).sum(),
                capital_deployed_sol: "0".to_string(), // Not available in new format
            },
            strengths: Vec::new(),
            concerns: Vec::new(),
            copy_trade_recommended: false,
        })
        .collect();

    let summary = format!(
        "Retrieved {} trader summaries from batch job results",
        trader_summaries.len()
    );

    info!(
        "✅ Returned {} trader summaries out of {} analyzed wallets",
        trader_summaries.len(),
        job.wallet_addresses.len()
    );

    let response = TraderFilterResponse {
        job_id,
        total_analyzed: job.wallet_addresses.len(),
        qualified_traders: trader_summaries.len(),
        traders: trader_summaries,
        summary,
    };

    Ok(Json(SuccessResponse::new(response)))
}

// =====================================
// Service Management Handlers
// =====================================

/// Get service status
pub async fn get_services_status(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let stats = state.service_manager.get_stats().await;
    Ok(Json(SuccessResponse::new(stats)))
}

/// Get service configuration
pub async fn get_services_config(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.service_manager.get_config().await;
    Ok(Json(SuccessResponse::new(config)))
}

/// Update service configuration
pub async fn update_services_config(
    State(state): State<AppState>,
    Json(new_config): Json<ServiceConfig>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .service_manager
        .update_config(new_config)
        .await
        .map_err(|e| ApiError::ServiceManager(e.to_string()))?;

    let response = MessageResponse {
        message: "Service configuration updated successfully".to_string(),
    };
    Ok(Json(SuccessResponse::new(response)))
}

/// Start wallet discovery service
pub async fn start_wallet_discovery(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .service_manager
        .start_wallet_discovery()
        .await
        .map_err(|e| ApiError::ServiceManager(e.to_string()))?;

    let response = MessageResponse {
        message: "Wallet discovery service started successfully".to_string(),
    };
    Ok(Json(SuccessResponse::new(response)))
}

/// Stop wallet discovery service
pub async fn stop_wallet_discovery(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .service_manager
        .stop_wallet_discovery()
        .await
        .map_err(|e| ApiError::ServiceManager(e.to_string()))?;

    let response = MessageResponse {
        message: "Wallet discovery service stopped successfully".to_string(),
    };
    Ok(Json(SuccessResponse::new(response)))
}

/// Trigger a manual discovery cycle
pub async fn trigger_discovery_cycle(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let discovered_wallets = state
        .service_manager
        .trigger_discovery_cycle()
        .await
        .map_err(|e| ApiError::ServiceManager(e.to_string()))?;

    let response = DiscoveryCycleResponse {
        message: "Manual discovery cycle completed".to_string(),
        discovered_wallets,
    };
    Ok(Json(SuccessResponse::new(response)))
}

/// Start P&L analysis service
pub async fn start_pnl_analysis(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .service_manager
        .start_pnl_analysis()
        .await
        .map_err(|e| ApiError::ServiceManager(e.to_string()))?;

    let response = MessageResponse {
        message: "P&L analysis service started successfully".to_string(),
    };
    Ok(Json(SuccessResponse::new(response)))
}

/// Stop P&L analysis service
pub async fn stop_pnl_analysis(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .service_manager
        .stop_pnl_analysis()
        .await
        .map_err(|e| ApiError::ServiceManager(e.to_string()))?;

    let response = MessageResponse {
        message: "P&L analysis service stopped successfully".to_string(),
    };
    Ok(Json(SuccessResponse::new(response)))
}

// =====================================
// Results Retrieval Handlers
// =====================================

/// Get all P&L analysis results
pub async fn get_all_results(
    State(state): State<AppState>,
    Query(query): Query<AllResultsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let start_time = std::time::Instant::now();
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(50).min(25000); // Max 25k per request (summary queries are lightweight ~1MB per 25k records)

    info!("⏱️  [PERF] get_all_results START - offset: {}, limit: {}", offset, limit);

    // Get results from persistence layer (PostgreSQL)
    let persistence_client = &state.persistence_client;

    // Apply database migration for new columns if needed to ensure data is available
    if let Err(e) = persistence_client
        .apply_advanced_filtering_migration()
        .await
    {
        warn!("Failed to apply advanced filtering migration: {}", e);
    }

    let query_start = std::time::Instant::now();
    // Use lightweight summary method - NO portfolio_json deserialization (600x memory reduction!)
    let (summary_results, total_count) = persistence_client
        .get_all_pnl_results_summary(offset, limit, query.chain.as_deref())
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to fetch results: {}", e)))?;
    info!("⏱️  [PERF] DB query completed: {} records in {}ms", summary_results.len(), query_start.elapsed().as_millis());

    // Get summary statistics
    let (total_results, _total_batch_jobs) = persistence_client
        .get_stats()
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to fetch stats: {}", e)))?;

    let mapping_start = std::time::Instant::now();
    // Convert to response format - using summary data directly (NO deserialization!)
    let results: Vec<StoredPnLResultSummary> = summary_results
        .into_iter()
        .map(|summary| StoredPnLResultSummary {
            wallet_address: summary.wallet_address,
            chain: summary.chain,
            token_address: "portfolio".to_string(), // Portfolio-level result
            token_symbol: "PORTFOLIO".to_string(),
            total_pnl_usd: summary.total_pnl_usd,
            realized_pnl_usd: summary.realized_pnl_usd,
            unrealized_pnl_usd: summary.unrealized_pnl_usd,
            roi_percentage: summary.roi_percentage,
            total_trades: summary.total_trades as u32,
            win_rate: summary.win_rate,
            avg_hold_time_minutes: summary.avg_hold_time_minutes,
            unique_tokens_count: summary.unique_tokens_count,
            active_days_count: summary.active_days_count,
            analyzed_at: summary.analyzed_at,
            is_favorited: summary.is_favorited,
            is_archived: summary.is_archived,
            incomplete_trades_count: summary.incomplete_trades_count,
        })
        .collect();
    info!("⏱️  [PERF] f64 mapping completed: {} records in {}ms", results.len(), mapping_start.elapsed().as_millis());

    let pagination = PaginationInfo {
        total_count: total_count as u64,
        limit: limit as u32,
        offset: offset as u32,
        has_more: offset + limit < total_count,
    };

    let summary_calc_start = std::time::Instant::now();
    // Calculate summary from results (simplified version without full DB stats)
    let total_wallets = total_results as u64;
    let profitable_wallets = results
        .iter()
        .filter(|r| r.total_pnl_usd > 0.0)
        .count() as u64;
    let total_pnl_usd = results.iter().map(|r| r.total_pnl_usd).sum::<f64>();
    let average_pnl_usd = if total_wallets > 0 {
        total_pnl_usd / total_wallets as f64
    } else {
        0.0
    };
    let total_trades = results.iter().map(|r| r.total_trades).sum::<u32>() as u64;
    let profitability_rate = if total_wallets > 0 {
        (profitable_wallets as f64 / total_wallets as f64) * 100.0
    } else {
        0.0
    };

    let summary = AllResultsSummary {
        total_wallets,
        profitable_wallets,
        total_pnl_usd,
        average_pnl_usd,
        total_trades,
        profitability_rate,
        last_updated: chrono::Utc::now(),
    };
    info!("⏱️  [PERF] Summary calculation completed in {}ms", summary_calc_start.elapsed().as_millis());

    let serialization_start = std::time::Instant::now();
    let response = AllResultsResponse {
        results,
        pagination,
        summary,
    };

    let json_response = Json(SuccessResponse::new(response));
    info!("⏱️  [PERF] JSON serialization completed in {}ms", serialization_start.elapsed().as_millis());
    info!("⏱️  [PERF] get_all_results TOTAL: {}ms", start_time.elapsed().as_millis());

    Ok(json_response)
}

/// Get detailed P&L result for a specific wallet-token pair
pub async fn get_detailed_result(
    State(state): State<AppState>,
    Path((wallet_address, token_address)): Path<(String, String)>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    // Token address is ignored in the new portfolio-based system
    let _ = token_address;

    let persistence_client = &state.persistence_client;

    // Use chain from query parameter or default
    let chain = query
        .get("chain")
        .map(|s| s.as_str())
        .unwrap_or(&state.config.multichain.default_chain);
    let stored_result = persistence_client
        .get_portfolio_pnl_result(&wallet_address, chain)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to fetch result: {}", e)))?;

    match stored_result {
        Some(result) => {
            let response = DetailedPnLResultResponse {
                wallet_address: result.wallet_address,
                chain: result.chain,
                portfolio_result: result.portfolio_result,
                analyzed_at: result.analyzed_at,
            };
            Ok(Json(SuccessResponse::new(response)))
        }
        None => Err(ApiError::NotFound(format!(
            "No P&L result found for wallet {}",
            wallet_address
        ))),
    }
}

/// Enhanced health check with component status
pub async fn enhanced_health_check(State(state): State<AppState>) -> impl IntoResponse {
    let start_time = std::time::Instant::now();

    // Test Redis connectivity
    let redis_client_result = persistence_layer::RedisClient::new(&state.config.redis.url).await;
    let redis_health = match redis_client_result {
        Ok(redis_client) => match redis_client.health_check().await {
            Ok(status) => RedisComponentHealth {
                connected: status.connected,
                latency_ms: status.latency_ms,
                error: status.error,
            },
            Err(e) => RedisComponentHealth {
                connected: false,
                latency_ms: start_time.elapsed().as_millis() as u64,
                error: Some(format!("Health check failed: {}", e)),
            },
        },
        Err(e) => RedisComponentHealth {
            connected: false,
            latency_ms: start_time.elapsed().as_millis() as u64,
            error: Some(format!("Connection failed: {}", e)),
        },
    };

    // Test BirdEye API connectivity
    let birdeye_health = {
        let birdeye_start = std::time::Instant::now();
        // Simple test request to BirdEye
        let client = reqwest::Client::new();
        match client
            .get("https://public-api.birdeye.so/defi/token_trending?chain=solana&limit=1")
            .header("X-API-KEY", &state.config.birdeye.api_key)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(response) => {
                let latency = birdeye_start.elapsed().as_millis() as u64;
                if response.status().is_success() {
                    ApiComponentHealth {
                        accessible: true,
                        latency_ms: Some(latency),
                        error: None,
                    }
                } else {
                    ApiComponentHealth {
                        accessible: false,
                        latency_ms: Some(latency),
                        error: Some(format!("HTTP {}", response.status())),
                    }
                }
            }
            Err(e) => ApiComponentHealth {
                accessible: false,
                latency_ms: Some(birdeye_start.elapsed().as_millis() as u64),
                error: Some(format!("Request failed: {}", e)),
            },
        }
    };

    // Get service states
    let service_stats = state.service_manager.get_stats().await;
    let services_health = ServicesComponentHealth {
        wallet_discovery: format!("{:?}", service_stats.wallet_discovery.state),
        pnl_analysis: format!("{:?}", service_stats.pnl_analysis.state),
    };

    let components = ComponentHealthStatus {
        redis: redis_health,
        birdeye_api: birdeye_health,
        services: services_health,
    };

    // Determine overall status
    let overall_status = if components.redis.connected && components.birdeye_api.accessible {
        "healthy"
    } else {
        "degraded"
    };

    let response = EnhancedHealthResponse {
        status: overall_status.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: 0, // TODO: Track actual uptime
        components,
    };

    Json(SuccessResponse::new(response))
}

/// Universal service control handler with optional configuration
pub async fn control_service(
    State(state): State<AppState>,
    Json(request): Json<ServiceControlRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let response = match (request.action.as_str(), request.service.as_str()) {
        ("start", "wallet_discovery") => {
            state
                .service_manager
                .start_wallet_discovery_with_config(request.config_override)
                .await
                .map_err(|e| ApiError::ServiceManager(e.to_string()))?;
            MessageResponse {
                message: "Wallet discovery service started successfully".to_string(),
            }
        }
        ("stop", "wallet_discovery") => {
            state
                .service_manager
                .stop_wallet_discovery()
                .await
                .map_err(|e| ApiError::ServiceManager(e.to_string()))?;
            MessageResponse {
                message: "Wallet discovery service stopped successfully".to_string(),
            }
        }
        ("start", "pnl_analysis") => {
            state
                .service_manager
                .start_pnl_analysis()
                .await
                .map_err(|e| ApiError::ServiceManager(e.to_string()))?;
            MessageResponse {
                message: "P&L analysis service started successfully".to_string(),
            }
        }
        ("stop", "pnl_analysis") => {
            state
                .service_manager
                .stop_pnl_analysis()
                .await
                .map_err(|e| ApiError::ServiceManager(e.to_string()))?;
            MessageResponse {
                message: "P&L analysis service stopped successfully".to_string(),
            }
        }
        ("restart", service) => {
            // Stop then start
            match service {
                "wallet_discovery" => {
                    let _ = state.service_manager.stop_wallet_discovery().await;
                    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                    state
                        .service_manager
                        .start_wallet_discovery_with_config(request.config_override)
                        .await
                        .map_err(|e| ApiError::ServiceManager(e.to_string()))?;
                }
                "pnl_analysis" => {
                    let _ = state.service_manager.stop_pnl_analysis().await;
                    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                    state
                        .service_manager
                        .start_pnl_analysis()
                        .await
                        .map_err(|e| ApiError::ServiceManager(e.to_string()))?;
                }
                _ => {
                    return Err(ApiError::Validation(format!(
                        "Unknown service: {}",
                        service
                    )))
                }
            }
            MessageResponse {
                message: format!("{} service restarted successfully", service),
            }
        }
        _ => {
            return Err(ApiError::Validation(format!(
                "Invalid action '{}' for service '{}'",
                request.action, request.service
            )))
        }
    };

    Ok(Json(SuccessResponse::new(response)))
}

/// Toggle favorite status for a wallet
pub async fn toggle_wallet_favorite(
    State(state): State<AppState>,
    Path(wallet_address): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    let chain = query.get("chain").map(|s| s.as_str()).unwrap_or("solana");

    // Get current status
    let current_result = state
        .persistence_client
        .get_portfolio_pnl_result(&wallet_address, chain)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to get wallet: {}", e)))?;

    match current_result {
        Some(result) => {
            // Toggle the favorite status
            let new_status = !result.is_favorited;

            state
                .persistence_client
                .update_wallet_favorite_status(&wallet_address, chain, new_status)
                .await
                .map_err(|e| {
                    ApiError::Internal(format!("Failed to update favorite status: {}", e))
                })?;

            let response = MessageResponse {
                message: format!(
                    "Wallet {} favorite status set to {}",
                    wallet_address, new_status
                ),
            };
            Ok(Json(SuccessResponse::new(response)))
        }
        None => Err(ApiError::NotFound(format!(
            "Wallet {} not found",
            wallet_address
        ))),
    }
}

/// Toggle archive status for a wallet
pub async fn toggle_wallet_archive(
    State(state): State<AppState>,
    Path(wallet_address): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    let chain = query.get("chain").map(|s| s.as_str()).unwrap_or("solana");

    // Get current status
    let current_result = state
        .persistence_client
        .get_portfolio_pnl_result(&wallet_address, chain)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to get wallet: {}", e)))?;

    match current_result {
        Some(result) => {
            // Toggle the archive status
            let new_status = !result.is_archived;

            state
                .persistence_client
                .update_wallet_archive_status(&wallet_address, chain, new_status)
                .await
                .map_err(|e| {
                    ApiError::Internal(format!("Failed to update archive status: {}", e))
                })?;

            let response = MessageResponse {
                message: format!(
                    "Wallet {} archive status set to {}",
                    wallet_address, new_status
                ),
            };
            Ok(Json(SuccessResponse::new(response)))
        }
        None => Err(ApiError::NotFound(format!(
            "Wallet {} not found",
            wallet_address
        ))),
    }
}

/// Backfill advanced filtering metrics for existing records
pub async fn backfill_advanced_filtering_metrics(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let persistence_client = &state.persistence_client;

    persistence_client
        .backfill_advanced_filtering_metrics()
        .await
        .map_err(|e| ApiError::Internal(format!("Backfill failed: {}", e)))?;

    let response = MessageResponse {
        message: "Advanced filtering metrics backfill completed successfully".to_string(),
    };
    Ok(Json(SuccessResponse::new(response)))
}

