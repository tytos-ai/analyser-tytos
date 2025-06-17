use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tracing::{debug, info, warn};

use crate::{PnLReport, Result};

/// Trader quality scoring and filtering for copy trading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraderFilter {
    pub min_realized_pnl_usd: Decimal,
    pub min_total_trades: u32,
    pub min_winning_trades: u32,
    pub min_win_rate: Decimal,
    pub min_roi_percentage: Decimal,
    pub min_capital_deployed_sol: Decimal,
    pub max_avg_hold_time_minutes: Decimal,
    pub min_avg_hold_time_minutes: Decimal,
    pub exclude_holders_only: bool,
    pub exclude_zero_pnl: bool,
    pub min_transaction_frequency: Decimal,
}

/// Quality assessment of a trader for copy trading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraderQuality {
    pub wallet_address: String,
    pub is_qualified: bool,
    pub score: f64,
    pub risk_level: RiskLevel,
    pub trading_style: TradingStyle,
    pub strengths: Vec<String>,
    pub concerns: Vec<String>,
    pub copy_trade_recommended: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    VeryHigh,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TradingStyle {
    Scalper,      // Very short holds, high frequency
    DayTrader,    // Intraday trading
    SwingTrader,  // Multi-day holds
    Holder,       // Long-term positions
    Unknown,
}

impl TraderFilter {
    pub fn new(config: &config_manager::TraderFilterConfig) -> Self {
        Self {
            min_realized_pnl_usd: Decimal::from_str(&config.min_realized_pnl_usd.to_string()).unwrap_or(Decimal::ZERO),
            min_total_trades: config.min_total_trades,
            min_winning_trades: config.min_winning_trades,
            min_win_rate: Decimal::from_str(&config.min_win_rate.to_string()).unwrap_or(Decimal::ZERO),
            min_roi_percentage: Decimal::from_str(&config.min_roi_percentage.to_string()).unwrap_or(Decimal::ZERO),
            min_capital_deployed_sol: Decimal::from_str(&config.min_capital_deployed_sol.to_string()).unwrap_or(Decimal::ZERO),
            max_avg_hold_time_minutes: Decimal::from_str(&config.max_avg_hold_time_minutes.to_string()).unwrap_or(Decimal::from(1440)),
            min_avg_hold_time_minutes: Decimal::from_str(&config.min_avg_hold_time_minutes.to_string()).unwrap_or(Decimal::ONE),
            exclude_holders_only: config.exclude_holders_only,
            exclude_zero_pnl: config.exclude_zero_pnl,
            min_transaction_frequency: Decimal::from_str(&config.min_transaction_frequency.to_string()).unwrap_or(Decimal::ZERO),
        }
    }

