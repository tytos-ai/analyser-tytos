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
use crate::service_manager::ServiceConfig;

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
        enabled: service_stats.wallet_discovery.state != crate::service_manager::ServiceState::Stopped,
        connected: matches!(service_stats.wallet_discovery.state, crate::service_manager::ServiceState::Running),
        last_activity: service_stats.wallet_discovery.last_activity,
        processed_pairs: service_stats.wallet_discovery.cycles_completed,
        discovered_wallets: service_stats.wallet_discovery.queue_size,
    };

    let config_summary = ConfigSummary {
        redis_mode: state.config.system.redis_mode,
        birdeye_api_configured: !state.config.birdeye.api_key.is_empty(),
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
    let mut job = state
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

    // Load the results separately from Redis
    let results = state
        .orchestrator
        .get_batch_job_results(&job_id.to_string())
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to load batch job results: {}", e)))?;

    // Populate the job results for conversion
    job.results = results;

    let response: BatchJobResultsResponse = job.into();
    Ok(Json(SuccessResponse::new(response)))
}

/// Get batch job history with pagination
pub async fn get_batch_job_history(
    State(state): State<AppState>,
    Query(query): Query<BatchJobHistoryQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(50).min(200) as usize; // Max 200 per request
    let offset = query.offset.unwrap_or(0) as usize;
    
    // Get batch jobs from the orchestrator
    let (jobs, total_count) = state
        .orchestrator
        .get_all_batch_jobs(limit, offset)
        .await?;
    
    // Convert to response format with loaded results
    let mut job_summaries = Vec::new();
    for mut job in jobs {
        // Load results for each completed job to get accurate counts
        if job.status == job_orchestrator::JobStatus::Completed {
            match state.orchestrator.get_batch_job_results(&job.id.to_string()).await {
                Ok(results) => {
                    job.results = results;
                }
                Err(e) => {
                    // Log error but continue with empty results
                    tracing::warn!("Failed to load results for job {}: {}", job.id, e);
                }
            }
        }
        
        let job_summary = BatchJobSummary {
            id: job.id,
            wallet_count: job.wallet_addresses.len(),
            status: job.status,
            created_at: job.created_at,
            started_at: job.started_at,
            completed_at: job.completed_at,
            success_count: job.results.values().filter(|r| r.is_ok()).count(),
            failure_count: job.results.values().filter(|r| r.is_err()).count(),
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
        pending_jobs: job_summaries.iter().filter(|j| j.status == JobStatus::Pending).count() as u64,
        running_jobs: job_summaries.iter().filter(|j| j.status == JobStatus::Running).count() as u64,
        completed_jobs: job_summaries.iter().filter(|j| j.status == JobStatus::Completed).count() as u64,
        failed_jobs: job_summaries.iter().filter(|j| j.status == JobStatus::Failed).count() as u64,
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
    State(state): State<AppState>,
    Query(query): Query<DiscoveredWalletsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    // Use the same logic as get_all_results since discovered wallets are stored as P&L results
    let limit = query.limit.unwrap_or(50) as usize;
    let offset = query.offset.unwrap_or(0) as usize;
    
    // Get P&L results (which are the discovered wallets with analysis)
    let redis_client = persistence_layer::RedisClient::new(&state.config.redis.url).await
        .map_err(|e| ApiError::Internal(format!("Redis connection error: {}", e)))?;
    
    let (results, total_count) = redis_client.get_all_pnl_results(offset, limit).await
        .map_err(|e| ApiError::Internal(format!("Failed to retrieve P&L results: {}", e)))?;

    // Convert P&L results to discovered wallets format
    let wallets: Vec<DiscoveredWalletSummary> = results.into_iter().map(|result| {
        DiscoveredWalletSummary {
            wallet_address: result.wallet_address,
            discovered_at: result.analyzed_at,
            analyzed_at: Some(result.analyzed_at),
            pnl_usd: Some(result.pnl_report.summary.total_pnl_usd),
            win_rate: Some(result.pnl_report.summary.win_rate),
            trade_count: Some(result.pnl_report.summary.total_trades as u32),
            status: "analyzed".to_string(),
        }
    }).collect();

    // Calculate summary statistics
    let analyzed_count = wallets.len() as u64;
    let profitable_count = wallets.iter().filter(|w| {
        w.pnl_usd.map_or(false, |pnl| pnl > Decimal::ZERO)
    }).count() as u64;
    
    let total_pnl = wallets.iter()
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
    state.service_manager.update_config(new_config).await
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
    state.service_manager.start_wallet_discovery().await
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
    state.service_manager.stop_wallet_discovery().await
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
    let discovered_wallets = state.service_manager.trigger_discovery_cycle().await
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
    state.service_manager.start_pnl_analysis().await
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
    state.service_manager.stop_pnl_analysis().await
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
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(50).min(200); // Max 200 per request
    
    // Get results from persistence layer
    let redis_client = persistence_layer::RedisClient::new(&state.config.redis.url).await
        .map_err(|e| ApiError::Internal(format!("Redis connection error: {}", e)))?;
    
    let (stored_results, total_count) = redis_client.get_all_pnl_results(offset, limit).await
        .map_err(|e| ApiError::Internal(format!("Failed to fetch results: {}", e)))?;
    
    // Get summary statistics
    let summary_stats = redis_client.get_pnl_summary_stats().await
        .map_err(|e| ApiError::Internal(format!("Failed to fetch summary: {}", e)))?;
    
    // Convert to response format
    let results: Vec<StoredPnLResultSummary> = stored_results
        .into_iter()
        .map(|stored_result| StoredPnLResultSummary {
            wallet_address: stored_result.wallet_address,
            token_address: stored_result.token_address,
            token_symbol: stored_result.token_symbol,
            total_pnl_usd: stored_result.pnl_report.summary.total_pnl_usd,
            realized_pnl_usd: stored_result.pnl_report.summary.realized_pnl_usd,
            unrealized_pnl_usd: stored_result.pnl_report.summary.unrealized_pnl_usd,
            roi_percentage: stored_result.pnl_report.summary.roi_percentage,
            total_trades: stored_result.pnl_report.summary.total_trades,
            win_rate: stored_result.pnl_report.summary.win_rate,
            analyzed_at: stored_result.analyzed_at,
        })
        .collect();
    
    let pagination = PaginationInfo {
        total_count: total_count as u64,
        limit: limit as u32,
        offset: offset as u32,
        has_more: offset + limit < total_count,
    };
    
    let summary = AllResultsSummary {
        total_wallets: summary_stats.total_wallets_analyzed,
        profitable_wallets: summary_stats.profitable_wallets,
        total_pnl_usd: summary_stats.total_pnl_usd,
        average_pnl_usd: summary_stats.average_pnl_usd,
        total_trades: summary_stats.total_trades,
        profitability_rate: summary_stats.profitability_rate,
        last_updated: summary_stats.last_updated,
    };
    
    let response = AllResultsResponse {
        results,
        pagination,
        summary,
    };
    
    Ok(Json(SuccessResponse::new(response)))
}

/// Get detailed P&L result for a specific wallet-token pair
pub async fn get_detailed_result(
    State(state): State<AppState>,
    Path((wallet_address, token_address)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let redis_client = persistence_layer::RedisClient::new(&state.config.redis.url).await
        .map_err(|e| ApiError::Internal(format!("Redis connection error: {}", e)))?;
    
    let stored_result = redis_client.get_pnl_result(&wallet_address, &token_address).await
        .map_err(|e| ApiError::Internal(format!("Failed to fetch result: {}", e)))?;
    
    match stored_result {
        Some(result) => {
            let response = DetailedPnLResultResponse {
                wallet_address: result.wallet_address,
                token_address: result.token_address,
                token_symbol: result.token_symbol,
                pnl_report: result.pnl_report,
                analyzed_at: result.analyzed_at,
            };
            Ok(Json(SuccessResponse::new(response)))
        }
        None => Err(ApiError::NotFound(format!(
            "No P&L result found for wallet {} token {}",
            wallet_address, token_address
        ))),
    }
}

/// Enhanced health check with component status
pub async fn enhanced_health_check(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let start_time = std::time::Instant::now();
    
    // Test Redis connectivity
    let redis_client_result = persistence_layer::RedisClient::new(&state.config.redis.url).await;
    let redis_health = match redis_client_result {
        Ok(redis_client) => {
            match redis_client.health_check().await {
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
            }
        }
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
            state.service_manager
                .start_wallet_discovery_with_config(request.config_override)
                .await
                .map_err(|e| ApiError::ServiceManager(e.to_string()))?;
            MessageResponse {
                message: "Wallet discovery service started successfully".to_string(),
            }
        }
        ("stop", "wallet_discovery") => {
            state.service_manager.stop_wallet_discovery().await
                .map_err(|e| ApiError::ServiceManager(e.to_string()))?;
            MessageResponse {
                message: "Wallet discovery service stopped successfully".to_string(),
            }
        }
        ("start", "pnl_analysis") => {
            state.service_manager.start_pnl_analysis().await
                .map_err(|e| ApiError::ServiceManager(e.to_string()))?;
            MessageResponse {
                message: "P&L analysis service started successfully".to_string(),
            }
        }
        ("stop", "pnl_analysis") => {
            state.service_manager.stop_pnl_analysis().await
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
                    state.service_manager
                        .start_wallet_discovery_with_config(request.config_override)
                        .await
                        .map_err(|e| ApiError::ServiceManager(e.to_string()))?;
                }
                "pnl_analysis" => {
                    let _ = state.service_manager.stop_pnl_analysis().await;
                    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                    state.service_manager.start_pnl_analysis().await
                        .map_err(|e| ApiError::ServiceManager(e.to_string()))?;
                }
                _ => return Err(ApiError::Validation(format!("Unknown service: {}", service)))
            }
            MessageResponse {
                message: format!("{} service restarted successfully", service),
            }
        }
        _ => return Err(ApiError::Validation(format!("Invalid action '{}' for service '{}'", request.action, request.service)))
    };

    Ok(Json(SuccessResponse::new(response)))
}