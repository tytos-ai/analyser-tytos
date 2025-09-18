//! API v2 Types - Enhanced Data Structures for Copy Trading Analysis

use chrono::{DateTime, Utc};
use pnl_core::{MatchedTrade, PortfolioPnLResult, RemainingPosition, UnmatchedSell};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Enhanced wallet analysis response with full P&L engine data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletAnalysisV2 {
    /// Wallet address analyzed
    pub wallet_address: String,

    /// Direct portfolio P&L result from new engine
    pub portfolio_result: PortfolioPnLResult,

    /// Enhanced copy trading metrics
    pub copy_trading_metrics: CopyTradingMetrics,

    /// Analysis metadata
    pub metadata: AnalysisMetadata,
}

/// Copy trading specific metrics for strategy evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyTradingMetrics {
    /// Trading style classification
    pub trading_style: TradingStyle,

    /// Performance consistency metrics
    pub consistency_score: Decimal,

    /// Risk management assessment
    pub risk_metrics: RiskMetrics,

    /// Position management patterns
    pub position_patterns: PositionPatterns,

    /// Profitability distribution
    pub profit_distribution: ProfitDistribution,
}

/// Trading style classification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TradingStyle {
    Scalper {
        avg_hold_minutes: Decimal,
    },
    SwingTrader {
        avg_hold_hours: Decimal,
    },
    LongTerm {
        avg_hold_days: Decimal,
    },
    Mixed {
        predominant_style: Box<TradingStyle>,
    },
}

/// Risk management metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskMetrics {
    /// Maximum position size as % of portfolio
    pub max_position_percentage: Decimal,

    /// Portfolio diversification score (0-100)
    pub diversification_score: Decimal,

    /// Maximum consecutive losses
    pub max_consecutive_losses: u32,

    /// Average loss per losing trade
    pub avg_loss_per_trade: Decimal,

    /// Win streak statistics
    pub max_win_streak: u32,

    /// Risk-adjusted return (Sharpe-like ratio)
    pub risk_adjusted_return: Decimal,
}

/// Position management patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionPatterns {
    /// Average position hold time
    pub avg_hold_time_minutes: Decimal,

    /// Position sizing consistency (std dev of position sizes)
    pub position_size_consistency: Decimal,

    /// Tendency to hold winners vs losers
    pub winner_hold_ratio: Decimal,

    /// Partial exit frequency
    pub partial_exit_frequency: Decimal,

    /// DCA (Dollar Cost Averaging) frequency
    pub dca_frequency: Decimal,
}

/// Profitability distribution analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfitDistribution {
    /// Percentage of trades that are highly profitable (>50% gain)
    pub high_profit_trades_pct: Decimal,

    /// Percentage of break-even trades
    pub breakeven_trades_pct: Decimal,

    /// Average profit of winning trades
    pub avg_winning_trade_pct: Decimal,

    /// Average loss of losing trades  
    pub avg_losing_trade_pct: Decimal,

    /// Profit factor (total wins / total losses)
    pub profit_factor: Decimal,
}

/// Enhanced analysis metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisMetadata {
    /// Analysis timestamp
    pub analyzed_at: DateTime<Utc>,

    /// Data source architecture used
    pub data_source: String,

    /// Number of tokens analyzed
    pub tokens_processed: u32,

    /// Total events processed
    pub events_processed: u32,

    /// Analysis duration in milliseconds
    pub analysis_duration_ms: u64,

    /// Algorithm version
    pub algorithm_version: String,

    /// Quality score of the analysis (0-100)
    pub quality_score: Decimal,
}

/// Individual trade details for copy trading analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeDetailsV2 {
    /// Matched trades with full FIFO details
    pub matched_trades: Vec<EnhancedMatchedTrade>,

    /// Unmatched sells (phantom trades)
    pub unmatched_sells: Vec<EnhancedUnmatchedSell>,

    /// Trade statistics
    pub statistics: TradeStatistics,
}

/// Enhanced matched trade with additional copy trading context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedMatchedTrade {
    /// Original matched trade data
    #[serde(flatten)]
    pub trade: MatchedTrade,

    /// Trade performance category
    pub performance_category: TradePerformance,

    /// Hold time category
    pub hold_time_category: HoldTimeCategory,

    /// Position size relative to portfolio
    pub position_size_percentage: Decimal,

    /// Entry/exit timing quality
    pub timing_score: Decimal,
}

/// Enhanced unmatched sell with context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedUnmatchedSell {
    /// Original unmatched sell data
    #[serde(flatten)]
    pub sell: UnmatchedSell,

    /// Likely reason for unmatched sell
    pub likely_reason: UnmatchedReason,

    /// Impact on portfolio
    pub portfolio_impact: Decimal,
}

/// Trade performance categories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TradePerformance {
    HighlyProfitable, // >50% gain
    Profitable,       // 10-50% gain
    ModerateGain,     // 0-10% gain
    BreakEven,        // -5% to 5%
    ModereLoss,       // -5% to -20%
    SignificantLoss,  // -20% to -50%
    MajorLoss,        // <-50%
}

/// Hold time categories for trading style analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HoldTimeCategory {
    Scalp,      // <1 hour
    Intraday,   // 1-24 hours
    ShortTerm,  // 1-7 days
    MediumTerm, // 1-4 weeks
    LongTerm,   // >1 month
}