    /// Evaluate if a trader qualifies for copy trading based on P&L report
    pub fn evaluate_trader(&self, report: &PnLReport) -> Result<TraderQuality> {
        debug!("🔍 Evaluating trader quality for wallet: {}", report.wallet_address);
        
        let mut quality = TraderQuality {
            wallet_address: report.wallet_address.clone(),
            is_qualified: false,
            score: 0.0,
            risk_level: RiskLevel::Medium,
            trading_style: TradingStyle::Unknown,
            strengths: Vec::new(),
            concerns: Vec::new(),
            copy_trade_recommended: false,
        };

        // Get metrics from report (already Decimals)
        let realized_pnl = report.summary.realized_pnl_usd;
        let total_pnl = report.summary.total_pnl_usd;
        let win_rate = report.summary.win_rate;
        let roi = report.summary.roi_percentage;
        let capital_deployed = report.summary.total_capital_deployed_sol;
        let avg_hold_time = report.summary.avg_hold_time_minutes;

        // 1. Basic qualification checks
        let mut passes_basic_checks = true;
        let mut qualification_score = 0.0;

        // Check realized P&L
        if realized_pnl < self.min_realized_pnl_usd {
            quality.concerns.push(format!("Low realized P&L: ${:.4} < ${:.4}", 
                realized_pnl, self.min_realized_pnl_usd));
            passes_basic_checks = false;
        } else {
            quality.strengths.push(format!("Good realized P&L: ${:.4}", realized_pnl));
            qualification_score += 20.0;
        }

        // Check total trades
        if report.summary.total_trades < self.min_total_trades {
            quality.concerns.push(format!("Insufficient trades: {} < {}", 
                report.summary.total_trades, self.min_total_trades));
            passes_basic_checks = false;
        } else {
            quality.strengths.push(format!("Active trader: {} total trades", report.summary.total_trades));
            qualification_score += 15.0;
        }

        // Check winning trades
        if report.summary.winning_trades < self.min_winning_trades {
            quality.concerns.push(format!("Few winning trades: {} < {}", 
                report.summary.winning_trades, self.min_winning_trades));
            passes_basic_checks = false;
        } else {
            quality.strengths.push(format!("Consistent wins: {} winning trades", report.summary.winning_trades));
            qualification_score += 15.0;
        }

        // Check win rate
        if win_rate < self.min_win_rate {
            quality.concerns.push(format!("Low win rate: {:.1}% < {:.1}%", 
                win_rate, self.min_win_rate));
            passes_basic_checks = false;
        } else {
            quality.strengths.push(format!("Good win rate: {:.1}%", win_rate));
            qualification_score += 20.0;
        }

        // Check ROI
        if roi < self.min_roi_percentage {
            quality.concerns.push(format!("Low ROI: {:.1}% < {:.1}%", 
                roi, self.min_roi_percentage));
        } else {
            quality.strengths.push(format!("Strong ROI: {:.1}%", roi));
            qualification_score += 15.0;
        }

        // Check capital deployment
        if capital_deployed < self.min_capital_deployed_sol {
            quality.concerns.push(format!("Low capital: {:.4} SOL < {:.4} SOL", 
                capital_deployed, self.min_capital_deployed_sol));
        } else {
            quality.strengths.push(format!("Adequate capital: {:.4} SOL", capital_deployed));
            qualification_score += 10.0;
        }

        // Check holders-only exclusion
        if self.exclude_holders_only && report.summary.losing_trades == 0 && report.summary.winning_trades == 0 {
            quality.concerns.push("No realized trades - appears to be holder only".to_string());
            passes_basic_checks = false;
        }

        // Check zero P&L exclusion  
        if self.exclude_zero_pnl && realized_pnl == Decimal::ZERO && total_pnl == Decimal::ZERO {
            quality.concerns.push("Zero P&L detected".to_string());
            passes_basic_checks = false;
        }

        // 2. Determine trading style based on hold times
        quality.trading_style = if avg_hold_time < Decimal::from(60) {
            TradingStyle::Scalper
        } else if avg_hold_time < Decimal::from(1440) {
            TradingStyle::DayTrader
        } else if avg_hold_time < Decimal::from(10080) {
            TradingStyle::SwingTrader
        } else {
            TradingStyle::Holder
        };

        // Add style-specific insights
        match quality.trading_style {
            TradingStyle::Scalper => {
                quality.strengths.push("Fast execution trader".to_string());
                if report.summary.total_trades > 10 {
                    qualification_score += 5.0;
                }
            },
            TradingStyle::DayTrader => {
                quality.strengths.push("Active day trader".to_string());
                qualification_score += 5.0;
            },
            TradingStyle::SwingTrader => {
                quality.strengths.push("Patient swing trader".to_string());
                qualification_score += 3.0;
            },
            TradingStyle::Holder => {
                quality.concerns.push("Long-term holder - may not be active trader".to_string());
            },
            TradingStyle::Unknown => {},
        }

        // 3. Risk assessment
        quality.risk_level = if win_rate >= Decimal::from(70) && roi >= Decimal::from(50) {
            RiskLevel::Low
        } else if win_rate >= Decimal::from(50) && roi >= Decimal::from(25) {
            RiskLevel::Medium
        } else if win_rate >= Decimal::from(30) {
            RiskLevel::High
        } else {
            RiskLevel::VeryHigh
        };

        // 4. Final scoring and recommendation
        quality.score = qualification_score;
        quality.is_qualified = passes_basic_checks && qualification_score >= 50.0;
        quality.copy_trade_recommended = quality.is_qualified && matches!(quality.risk_level, RiskLevel::Low | RiskLevel::Medium);

        // Log assessment
        if quality.is_qualified {
            info!("✅ Trader {} QUALIFIED for copy trading - Score: {:.1}, Risk: {:?}", 
                report.wallet_address, quality.score, quality.risk_level);
        } else {
            debug!("❌ Trader {} not qualified - Score: {:.1}, Concerns: {}", 
                report.wallet_address, quality.score, quality.concerns.len());
        }

        Ok(quality)
    }

    /// Filter a batch of P&L reports to find qualified traders
    pub fn filter_traders(&self, reports: Vec<PnLReport>) -> Result<Vec<(PnLReport, TraderQuality)>> {
        info!("🔍 Filtering {} traders for copy trading qualification", reports.len());
        
        let total_reports = reports.len();
        let mut qualified_traders = Vec::new();
        let mut total_qualified = 0;

        for report in reports {
            match self.evaluate_trader(&report) {
                Ok(quality) => {
                    if quality.is_qualified {
                        total_qualified += 1;
                        qualified_traders.push((report, quality));
                    }
                },
                Err(e) => {
                    warn!("Failed to evaluate trader {}: {}", report.wallet_address, e);
                }
            }
        }

        info!("✅ Found {} qualified traders out of {} analyzed ({:.1}% qualification rate)", 
            total_qualified, total_reports, 
            (total_qualified as f64 / total_reports as f64) * 100.0);

        // Sort by score (best traders first)
        qualified_traders.sort_by(|a, b| b.1.score.partial_cmp(&a.1.score).unwrap_or(std::cmp::Ordering::Equal));

        Ok(qualified_traders)
    }
}

/// Generate a summary report for trader filtering results
pub fn generate_trader_summary(qualified_traders: &[(PnLReport, TraderQuality)]) -> String {
    if qualified_traders.is_empty() {
        return "No qualified traders found for copy trading.".to_string();
    }

    let mut summary = format!("🎯 QUALIFIED COPY TRADERS SUMMARY\n");
    summary.push_str(&format!("Found {} qualified traders:\n\n", qualified_traders.len()));

    for (i, (report, quality)) in qualified_traders.iter().take(10).enumerate() {
        summary.push_str(&format!(
            "{}. {} (Score: {:.1})\n   💰 P&L: ${} | 📈 ROI: {}% | 🎯 Win Rate: {}% | 📊 Trades: {}\n   ⏱️  Style: {:?} | 🚨 Risk: {:?}\n\n",
            i + 1,
            report.wallet_address,
            quality.score,
            report.summary.total_pnl_usd,
            report.summary.roi_percentage,
            report.summary.win_rate,
            report.summary.total_trades,
            quality.trading_style,
            quality.risk_level
        ));
    }

    if qualified_traders.len() > 10 {
        summary.push_str(&format!("... and {} more qualified traders\n", qualified_traders.len() - 10));
    }

    summary
}