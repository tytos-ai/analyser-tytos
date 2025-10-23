//! API v2 Handlers - Enhanced P&L Analysis Endpoints

use axum::{
    extract::{Path, Query, State},
    response::Json,
};
use chrono::Utc;
use pnl_core::{NewPnLEngine, PortfolioPnLResult, ZerionBalanceFetcher};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use std::collections::HashMap;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::types::{validate_chain, SuccessResponse};
use crate::v2::types::*;
use crate::{ApiError, AppState};

/// Get comprehensive wallet analysis with full P&L engine data
pub async fn get_wallet_analysis_v2(
    State(state): State<AppState>,
    Path(wallet_address): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<SuccessResponse<WalletAnalysisV2>>, ApiError> {
    info!("Starting v2 wallet analysis for: {}", wallet_address);

    let start_time = std::time::Instant::now();

    // Validate wallet address
    if wallet_address.len() < 32 || wallet_address.len() > 44 {
        return Err(ApiError::BadRequest(
            "Invalid wallet address format".to_string(),
        ));
    }

    // Parse optional parameters
    let include_copy_metrics = params
        .get("include_copy_metrics")
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(true);

    let max_transactions = params
        .get("max_transactions")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(500);

    // Get chain parameter or use default
    let chain = params
        .get("chain")
        .map(|s| s.as_str())
        .unwrap_or(&state.config.multichain.default_chain);

    // Validate chain is enabled
    validate_chain(chain, &state.config.multichain.enabled_chains)
        .map_err(|e| ApiError::BadRequest(e))?;

    // Fetch transaction data using Zerion data source
    let financial_events = {
        debug!(
            "Using Zerion data source for wallet analysis on chain: {}",
            chain
        );

        let zerion_client = state.orchestrator.get_zerion_client()
            .ok_or_else(|| ApiError::InternalServerError("Zerion client not initialized".to_string()))?;
        let zerion_transactions = zerion_client
            .get_wallet_transactions_with_time_range(
                &wallet_address,
                "usd",
                None, // No time range filtering for V2 API
                Some(max_transactions as usize),
                Some(chain),
            )
            .await
            .map_err(|e| ApiError::InternalServerError(format!("Zerion error: {}", e)))?;

        debug!(
            "Fetched {} Zerion transactions for wallet {}",
            zerion_transactions.len(),
            wallet_address
        );

        // Convert Zerion transactions directly to financial events
        zerion_client.convert_to_financial_events(&zerion_transactions, &wallet_address)
    };

    debug!(
        "Converted {} financial events for wallet {}",
        financial_events.events.len(),
        wallet_address
    );

    // Group events by token
    let events_by_token = pnl_core::NewTransactionParser::group_events_by_token(financial_events.events);

    let total_events: usize = events_by_token.values().map(|events| events.len()).sum();
    debug!(
        "Grouped {} events across {} tokens",
        total_events,
        events_by_token.len()
    );

    // Get current prices for unrealized P&L calculation using Zerion portfolio
    let current_prices = {
        debug!("Fetching current portfolio from Zerion for current prices on chain: {}", chain);

        // Create Zerion balance fetcher
        let balance_fetcher = ZerionBalanceFetcher::new(
            state.config.zerion.api_key.clone(),
            Some("https://api.zerion.io".to_string()),
        );

        match balance_fetcher.fetch_wallet_balances_for_chain(&wallet_address, chain).await {
            Ok(zerion_balances) => {
                // Convert Zerion balances to portfolio format using orchestrator
                let portfolio = state.orchestrator.convert_zerion_to_wallet_token_balance(&zerion_balances, chain);
                debug!("✅ Successfully fetched {} tokens from Zerion portfolio for prices", portfolio.len());

                // Extract current prices from portfolio
                Some(dex_client::extract_current_prices_from_portfolio(&portfolio))
            }
            Err(e) => {
                warn!("Failed to fetch current prices from Zerion portfolio: {}", e);
                None
            }
        }
    };

    // Calculate P&L using new engine with Zerion balance API integration (direct PortfolioPnLResult)
    let balance_fetcher = ZerionBalanceFetcher::new(
        state.config.zerion.api_key.clone(),
        Some("https://api.zerion.io".to_string()),
    );

    let pnl_engine = if true {
        // Always enabled for hybrid mode
        NewPnLEngine::with_balance_fetcher(wallet_address.clone(), balance_fetcher)
    } else {
        NewPnLEngine::new(wallet_address.clone())
    };
    let portfolio_result = pnl_engine
        .calculate_portfolio_pnl(events_by_token, current_prices)
        .map_err(|e| ApiError::InternalServerError(format!("P&L calculation error: {}", e)))?;

    // Calculate copy trading metrics if requested
    let copy_trading_metrics = if include_copy_metrics {
        calculate_copy_trading_metrics(&portfolio_result)
    } else {
        CopyTradingMetrics {
            trading_style: TradingStyle::Mixed {
                predominant_style: Box::new(TradingStyle::LongTerm {
                    avg_hold_days: Decimal::ZERO,
                }),
            },
            consistency_score: Decimal::ZERO,
            risk_metrics: RiskMetrics {
                max_position_percentage: Decimal::ZERO,
                diversification_score: Decimal::ZERO,
                max_consecutive_losses: 0,
                avg_loss_per_trade: Decimal::ZERO,
                max_win_streak: 0,
                risk_adjusted_return: Decimal::ZERO,
            },
            position_patterns: PositionPatterns {
                avg_hold_time_minutes: Decimal::ZERO,
                position_size_consistency: Decimal::ZERO,
                winner_hold_ratio: Decimal::ZERO,
                partial_exit_frequency: Decimal::ZERO,
                dca_frequency: Decimal::ZERO,
            },
            profit_distribution: ProfitDistribution {
                high_profit_trades_pct: Decimal::ZERO,
                breakeven_trades_pct: Decimal::ZERO,
                avg_winning_trade_pct: Decimal::ZERO,
                avg_losing_trade_pct: Decimal::ZERO,
                profit_factor: Decimal::ZERO,
            },
        }
    };

    let analysis_duration = start_time.elapsed();

    let metadata = AnalysisMetadata {
        analyzed_at: Utc::now(),
        data_source: "Zerion".to_string(),
        tokens_processed: portfolio_result.tokens_analyzed,
        events_processed: portfolio_result.events_processed,
        analysis_duration_ms: analysis_duration.as_millis() as u64,
        algorithm_version: "new_pnl_engine_v1.0".to_string(),
        quality_score: calculate_quality_score(&portfolio_result),
    };

    let analysis = WalletAnalysisV2 {
        wallet_address,
        portfolio_result,
        copy_trading_metrics,
        metadata,
    };

    info!(
        "Completed v2 wallet analysis in {}ms",
        analysis_duration.as_millis()
    );

    Ok(Json(SuccessResponse::new(analysis)))
}

/// Get individual trade details for copy trading analysis
pub async fn get_wallet_trades_v2(
    State(state): State<AppState>,
    Path(wallet_address): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<SuccessResponse<TradeDetailsV2>>, ApiError> {
    info!("Getting trade details v2 for wallet: {}", wallet_address);

    // First get the wallet analysis to extract trade data
    let analysis_result =
        get_wallet_analysis_v2(State(state), Path(wallet_address), Query(params)).await?;
    let portfolio_result = &analysis_result.0.data.portfolio_result;

    let mut matched_trades = Vec::new();
    let unmatched_sells = Vec::new();

    // Extract and enhance all trades from all tokens
    for token_result in &portfolio_result.token_results {
        for trade in &token_result.matched_trades {
            let enhanced_trade = EnhancedMatchedTrade {
                trade: trade.clone(),
                performance_category: classify_trade_performance(&trade.realized_pnl_usd),
                hold_time_category: classify_hold_time(trade.hold_time_seconds),
                position_size_percentage: calculate_position_size_percentage(
                    &trade,
                    &portfolio_result,
                ),
                timing_score: calculate_timing_score(&trade),
            };
            matched_trades.push(enhanced_trade);
        }

        // Note: No unmatched sells anymore - all sells are matched against phantom buys if needed
    }

    let statistics = calculate_trade_statistics(&matched_trades, &unmatched_sells);

    let trade_details = TradeDetailsV2 {
        matched_trades,
        unmatched_sells,
        statistics,
    };

    Ok(Json(SuccessResponse::new(trade_details)))
}

/// Get current positions with enhanced tracking
pub async fn get_wallet_positions_v2(
    State(state): State<AppState>,
    Path(wallet_address): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<SuccessResponse<PositionsV2>>, ApiError> {
    info!("Getting positions v2 for wallet: {}", wallet_address);

    // Get wallet analysis to extract position data
    let analysis_result =
        get_wallet_analysis_v2(State(state), Path(wallet_address), Query(params)).await?;
    let portfolio_result = &analysis_result.0.data.portfolio_result;

    let mut enhanced_positions = Vec::new();
    let mut total_portfolio_value = Decimal::ZERO;

    // Extract and enhance all positions from all tokens
    for token_result in &portfolio_result.token_results {
        if let Some(ref position) = token_result.remaining_position {
            // Calculate current market value (would need current prices)
            let current_value =
                position.total_cost_basis_usd + token_result.total_unrealized_pnl_usd;
            total_portfolio_value += current_value;

            let enhanced_position = EnhancedPosition {
                position: position.clone(),
                current_value_usd: current_value,
                unrealized_pnl_usd: token_result.total_unrealized_pnl_usd,
                unrealized_pnl_percentage: if position.total_cost_basis_usd > Decimal::ZERO {
                    (token_result.total_unrealized_pnl_usd / position.total_cost_basis_usd)
                        * Decimal::from(100)
                } else {
                    Decimal::ZERO
                },
                days_held: 30, // TODO: Calculate actual days held from first purchase
                portfolio_percentage: Decimal::ZERO, // Will be calculated after we know total value
                risk_level: PositionRisk::Low, // Will be calculated based on percentage
            };
            enhanced_positions.push(enhanced_position);
        }
    }

    // Calculate portfolio percentages and risk levels
    for position in &mut enhanced_positions {
        if total_portfolio_value > Decimal::ZERO {
            position.portfolio_percentage =
                (position.current_value_usd / total_portfolio_value) * Decimal::from(100);
            position.risk_level = classify_position_risk(position.portfolio_percentage);
        }
    }

    let allocation = calculate_portfolio_allocation(&enhanced_positions);
    let management_metrics =
        calculate_position_management_metrics(&enhanced_positions, &portfolio_result);

    let positions = PositionsV2 {
        positions: enhanced_positions,
        allocation,
        management_metrics,
    };

    Ok(Json(SuccessResponse::new(positions)))
}

/// Submit enhanced batch analysis v2
pub async fn submit_batch_analysis_v2(
    State(_state): State<AppState>,
    Json(request): Json<BatchAnalysisV2Request>,
) -> Result<Json<SuccessResponse<Value>>, ApiError> {
    info!(
        "Submitting batch analysis v2 for {} wallets",
        request.wallet_addresses.len()
    );

    if request.wallet_addresses.is_empty() {
        return Err(ApiError::BadRequest(
            "No wallet addresses provided".to_string(),
        ));
    }

    if request.wallet_addresses.len() > 100 {
        return Err(ApiError::BadRequest(
            "Maximum 100 wallets per batch".to_string(),
        ));
    }

    let job_id = Uuid::new_v4();

    // TODO: Implement actual batch processing with enhanced features
    // For now, return a job ID that can be used to retrieve results

    Ok(Json(SuccessResponse::new(json!({
        "job_id": job_id,
        "status": "pending",
        "wallet_count": request.wallet_addresses.len(),
        "estimated_completion_minutes": request.wallet_addresses.len() / 10 + 1,
        "features": {
            "copy_trading_metrics": request.include_copy_trading_metrics.unwrap_or(true),
            "trade_details": request.include_trade_details.unwrap_or(true)
        }
    }))))
}

// Helper functions for metric calculations

fn calculate_copy_trading_metrics(portfolio_result: &PortfolioPnLResult) -> CopyTradingMetrics {
    // Analyze trading style based on average hold times
    let trading_style = if portfolio_result.avg_hold_time_minutes < Decimal::from(60) {
        TradingStyle::Scalper {
            avg_hold_minutes: portfolio_result.avg_hold_time_minutes,
        }
    } else if portfolio_result.avg_hold_time_minutes < Decimal::from(24 * 60) {
        TradingStyle::SwingTrader {
            avg_hold_hours: portfolio_result.avg_hold_time_minutes / Decimal::from(60),
        }
    } else {
        TradingStyle::LongTerm {
            avg_hold_days: portfolio_result.avg_hold_time_minutes / Decimal::from(24 * 60),
        }
    };

    // Calculate risk metrics (simplified)
    let risk_metrics = RiskMetrics {
        max_position_percentage: Decimal::from(25), // TODO: Calculate from actual positions
        diversification_score: Decimal::from(portfolio_result.tokens_analyzed * 10)
            .min(Decimal::from(100)),
        max_consecutive_losses: 0, // TODO: Calculate from trade sequence
        avg_loss_per_trade: Decimal::ZERO, // TODO: Calculate from losing trades
        max_win_streak: 0,         // TODO: Calculate from trade sequence
        risk_adjusted_return: Decimal::ZERO, // TODO: Calculate Sharpe-like ratio
    };

    // Position patterns (simplified)
    let position_patterns = PositionPatterns {
        avg_hold_time_minutes: portfolio_result.avg_hold_time_minutes,
        position_size_consistency: Decimal::ZERO, // TODO: Calculate from position sizes
        winner_hold_ratio: Decimal::ZERO,         // TODO: Calculate hold time ratio
        partial_exit_frequency: rust_decimal_macros::dec!(0.1), // TODO: Calculate from partial sales
        dca_frequency: Decimal::ZERO,                           // TODO: Calculate DCA patterns
    };

    // Profit distribution (simplified)
    let profit_distribution = ProfitDistribution {
        high_profit_trades_pct: Decimal::ZERO, // TODO: Calculate high profit percentage
        breakeven_trades_pct: Decimal::from(10),
        avg_winning_trade_pct: Decimal::from(15),
        avg_losing_trade_pct: Decimal::from(-8),
        profit_factor: if portfolio_result.total_trades > 0 {
            portfolio_result.overall_win_rate_percentage / Decimal::from(100)
        } else {
            Decimal::ZERO
        },
    };

    CopyTradingMetrics {
        trading_style,
        consistency_score: Decimal::ZERO, // TODO: Calculate consistency score
        risk_metrics,
        position_patterns,
        profit_distribution,
    }
}

fn calculate_quality_score(portfolio_result: &PortfolioPnLResult) -> Decimal {
    let mut score = Decimal::from(50); // Base score

    // Bonus for more trades
    if portfolio_result.total_trades >= 10 {
        score += Decimal::from(20);
    } else if portfolio_result.total_trades >= 5 {
        score += Decimal::from(10);
    }

    // Bonus for profitability
    if portfolio_result.total_pnl_usd > Decimal::ZERO {
        score += Decimal::from(15);
    }

    // Bonus for good win rate
    if portfolio_result.overall_win_rate_percentage > Decimal::from(60) {
        score += Decimal::from(15);
    } else if portfolio_result.overall_win_rate_percentage > Decimal::from(40) {
        score += Decimal::from(10);
    }

    score.min(Decimal::from(100)).max(Decimal::from(0))
}

fn classify_trade_performance(pnl: &Decimal) -> TradePerformance {
    let pnl_pct = *pnl; // Assuming this is already in percentage terms

    if pnl_pct > Decimal::from(50) {
        TradePerformance::HighlyProfitable
    } else if pnl_pct > Decimal::from(10) {
        TradePerformance::Profitable
    } else if pnl_pct > Decimal::ZERO {
        TradePerformance::ModerateGain
    } else if pnl_pct > Decimal::from(-5) {
        TradePerformance::BreakEven
    } else if pnl_pct > Decimal::from(-20) {
        TradePerformance::ModereLoss
    } else if pnl_pct > Decimal::from(-50) {
        TradePerformance::SignificantLoss
    } else {
        TradePerformance::MajorLoss
    }
}

fn classify_hold_time(hold_time_seconds: i64) -> HoldTimeCategory {
    let minutes = hold_time_seconds / 60;

    if minutes < 60 {
        HoldTimeCategory::Scalp
    } else if minutes < 24 * 60 {
        HoldTimeCategory::Intraday
    } else if minutes < 7 * 24 * 60 {
        HoldTimeCategory::ShortTerm
    } else if minutes < 30 * 24 * 60 {
        HoldTimeCategory::MediumTerm
    } else {
        HoldTimeCategory::LongTerm
    }
}

fn classify_position_risk(percentage: Decimal) -> PositionRisk {
    if percentage < Decimal::from(5) {
        PositionRisk::Low
    } else if percentage < Decimal::from(15) {
        PositionRisk::Medium
    } else if percentage < Decimal::from(30) {
        PositionRisk::High
    } else {
        PositionRisk::VeryHigh
    }
}

// Additional helper functions would be implemented here...
// These are simplified stubs for compilation

fn calculate_position_size_percentage(
    _trade: &pnl_core::MatchedTrade,
    _portfolio: &PortfolioPnLResult,
) -> Decimal {
    Decimal::from(5) // Simplified
}

fn calculate_timing_score(_trade: &pnl_core::MatchedTrade) -> Decimal {
    Decimal::from(75) // Simplified
}

fn calculate_trade_statistics(
    _matched_trades: &[EnhancedMatchedTrade],
    _unmatched_sells: &[EnhancedUnmatchedSell],
) -> TradeStatistics {
    // Simplified implementation
    TradeStatistics {
        total_trades: 0,
        win_rate: Decimal::ZERO,
        avg_trade_duration_minutes: Decimal::ZERO,
        best_trade_pnl: Decimal::ZERO,
        worst_trade_pnl: Decimal::ZERO,
        consistency_metrics: ConsistencyMetrics {
            return_volatility: Decimal::ZERO,
            trades_within_1_stddev: Decimal::ZERO,
            longest_win_streak: 0,
            longest_lose_streak: 0,
            avg_time_between_trades_hours: Decimal::ZERO,
        },
    }
}

fn calculate_portfolio_allocation(positions: &[EnhancedPosition]) -> PortfolioAllocation {
    let position_count = positions.len() as u32;

    if position_count == 0 {
        return PortfolioAllocation {
            position_count: 0,
            largest_position_pct: Decimal::ZERO,
            smallest_position_pct: Decimal::ZERO,
            avg_position_pct: Decimal::ZERO,
            concentration_score: Decimal::ZERO,
        };
    }

    let percentages: Vec<Decimal> = positions.iter().map(|p| p.portfolio_percentage).collect();

    let largest = percentages.iter().copied().max().unwrap_or(Decimal::ZERO);
    let smallest = percentages.iter().copied().min().unwrap_or(Decimal::ZERO);
    let sum: Decimal = percentages.iter().sum();
    let avg = sum / Decimal::from(position_count);

    // Concentration score based on largest position
    let concentration_score = if largest > Decimal::from(30) {
        Decimal::from(20) // High concentration
    } else if largest > Decimal::from(15) {
        Decimal::from(50) // Medium concentration
    } else {
        Decimal::from(80) // Well diversified
    };

    PortfolioAllocation {
        position_count,
        largest_position_pct: largest,
        smallest_position_pct: smallest,
        avg_position_pct: avg,
        concentration_score,
    }
}

fn calculate_position_management_metrics(
    _positions: &[EnhancedPosition],
    portfolio: &PortfolioPnLResult,
) -> PositionManagementMetrics {
    // Simplified implementation based on available portfolio data
    PositionManagementMetrics {
        avg_hold_time_days: portfolio.avg_hold_time_minutes / Decimal::from(24 * 60),
        sizing_consistency_score: Decimal::from(75), // TODO: Calculate actual consistency
        diversification_score: Decimal::from(portfolio.tokens_analyzed * 15)
            .min(Decimal::from(100)),
        risk_management_score: if portfolio.overall_win_rate_percentage > Decimal::from(50) {
            Decimal::from(80)
        } else {
            Decimal::from(60)
        },
    }
}