/// Reasons for unmatched sells
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UnmatchedReason {
    PreExistingPosition, // Token held before analysis period
    Airdrop,             // Free tokens received
    Transfer,            // Tokens transferred in
    DataGap,             // Missing transaction data
    Other,
}

/// Trade statistics for copy trading evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeStatistics {
    /// Total number of completed trades
    pub total_trades: u32,

    /// Win rate percentage
    pub win_rate: Decimal,

    /// Average trade duration in minutes
    pub avg_trade_duration_minutes: Decimal,

    /// Best performing trade P&L
    pub best_trade_pnl: Decimal,

    /// Worst performing trade P&L
    pub worst_trade_pnl: Decimal,

    /// Consistency metrics
    pub consistency_metrics: ConsistencyMetrics,
}

/// Trading consistency metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsistencyMetrics {
    /// Standard deviation of trade returns
    pub return_volatility: Decimal,

    /// Percentage of trades within 1 std dev of mean
    pub trades_within_1_stddev: Decimal,

    /// Longest winning streak
    pub longest_win_streak: u32,

    /// Longest losing streak
    pub longest_lose_streak: u32,

    /// Average time between trades in hours
    pub avg_time_between_trades_hours: Decimal,
}

/// Current positions with enhanced tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionsV2 {
    /// All current positions
    pub positions: Vec<EnhancedPosition>,

    /// Portfolio allocation
    pub allocation: PortfolioAllocation,

    /// Position management metrics
    pub management_metrics: PositionManagementMetrics,
}

/// Enhanced position information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedPosition {
    /// Original position data
    #[serde(flatten)]
    pub position: RemainingPosition,

    /// Current market value
    pub current_value_usd: Decimal,

    /// Unrealized P&L
    pub unrealized_pnl_usd: Decimal,

    /// Unrealized P&L percentage
    pub unrealized_pnl_percentage: Decimal,

    /// Days held
    pub days_held: u32,

    /// Position size as percentage of portfolio
    pub portfolio_percentage: Decimal,

    /// Risk level of this position
    pub risk_level: PositionRisk,
}

/// Portfolio allocation breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioAllocation {
    /// Number of positions
    pub position_count: u32,

    /// Largest position percentage
    pub largest_position_pct: Decimal,

    /// Smallest position percentage
    pub smallest_position_pct: Decimal,

    /// Average position size percentage
    pub avg_position_pct: Decimal,

    /// Portfolio concentration score
    pub concentration_score: Decimal,
}

/// Position management quality metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionManagementMetrics {
    /// Average position hold time
    pub avg_hold_time_days: Decimal,

    /// Position sizing consistency
    pub sizing_consistency_score: Decimal,

    /// Diversification quality
    pub diversification_score: Decimal,

    /// Risk management score
    pub risk_management_score: Decimal,
}

/// Position risk classification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PositionRisk {
    Low,      // <5% of portfolio
    Medium,   // 5-15% of portfolio
    High,     // 15-30% of portfolio
    VeryHigh, // >30% of portfolio
}

/// Batch analysis v2 request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchAnalysisV2Request {
    /// Wallet addresses to analyze
    pub wallet_addresses: Vec<String>,

    /// Analysis filters
    pub filters: Option<AnalysisFilters>,

    /// Include copy trading metrics
    pub include_copy_trading_metrics: Option<bool>,

    /// Include detailed trade breakdown
    pub include_trade_details: Option<bool>,
}

/// Enhanced analysis filters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisFilters {
    /// Minimum portfolio value in USD
    pub min_portfolio_value_usd: Option<Decimal>,

    /// Minimum number of trades
    pub min_trades: Option<u32>,

    /// Minimum win rate percentage
    pub min_win_rate: Option<Decimal>,

    /// Maximum analysis timeframe
    pub timeframe_days: Option<u32>,

    /// Include only active traders
    pub active_traders_only: Option<bool>,

    /// Minimum trade frequency (trades per week)
    pub min_trade_frequency: Option<Decimal>,
}

/// Batch analysis v2 response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchAnalysisV2Response {
    /// Job identifier
    pub job_id: Uuid,

    /// Analysis results
    pub results: HashMap<String, WalletAnalysisV2>,

    /// Batch statistics
    pub batch_statistics: BatchStatistics,

    /// Ranking of wallets by copy trading potential
    pub copy_trading_ranking: Vec<WalletRanking>,
}

/// Batch analysis statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchStatistics {
    /// Total wallets analyzed
    pub total_wallets: u32,

    /// Successful analyses
    pub successful_analyses: u32,

    /// Failed analyses
    pub failed_analyses: u32,

    /// Average analysis time per wallet
    pub avg_analysis_time_ms: u64,

    /// Top performer
    pub top_performer: Option<String>,

    /// Average portfolio value
    pub avg_portfolio_value_usd: Decimal,
}

/// Wallet ranking for copy trading potential
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletRanking {
    /// Wallet address
    pub wallet_address: String,

    /// Overall copy trading score (0-100)
    pub copy_trading_score: Decimal,

    /// Ranking position
    pub rank: u32,

    /// Key strengths
    pub strengths: Vec<String>,

    /// Risk factors
    pub risk_factors: Vec<String>,

    /// Recommended copy allocation percentage
    pub recommended_allocation_pct: Decimal,
}
