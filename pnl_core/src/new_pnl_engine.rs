use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info, warn};

use crate::new_parser::{NewEventType, NewFinancialEvent};
use crate::zerion_balance_fetcher::ZerionBalanceFetcher;

/// A matched trade pair in FIFO order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchedTrade {
    /// Buy event details
    pub buy_event: NewFinancialEvent,

    /// Sell event details
    pub sell_event: NewFinancialEvent,

    /// Quantity matched (min of buy and sell quantities)
    pub matched_quantity: Decimal,

    /// Realized P&L for this matched pair: (sell_price - buy_price) × quantity
    pub realized_pnl_usd: Decimal,

    /// Hold time in seconds
    pub hold_time_seconds: i64,
}

/// An unmatched sell event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnmatchedSell {
    /// The sell event
    pub sell_event: NewFinancialEvent,

    /// Quantity that couldn't be matched
    pub unmatched_quantity: Decimal,

    /// Phantom buy price (same as sell price, resulting in zero P&L)
    pub phantom_buy_price: Decimal,

    /// P&L (always zero for unmatched sells)
    pub phantom_pnl_usd: Decimal,
}

/// A received token event (airdrops, transfers from other wallets)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceivedToken {
    /// The receive event
    pub receive_event: NewFinancialEvent,

    /// Remaining quantity available for consumption by sells
    pub remaining_quantity: Decimal,
}

/// Consumption of received tokens by a sell event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiveConsumption {
    /// The receive event that provided the tokens
    pub receive_event: NewFinancialEvent,

    /// The sell event that consumed the tokens
    pub sell_event: NewFinancialEvent,

    /// Quantity consumed from this receive event
    pub consumed_quantity: Decimal,

    /// No P&L impact (received tokens have no cost basis)
    pub pnl_impact_usd: Decimal, // Always zero
}

/// Remaining position after all matching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemainingPosition {
    /// Token details
    pub token_address: String,
    pub token_symbol: String,

    /// Remaining quantity from bought tokens
    #[serde(default)]
    pub bought_quantity: Decimal,

    /// Remaining quantity from received tokens
    #[serde(default)]
    pub received_quantity: Decimal,

    /// Weighted average cost basis (only applies to bought tokens)
    pub avg_cost_basis_usd: Decimal,

    /// Total cost basis for remaining bought position
    pub total_cost_basis_usd: Decimal,
}

/// Token-level P&L results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPnLResult {
    /// Token details
    pub token_address: String,
    pub token_symbol: String,

    /// All matched trades for this token (includes phantom buy matches)
    pub matched_trades: Vec<MatchedTrade>,

    /// Remaining position (if any)
    pub remaining_position: Option<RemainingPosition>,

    /// Total realized P&L for this token
    pub total_realized_pnl_usd: Decimal,

    /// Total unrealized P&L for this token (calculated separately)
    pub total_unrealized_pnl_usd: Decimal,

    /// Total P&L (realized + unrealized)
    pub total_pnl_usd: Decimal,

    /// Trade statistics
    pub total_trades: u32,
    pub winning_trades: u32,
    pub losing_trades: u32,
    pub win_rate_percentage: Decimal,

    /// Hold time statistics (in minutes)
    pub avg_hold_time_minutes: Decimal,
    pub min_hold_time_minutes: Decimal,
    pub max_hold_time_minutes: Decimal,

    /// Investment metrics
    #[serde(default)]
    pub total_invested_usd: Decimal,
    #[serde(default)]
    pub total_returned_usd: Decimal,

    /// Streak analytics
    #[serde(default)]
    pub current_winning_streak: u32,
    #[serde(default)]
    pub longest_winning_streak: u32,
    #[serde(default)]
    pub current_losing_streak: u32,
    #[serde(default)]
    pub longest_losing_streak: u32,

    /// Receive-related tracking
    #[serde(default)]
    pub receive_consumptions: Vec<ReceiveConsumption>,
    #[serde(default)]
    pub total_received_quantity: Decimal,
    #[serde(default)]
    pub total_received_sold_quantity: Decimal,
    #[serde(default)]
    pub remaining_received_quantity: Decimal,
}

/// Portfolio-level P&L results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioPnLResult {
    /// Wallet address
    pub wallet_address: String,

    /// Results for each token
    pub token_results: Vec<TokenPnLResult>,

    /// Portfolio-level aggregated metrics
    pub total_realized_pnl_usd: Decimal,
    pub total_unrealized_pnl_usd: Decimal,
    pub total_pnl_usd: Decimal,

    /// Portfolio trade statistics
    pub total_trades: u32,
    #[serde(default)]
    pub winning_trades: u32,
    #[serde(default)]
    pub losing_trades: u32,
    pub overall_win_rate_percentage: Decimal,
    pub avg_hold_time_minutes: Decimal,

    /// Number of tokens analyzed
    pub tokens_analyzed: u32,

    /// Analysis metadata
    pub events_processed: u32,
    pub analysis_timestamp: DateTime<Utc>,

    /// Portfolio investment metrics
    #[serde(default)]
    pub total_invested_usd: Decimal,
    #[serde(default)]
    pub total_returned_usd: Decimal,

    /// Portfolio streak analytics
    #[serde(default)]
    pub current_winning_streak: u32,
    #[serde(default)]
    pub longest_winning_streak: u32,
    #[serde(default)]
    pub current_losing_streak: u32,
    #[serde(default)]
    pub longest_losing_streak: u32,

    /// Calculated profit percentage
    #[serde(default)]
    pub profit_percentage: Decimal,

    /// Advanced filtering metrics
    #[serde(default)]
    pub unique_tokens_count: u32,
    #[serde(default)]
    pub active_days_count: u32,
}

/// P&L Engine Module
/// Calculates comprehensive P&L metrics by consuming financial events
/// and performing token-by-token analysis with FIFO matching
pub struct NewPnLEngine {
    wallet_address: String,
    balance_fetcher: Option<ZerionBalanceFetcher>,
}

impl NewPnLEngine {
    /// Create a new P&L engine for a specific wallet
    pub fn new(wallet_address: String) -> Self {
        Self {
            wallet_address,
            balance_fetcher: None,
        }
    }

    /// Create a new P&L engine with Zerion balance fetching enabled
    pub fn with_balance_fetcher(wallet_address: String, balance_fetcher: ZerionBalanceFetcher) -> Self {
        Self {
            wallet_address,
            balance_fetcher: Some(balance_fetcher),
        }
    }

    /// Check if a token is an exchange currency (used for trading, not investment)
    /// This prevents double-counting in portfolio totals across all supported chains
    fn is_exchange_currency_token(token_result: &TokenPnLResult) -> bool {
        // Debug: Log the token being checked
        debug!(
            "🔍 Checking if {} ({}) is an exchange currency",
            token_result.token_symbol,
            token_result.token_address
        );

        // Check if this is an exchange currency based on trading patterns:
        // 1. All trades have very short hold times (1-2 seconds = phantom trades)
        // 2. All trades have $0 P&L (phantom buy-sell pairs)

        let is_phantom_pattern = token_result.avg_hold_time_minutes < Decimal::new(1, 1) && // 0.1 minutes = 6 seconds avg
            token_result.total_realized_pnl_usd.abs() < Decimal::new(1, 2) && // 0.01 = ~$0 P&L
            token_result.total_trades > 0;

        debug!(
            "   Phantom pattern check: avg_hold_time={:.2}min, pnl=${:.4}, trades={} => {}",
            token_result.avg_hold_time_minutes,
            token_result.total_realized_pnl_usd,
            token_result.total_trades,
            is_phantom_pattern
        );

        // Also check for known exchange currency addresses across chains
        // NOTE: SOL has multiple address formats in the wild, so we check all variations
        let is_known_exchange_currency = matches!(
            token_result.token_address.as_str(),
            // Solana - Native & Stablecoins
            "So11111111111111111111111111111111111111112" |  // SOL (native, full base58)
            "11111111111111111111111111111111" |             // SOL (32 ones, truncated format)
            "11111111111111111111111111111112" |             // SOL (base58 decoded variant)
            "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB" | // USDT
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v" | // USDC

            // Ethereum - Wrapped & Stablecoins
            "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2" | // WETH
            "0xdAC17F958D2ee523a2206206994597C13D831ec7" | // USDT
            "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48" | // USDC
            "0x6B175474E89094C44Da98b954EedeAC495271d0F" | // DAI

            // Binance Smart Chain - Wrapped & Stablecoins
            "0xbb4CdB9CBd36B01bD1cBaEBF2De08d9173bc095c" | // WBNB
            "0x55d398326f99059fF775485246999027B3197955" | // USDT
            "0x8AC76a51cc950d9822D68b83fE1Ad97B32Cd580d" | // USDC
            "0x1AF3F329e8BE154074D8769D1FFa4eE058B1DBc3" | // DAI
            "0xe9e7CEA3DedcA5984780Bafc599bD69ADd087D56" | // BUSD (deprecated)

            // Base - Wrapped ETH & Stablecoins
            "0x4200000000000000000000000000000000000006" | // WETH
            "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913" | // USDC
            "0xfde4C96c8593536E31F229EA8f37b2ADa2699bb2"  // USDT
        );

        debug!(
            "   Known address check: {} => {}",
            token_result.token_address,
            is_known_exchange_currency
        );

        let should_filter = is_phantom_pattern || is_known_exchange_currency;

        if should_filter {
            info!(
                "✓ Filtering {} ({}) - phantom_pattern: {}, known_address: {}",
                token_result.token_symbol,
                token_result.token_address,
                is_phantom_pattern,
                is_known_exchange_currency
            );
        } else {
            debug!(
                "✗ NOT filtering {} ({}) - will be included in results",
                token_result.token_symbol,
                token_result.token_address
            );
        }

        should_filter
    }

    /// Enable balance fetching by setting the balance fetcher
    pub fn set_balance_fetcher(&mut self, balance_fetcher: ZerionBalanceFetcher) {
        self.balance_fetcher = Some(balance_fetcher);
    }

    /// Calculate portfolio P&L from financial events
    /// This is the main entry point for P&L calculation
    pub fn calculate_portfolio_pnl(
        &self,
        events_by_token: HashMap<String, Vec<NewFinancialEvent>>,
        current_prices: Option<HashMap<String, Decimal>>,
    ) -> Result<PortfolioPnLResult, String> {
        info!(
            "Starting P&L calculation for wallet {} with {} tokens",
            self.wallet_address,
            events_by_token.len()
        );

        let mut token_results = Vec::new();
        let mut total_realized_pnl = Decimal::ZERO;
        let mut total_unrealized_pnl = Decimal::ZERO;
        let mut total_trades = 0u32;
        let mut total_events_processed = 0u32;

        // Store the original count before we consume the HashMap
        let tokens_analyzed = events_by_token.len() as u32;

        let mut total_winning_trades = 0u32;
        let mut total_losing_trades = 0u32;

        // Enhanced tracking for receive vs buy debugging
        let mut total_receive_events = 0u32;

        // Process each token separately (supports parallel processing)
        for (token_address, events) in events_by_token {
            debug!(
                "Processing token {} with {} events",
                token_address,
                events.len()
            );

            total_events_processed += events.len() as u32;

            // Count receive events for debug tracking
            let receive_events_count = events
                .iter()
                .filter(|e| e.event_type == NewEventType::Receive)
                .count() as u32;
            total_receive_events += receive_events_count;

            let current_price = current_prices
                .as_ref()
                .and_then(|prices| prices.get(&token_address))
                .copied();

            match self.calculate_token_pnl(events, current_price) {
                Ok(token_result) => {
                    // Skip exchange currencies entirely - don't add to results or aggregates
                    if Self::is_exchange_currency_token(&token_result) {
                        debug!(
                            "Skipping exchange currency {} from portfolio results (used for trading, not investment)",
                            token_result.token_symbol
                        );
                        continue;
                    }

                    // Only non-exchange-currency tokens reach here
                    total_realized_pnl += token_result.total_realized_pnl_usd;
                    total_unrealized_pnl += token_result.total_unrealized_pnl_usd;
                    total_trades += token_result.total_trades;
                    total_winning_trades += token_result.winning_trades;
                    total_losing_trades += token_result.losing_trades;

                    token_results.push(token_result);
                }
                Err(e) => {
                    warn!("Failed to calculate P&L for token {}: {}", token_address, e);
                    continue;
                }
            }
        }

        // Calculate portfolio-level statistics
        let total_pnl = total_realized_pnl + total_unrealized_pnl;

        // Enhanced debugging for zero P&L scenarios
        if total_pnl == Decimal::ZERO && total_events_processed > 0 {
            warn!(
                "⚠️  ZERO P&L DETECTED for wallet {} despite processing {} events across {} tokens!",
                self.wallet_address,
                total_events_processed,
                tokens_analyzed
            );

            // Analyze token results to find the issue
            let tokens_with_only_sells = token_results
                .iter()
                .filter(|t| t.total_trades > 0 && t.total_invested_usd == Decimal::ZERO)
                .count();

            if tokens_with_only_sells > 0 {
                warn!(
                    "    → {} tokens had only SELL events (no BUY events), resulting in phantom buys with zero P&L",
                    tokens_with_only_sells
                );
            }

            // Enhanced debugging for receive vs buy tracking
            if total_receive_events > 0 {
                debug!(
                    "🎯 Enhanced P&L Debug - Wallet {}: {} received token events processed, enhanced FIFO logic applied",
                    self.wallet_address,
                    total_receive_events
                );
                debug!(
                    "  → Received tokens consume FIFO priority in sells (P&L impact: $0 as designed for received tokens)"
                );
            } else {
                debug!(
                    "🔍 Enhanced P&L Debug - Wallet {}: No received tokens detected, all events were buy/sell pairs",
                    self.wallet_address
                );
            }

            info!(
                "    → Total trades: {}, Winning: {}, Losing: {}",
                total_trades,
                total_winning_trades,
                total_losing_trades
            );
        }

        // Sanity check for unrealistic total P&L values
        let hundred_million = Decimal::from(100_000_000);
        if total_pnl.abs() > hundred_million {
            warn!(
                "Unrealistic total P&L detected for wallet {}: ${} (Realized: ${}, Unrealized: ${}) - likely data error",
                self.wallet_address,
                total_pnl,
                total_realized_pnl,
                total_unrealized_pnl
            );
        }

        let overall_win_rate = if total_trades > 0 {
            Decimal::from(total_winning_trades * 100) / Decimal::from(total_trades)
        } else {
            Decimal::ZERO
        };

        let avg_hold_time = if !token_results.is_empty() {
            token_results
                .iter()
                .map(|t| t.avg_hold_time_minutes)
                .sum::<Decimal>()
                / Decimal::from(token_results.len())
        } else {
            Decimal::ZERO
        };

        // Calculate portfolio investment metrics
        // (exchange currencies already excluded from token_results)
        let total_invested_usd: Decimal = token_results
            .iter()
            .map(|t| t.total_invested_usd)
            .sum();

        let total_returned_usd: Decimal = token_results
            .iter()
            .map(|t| t.total_returned_usd)
            .sum();

        // Calculate portfolio-level streaks (continue across all tokens chronologically)
        let (
            current_winning_streak,
            longest_winning_streak,
            current_losing_streak,
            longest_losing_streak,
        ) = self.calculate_portfolio_streaks(&token_results);

        // Calculate total return percentage (includes original investment)
        // Formula: ((total_sells + current_holdings_value) / total_invested) × 100
        // Result interpretation: <100% = loss, 100% = break even, >100% = profit
        // E.g., 130% means you got 130% of your investment back (30% profit)
        // (exchange currencies already excluded from token_results)
        let current_holdings_value = total_unrealized_pnl + token_results
            .iter()
            .filter_map(|t| t.remaining_position.as_ref())
            .map(|p| p.total_cost_basis_usd)
            .sum::<Decimal>();

        let profit_percentage = if total_invested_usd > Decimal::ZERO {
            (((total_returned_usd + current_holdings_value) / total_invested_usd) * Decimal::from(100)).round_dp(2)
        } else {
            Decimal::ZERO
        };

        // Calculate advanced filtering metrics
        let unique_tokens_count = tokens_analyzed;
        let active_days_count = self.calculate_active_days_count(&token_results);

        let result = PortfolioPnLResult {
            wallet_address: self.wallet_address.clone(),
            token_results,
            total_realized_pnl_usd: total_realized_pnl,
            total_unrealized_pnl_usd: total_unrealized_pnl,
            total_pnl_usd: total_pnl,
            total_trades,
            winning_trades: total_winning_trades,
            losing_trades: total_losing_trades,
            overall_win_rate_percentage: overall_win_rate,
            avg_hold_time_minutes: avg_hold_time,
            tokens_analyzed,
            events_processed: total_events_processed,
            analysis_timestamp: Utc::now(),
            total_invested_usd,
            total_returned_usd,
            current_winning_streak,
            longest_winning_streak,
            current_losing_streak,
            longest_losing_streak,
            profit_percentage,
            unique_tokens_count,
            active_days_count,
        };

        info!(
            "P&L calculation completed for wallet {}: Total P&L: ${}, Trades: {}, Win Rate: {}%",
            self.wallet_address,
            result.total_pnl_usd,
            result.total_trades,
            result.overall_win_rate_percentage
        );

        Ok(result)
    }

    /// Calculate P&L for a single token using FIFO matching
    pub fn calculate_token_pnl(
        &self,
        mut events: Vec<NewFinancialEvent>,
        current_price: Option<Decimal>,
    ) -> Result<TokenPnLResult, String> {
        if events.is_empty() {
            return Err("No events provided for token P&L calculation".to_string());
        }

        let token_address = events[0].token_address.clone();
        let token_symbol = events[0].token_symbol.clone();

        // Sort events chronologically (required for FIFO)
        // Primary sort: timestamp, Secondary sort: transaction hash for stability
        events.sort_by(|a, b| {
            a.timestamp
                .cmp(&b.timestamp)
                .then_with(|| a.transaction_hash.cmp(&b.transaction_hash))
        });

        debug!(
            "Calculating FIFO P&L for token {} ({}) with {} events",
            token_symbol,
            token_address,
            events.len()
        );

        // Validate all event prices are positive (data quality check)
        for event in &events {
            if event.usd_price_per_token <= Decimal::ZERO {
                return Err(format!(
                    "Invalid price for token {} ({}): price={} in tx {}. Prices must be positive.",
                    event.token_symbol,
                    event.token_address,
                    event.usd_price_per_token,
                    event.transaction_hash
                ));
            }
        }

        // Separate buy, sell, and receive events (already sorted chronologically)
        let mut buy_events: Vec<NewFinancialEvent> = events
            .iter()
            .filter(|e| e.event_type == NewEventType::Buy)
            .cloned()
            .collect();

        let sell_events: Vec<NewFinancialEvent> = events
            .iter()
            .filter(|e| e.event_type == NewEventType::Sell)
            .cloned()
            .collect();

        let receive_events: Vec<NewFinancialEvent> = events
            .iter()
            .filter(|e| e.event_type == NewEventType::Receive)
            .cloned()
            .collect();

        // Enhanced debugging for token event pairing including receives
        let total_buy_quantity: Decimal = buy_events.iter().map(|e| e.quantity).sum();
        let total_receive_quantity: Decimal = receive_events.iter().map(|e| e.quantity).sum();
        let total_sell_quantity: Decimal = sell_events.iter().map(|e| e.quantity).sum();

        info!(
            "📊 EVENT SUMMARY for {}: {} buys (total: {}), {} sells (total: {}), {} receives (total: {})",
            token_symbol,
            buy_events.len(),
            total_buy_quantity,
            sell_events.len(),
            total_sell_quantity,
            receive_events.len(),
            total_receive_quantity
        );

        // Log individual events for debugging
        for (i, buy) in buy_events.iter().enumerate() {
            debug!(
                "  Buy #{}: {} {} @ ${} (total: ${}) at {}",
                i + 1,
                buy.quantity,
                token_symbol,
                buy.usd_price_per_token,
                buy.usd_value,
                buy.timestamp
            );
        }

        for (i, receive) in receive_events.iter().enumerate() {
            debug!(
                "  Receive #{}: {} {} at {}",
                i + 1,
                receive.quantity,
                token_symbol,
                receive.timestamp
            );
        }

        for (i, sell) in sell_events.iter().enumerate() {
            debug!(
                "  Sell #{}: {} {} @ ${} (total: ${}) at {}",
                i + 1,
                sell.quantity,
                token_symbol,
                sell.usd_price_per_token,
                sell.usd_value,
                sell.timestamp
            );
        }

        if buy_events.is_empty() && receive_events.is_empty() && !sell_events.is_empty() {
            warn!(
                "⚠️  Token {} ({}): No buy or receive events found but {} sell events exist. This will result in phantom buys with zero P&L.",
                token_symbol,
                token_address,
                sell_events.len()
            );
        }

        // === ENHANCED DEBUG LOGGING for total_invested calculation (Fix #4) ===
        let buy_events_for_invested: Vec<&NewFinancialEvent> = events
            .iter()
            .filter(|e| e.event_type == NewEventType::Buy)
            .filter(|e| !e.transaction_hash.starts_with("phantom_buy_")) // Exclude phantom buys
            .collect();

        debug!(
            "📊 Total Invested Calculation for {}: {} buy events (excluding phantom buys)",
            token_symbol,
            buy_events_for_invested.len()
        );

        for (i, event) in buy_events_for_invested.iter().enumerate() {
            debug!(
                "  Buy #{}: {} {} @ ${:.10} = ${:.2} (tx: {}...)",
                i + 1,
                event.quantity,
                event.token_symbol,
                event.usd_price_per_token,
                event.usd_value,
                &event.transaction_hash[..8.min(event.transaction_hash.len())]
            );
        }

        let total_invested_usd: Decimal = buy_events_for_invested.iter().map(|e| e.usd_value).sum();

        debug!(
            "💰 Total invested in {}: ${:.2} (from {} buy events)",
            token_symbol,
            total_invested_usd,
            buy_events_for_invested.len()
        );
        // === END ENHANCED DEBUG LOGGING ===

        // === VALIDATION: Check for extreme buy/sell imbalances (Fix #3) ===
        let total_sell_value: Decimal = events
            .iter()
            .filter(|e| e.event_type == NewEventType::Sell)
            .map(|e| e.usd_value)
            .sum();

        if total_invested_usd > Decimal::ZERO && total_sell_value > Decimal::ZERO {
            let imbalance_ratio = total_invested_usd / total_sell_value;

            // Warn if buy value is >10x sell value (likely parsing error)
            if imbalance_ratio > Decimal::from(10) {
                warn!(
                    "⚠️  EXTREME BUY/SELL IMBALANCE detected for {} ({}): ${:.2} buy vs ${:.2} sell ({}x ratio)",
                    token_symbol,
                    &token_address[..8.min(token_address.len())],
                    total_invested_usd,
                    total_sell_value,
                    imbalance_ratio
                );
                warn!(
                    "    → This likely indicates a transaction parsing error (multi-hop swaps, duplicates, etc.)"
                );
                warn!(
                    "    → Review transaction logs for this token to identify erroneous BUY events"
                );
            } else if imbalance_ratio > Decimal::from(3) {
                info!(
                    "ℹ️  Moderate buy/sell imbalance for {}: ${:.2} buy vs ${:.2} sell ({:.1}x ratio)",
                    token_symbol,
                    total_invested_usd,
                    total_sell_value,
                    imbalance_ratio
                );
            }
        } else if total_invested_usd > Decimal::ZERO && total_sell_value == Decimal::ZERO {
            info!(
                "ℹ️  Token {} has only BUY events (${:.2}), no sells yet - all tokens remain in position",
                token_symbol,
                total_invested_usd
            );
        }
        // === END VALIDATION ===

        // Perform enhanced FIFO matching that handles received tokens separately
        let (matched_trades, receive_consumptions) = self.perform_enhanced_fifo_matching(&mut buy_events, &sell_events, &receive_events)?;

        // Calculate total_returned_usd from matched trades only
        // This ensures we only count proceeds from selling BOUGHT tokens,
        // excluding proceeds from selling received or pre-existing (implicit receive) tokens
        let total_returned_usd: Decimal = matched_trades
            .iter()
            .map(|t| t.sell_event.usd_value)
            .sum();

        // Calculate remaining position from unmatched buys and remaining receives
        let remaining_position = self.calculate_enhanced_remaining_position(
            &buy_events,
            &receive_events,
            &receive_consumptions,
            &token_address,
            &token_symbol
        )?;

        // Calculate realized P&L (all P&L is now captured in matched_trades)
        let total_realized_pnl: Decimal = matched_trades
            .iter()
            .map(|t| t.realized_pnl_usd)
            .sum::<Decimal>();

        // Calculate unrealized P&L from remaining position after FIFO matching
        // This only includes tokens that were actually analyzed in the transaction history
        let total_unrealized_pnl = self.calculate_unrealized_pnl(&remaining_position, current_price);

        // Calculate trade statistics
        let total_trades = matched_trades.len() as u32;
        let winning_trades = matched_trades
            .iter()
            .filter(|t| t.realized_pnl_usd > Decimal::ZERO)
            .count() as u32;
        let losing_trades = total_trades - winning_trades;

        let win_rate = if total_trades > 0 {
            Decimal::from(winning_trades * 100) / Decimal::from(total_trades)
        } else {
            Decimal::ZERO
        };

        // Calculate hold time statistics
        let (avg_hold_time, min_hold_time, max_hold_time) =
            self.calculate_hold_time_stats(&matched_trades);

        // Calculate streak analytics
        let (
            current_winning_streak,
            longest_winning_streak,
            current_losing_streak,
            longest_losing_streak,
        ) = self.calculate_streak_analytics(&matched_trades);

        // Calculate receive-related metrics
        // Count both original receive_events and implicit receives created during FIFO matching
        let original_received_quantity: Decimal = receive_events.iter().map(|e| e.quantity).sum();
        let implicit_received_quantity: Decimal = receive_consumptions
            .iter()
            .filter(|c| c.receive_event.transaction_hash.starts_with("implicit_receive_"))
            .map(|c| c.consumed_quantity)
            .sum();
        let total_received_quantity = original_received_quantity + implicit_received_quantity;

        let total_received_sold_quantity: Decimal = receive_consumptions.iter().map(|c| c.consumed_quantity).sum();
        let remaining_received_quantity = total_received_quantity - total_received_sold_quantity;

        let result = TokenPnLResult {
            token_address,
            token_symbol,
            matched_trades,
            remaining_position,
            total_realized_pnl_usd: total_realized_pnl,
            total_unrealized_pnl_usd: total_unrealized_pnl,
            total_pnl_usd: total_realized_pnl + total_unrealized_pnl,
            total_trades,
            winning_trades,
            losing_trades,
            win_rate_percentage: win_rate,
            avg_hold_time_minutes: avg_hold_time,
            min_hold_time_minutes: min_hold_time,
            max_hold_time_minutes: max_hold_time,
            total_invested_usd,
            total_returned_usd,
            current_winning_streak,
            longest_winning_streak,
            current_losing_streak,
            longest_losing_streak,
            receive_consumptions,
            total_received_quantity,
            total_received_sold_quantity,
            remaining_received_quantity,
        };

        debug!(
            "Token {} Enhanced P&L Summary:",
            result.token_symbol
        );
        debug!(
            "  💰 P&L: Realized: ${}, Unrealized: ${}, Total: ${}, Trades: {}, Win Rate: {}%",
            result.total_realized_pnl_usd,
            result.total_unrealized_pnl_usd,
            result.total_pnl_usd,
            result.total_trades,
            result.win_rate_percentage
        );
        debug!(
            "  🎁 Receives: Total: {}, Sold: {}, Remaining: {} (no P&L impact)",
            result.total_received_quantity,
            result.total_received_sold_quantity,
            result.remaining_received_quantity
        );
        debug!(
            "  📊 Consumptions: {} receive→sell events excluded from P&L",
            result.receive_consumptions.len()
        );

        Ok(result)
    }


    /// Enhanced FIFO matching that handles received tokens separately
    /// Phase 1: Consume received tokens first (FIFO within receives)
    /// Phase 2: Match remaining sells with bought tokens (traditional FIFO)
    fn perform_enhanced_fifo_matching(
        &self,
        buy_events: &mut Vec<NewFinancialEvent>,
        sell_events: &[NewFinancialEvent],
        receive_events: &[NewFinancialEvent],
    ) -> Result<(Vec<MatchedTrade>, Vec<ReceiveConsumption>), String> {
        let mut matched_trades = Vec::new();
        let mut receive_consumptions = Vec::new();

        // Create mutable copies for consumption tracking
        let mut remaining_receives: Vec<ReceivedToken> = receive_events
            .iter()
            .map(|event| ReceivedToken {
                receive_event: event.clone(),
                remaining_quantity: event.quantity,
            })
            .collect();

        info!("🔄 Starting FIFO matching - Phase 1: Match BOUGHT tokens FIRST (priority), Phase 2: Received tokens (fallback only)");
        let total_available_receives: Decimal = remaining_receives.iter().map(|r| r.remaining_quantity).sum();
        info!("   Total received tokens available: {}", total_available_receives);

        // Process each sell event
        for (sell_idx, sell_event) in sell_events.iter().enumerate() {
            let mut remaining_sell_quantity = sell_event.quantity;

            info!(
                "🔹 Processing Sell #{}/{}: {} {} @ ${}",
                sell_idx + 1,
                sell_events.len(),
                sell_event.quantity,
                sell_event.token_symbol,
                sell_event.usd_price_per_token
            );

            // ═══════════════════════════════════════════════════════════
            // NEW PHASE 1: Match BOUGHT tokens FIRST (priority)
            // ═══════════════════════════════════════════════════════════
            info!(
                "🔄 Phase 1: Matching {} {} against BOUGHT tokens (priority)",
                sell_event.quantity,
                sell_event.token_symbol
            );

            // Create a sell event tracker for matching
            let mut remaining_sell_event = sell_event.clone();
            remaining_sell_event.quantity = remaining_sell_quantity;
            remaining_sell_event.usd_value = remaining_sell_quantity * sell_event.usd_price_per_token;

            // Match against bought tokens using traditional FIFO
            let mut total_matched_from_buys = Decimal::ZERO;
            for buy_event in buy_events.iter_mut() {
                if remaining_sell_event.quantity <= Decimal::ZERO {
                    break;
                }

                if buy_event.quantity > Decimal::ZERO && buy_event.timestamp <= remaining_sell_event.timestamp {
                    let matched_quantity = remaining_sell_event.quantity.min(buy_event.quantity);

                    // Calculate price difference with overflow protection
                    let price_diff = remaining_sell_event.usd_price_per_token
                        .checked_sub(buy_event.usd_price_per_token)
                        .ok_or_else(|| {
                            format!(
                                "Price difference overflow: sell_price={}, buy_price={} for token {}",
                                remaining_sell_event.usd_price_per_token,
                                buy_event.usd_price_per_token,
                                remaining_sell_event.token_symbol
                            )
                        })?;

                    // Calculate realized P&L with overflow protection
                    let realized_pnl = price_diff
                        .checked_mul(matched_quantity)
                        .ok_or_else(|| {
                            format!(
                                "P&L multiplication overflow: price_diff={}, quantity={} for token {}",
                                price_diff,
                                matched_quantity,
                                remaining_sell_event.token_symbol
                            )
                        })?;

                    let hold_time_seconds = (remaining_sell_event.timestamp - buy_event.timestamp)
                        .num_seconds()
                        .max(0);

                    // Create a new event representing ONLY the matched portion
                    let matched_buy_portion = NewFinancialEvent {
                        wallet_address: buy_event.wallet_address.clone(),
                        transaction_hash: buy_event.transaction_hash.clone(),
                        timestamp: buy_event.timestamp,
                        token_address: buy_event.token_address.clone(),
                        token_symbol: buy_event.token_symbol.clone(),
                        chain_id: buy_event.chain_id.clone(),
                        event_type: buy_event.event_type.clone(),

                        // Only the matched portion, not remaining quantity
                        quantity: matched_quantity,
                        usd_price_per_token: buy_event.usd_price_per_token,
                        usd_value: matched_quantity * buy_event.usd_price_per_token,
                    };

                    // Create matched sell portion (only the matched quantity)
                    let matched_sell_portion = NewFinancialEvent {
                        wallet_address: remaining_sell_event.wallet_address.clone(),
                        transaction_hash: remaining_sell_event.transaction_hash.clone(),
                        timestamp: remaining_sell_event.timestamp,
                        token_address: remaining_sell_event.token_address.clone(),
                        token_symbol: remaining_sell_event.token_symbol.clone(),
                        chain_id: remaining_sell_event.chain_id.clone(),
                        event_type: remaining_sell_event.event_type.clone(),

                        // Only the matched portion, not remaining quantity
                        quantity: matched_quantity,
                        usd_price_per_token: remaining_sell_event.usd_price_per_token,
                        usd_value: matched_quantity * remaining_sell_event.usd_price_per_token,
                    };

                    matched_trades.push(MatchedTrade {
                        buy_event: matched_buy_portion,
                        sell_event: matched_sell_portion,
                        matched_quantity,
                        realized_pnl_usd: realized_pnl,
                        hold_time_seconds,
                    });

                    // Update remaining quantities and USD values proportionally
                    buy_event.quantity -= matched_quantity;
                    buy_event.usd_value = buy_event.quantity * buy_event.usd_price_per_token;

                    // Clear dust to prevent accumulation
                    // Dust threshold: 1e-18 (smallest meaningful Decimal precision)
                    const DUST_THRESHOLD: i128 = 1; // 1e-18 when scale is 18
                    if buy_event.quantity > Decimal::ZERO &&
                       buy_event.quantity.mantissa().abs() < DUST_THRESHOLD &&
                       buy_event.quantity.scale() >= 18 {
                        debug!(
                            "   🧹 Clearing dust from buy lot: {} {} (below 1e-18 threshold)",
                            buy_event.quantity,
                            buy_event.token_symbol
                        );
                        buy_event.quantity = Decimal::ZERO;
                        buy_event.usd_value = Decimal::ZERO;
                    }

                    remaining_sell_event.quantity -= matched_quantity;
                    remaining_sell_event.usd_value = remaining_sell_event.quantity * remaining_sell_event.usd_price_per_token;

                    total_matched_from_buys += matched_quantity;

                    debug!(
                        "   ✓ Matched {} {} with buy @ ${} -> P&L: ${:.2}. Buy remaining: {}",
                        matched_quantity,
                        buy_event.token_symbol,
                        buy_event.usd_price_per_token,
                        realized_pnl,
                        buy_event.quantity
                    );
                }
            }

            if total_matched_from_buys > Decimal::ZERO {
                info!(
                    "   Phase 1 complete: Matched {} {} with bought tokens",
                    total_matched_from_buys,
                    sell_event.token_symbol
                );
            }

            // Update remaining quantity for Phase 2
            remaining_sell_quantity = remaining_sell_event.quantity;

            // ═══════════════════════════════════════════════════════════
            // NEW PHASE 2: Match RECEIVED tokens (fallback only)
            // ═══════════════════════════════════════════════════════════
            if remaining_sell_quantity > Decimal::ZERO {
                info!(
                    "🔄 Phase 2: Matching remaining {} {} against RECEIVED tokens (fallback)",
                    remaining_sell_quantity,
                    sell_event.token_symbol
                );

                let mut total_consumed_from_receives = Decimal::ZERO;
                for received_token in &mut remaining_receives {
                    if remaining_sell_quantity <= Decimal::ZERO {
                        break;
                    }

                    // Only consume receives that happened BEFORE or AT the sell timestamp
                    if received_token.remaining_quantity > Decimal::ZERO
                        && received_token.receive_event.timestamp <= sell_event.timestamp {
                        let consumed_quantity = remaining_sell_quantity.min(received_token.remaining_quantity);

                        // Record the consumption
                        receive_consumptions.push(ReceiveConsumption {
                            receive_event: received_token.receive_event.clone(),
                            sell_event: sell_event.clone(),
                            consumed_quantity,
                            pnl_impact_usd: Decimal::ZERO, // No P&L impact for received tokens
                        });

                        // Update remaining quantities
                        received_token.remaining_quantity -= consumed_quantity;
                        remaining_sell_quantity -= consumed_quantity;
                        total_consumed_from_receives += consumed_quantity;

                        debug!(
                            "   ✓ Consumed {} {} from receive event (no P&L impact). Receive remaining: {}",
                            consumed_quantity,
                            sell_event.token_symbol,
                            received_token.remaining_quantity
                        );
                    }
                }

                if total_consumed_from_receives > Decimal::ZERO {
                    info!(
                        "   Phase 2 complete: Consumed {} {} from receives. Remaining sell quantity: {}",
                        total_consumed_from_receives,
                        sell_event.token_symbol,
                        remaining_sell_quantity
                    );
                }
            }

            // Sync remaining quantities for Phase 3 check
            remaining_sell_event.quantity = remaining_sell_quantity;

            // ═══════════════════════════════════════════════════════════
            // PHASE 3: Implicit receives (UNCHANGED)
            // ═══════════════════════════════════════════════════════════
            // If there's still unmatched sell quantity, create implicit receive
            // (tokens held from outside the analysis timeframe)
            if remaining_sell_event.quantity > Decimal::ZERO {
                    let implicit_receive = NewFinancialEvent {
                        wallet_address: remaining_sell_event.wallet_address.clone(),
                        transaction_hash: format!("implicit_receive_{}", remaining_sell_event.transaction_hash),
                        timestamp: remaining_sell_event.timestamp - chrono::Duration::seconds(1),
                        event_type: NewEventType::Receive,
                        token_address: remaining_sell_event.token_address.clone(),
                        token_symbol: remaining_sell_event.token_symbol.clone(),
                        quantity: remaining_sell_event.quantity,
                        usd_price_per_token: Decimal::ZERO, // No cost basis for pre-existing holdings
                        usd_value: Decimal::ZERO,
                        chain_id: remaining_sell_event.chain_id.clone(),
                    };

                    // Add to remaining receives and consume immediately
                    let consumed_quantity = remaining_sell_event.quantity;

                    receive_consumptions.push(ReceiveConsumption {
                        receive_event: implicit_receive,
                        sell_event: remaining_sell_event.clone(),
                        consumed_quantity,
                        pnl_impact_usd: Decimal::ZERO, // No P&L impact for pre-existing holdings
                    });

                    info!(
                        "   ⚠️  Created implicit receive for {} {} (pre-existing holdings from outside timeframe - excluded from P&L)",
                        consumed_quantity, remaining_sell_event.token_symbol
                    );
                }
        }

        // Log final matching summary
        let total_consumed_receives: Decimal = receive_consumptions.iter().map(|c| c.consumed_quantity).sum();
        let total_matched_buys: Decimal = matched_trades.iter().map(|t| t.matched_quantity).sum();
        let total_realized_pnl: Decimal = matched_trades.iter().map(|t| t.realized_pnl_usd).sum();

        info!(
            "✅ Enhanced FIFO matching complete: {} receives consumed (no P&L), {} buys matched (P&L: ${:.2}), {} trades total",
            total_consumed_receives,
            total_matched_buys,
            total_realized_pnl,
            matched_trades.len()
        );

        Ok((matched_trades, receive_consumptions))
    }

    /// Calculate remaining position from unmatched buy events (LEGACY - includes phantom buys)
    /// This method is kept for backward compatibility with the legacy unrealized P&L calculation
    #[allow(dead_code)]
    fn calculate_remaining_position(
        &self,
        remaining_buys: &[NewFinancialEvent],
        token_address: &str,
        token_symbol: &str,
    ) -> Result<Option<RemainingPosition>, String> {
        if remaining_buys.is_empty() {
            return Ok(None);
        }

        let total_quantity: Decimal = remaining_buys.iter().map(|b| b.quantity).sum();
        let total_cost: Decimal = remaining_buys.iter().map(|b| b.usd_value).sum();

        if total_quantity <= Decimal::ZERO {
            return Ok(None);
        }

        let avg_cost_basis = total_cost / total_quantity;

        let position = RemainingPosition {
            token_address: token_address.to_string(),
            token_symbol: token_symbol.to_string(),
            bought_quantity: total_quantity, // Legacy function treats all as bought
            received_quantity: Decimal::ZERO, // Legacy doesn't track receives
            avg_cost_basis_usd: avg_cost_basis,
            total_cost_basis_usd: total_cost,
        };

        debug!(
            "LEGACY remaining position (includes phantom buys): {} {} @ avg cost ${} (total cost: ${})",
            position.bought_quantity,
            position.token_symbol,
            position.avg_cost_basis_usd,
            position.total_cost_basis_usd
        );

        Ok(Some(position))
    }

    /// Calculate enhanced remaining position including both bought and received tokens
    fn calculate_enhanced_remaining_position(
        &self,
        remaining_buys: &[NewFinancialEvent],
        receive_events: &[NewFinancialEvent],
        receive_consumptions: &[ReceiveConsumption],
        token_address: &str,
        token_symbol: &str,
    ) -> Result<Option<RemainingPosition>, String> {
        info!("📦 Calculating remaining position for {}", token_symbol);

        // Calculate remaining bought tokens (exclude phantom buys)
        let bought_quantity: Decimal = remaining_buys
            .iter()
            .filter(|e| !e.transaction_hash.starts_with("phantom_buy_"))
            .map(|e| e.quantity)
            .sum();

        let total_bought_cost: Decimal = remaining_buys
            .iter()
            .filter(|e| !e.transaction_hash.starts_with("phantom_buy_"))
            .map(|e| e.usd_value)
            .sum();
        let avg_cost_basis = if bought_quantity > Decimal::ZERO {
            total_bought_cost / bought_quantity
        } else {
            Decimal::ZERO
        };

        info!(
            "   Remaining BOUGHT tokens: {} (from {} buy events, avg cost: ${})",
            bought_quantity,
            remaining_buys.len(),
            avg_cost_basis
        );

        // Calculate remaining received tokens
        let total_received_quantity: Decimal = receive_events.iter().map(|e| e.quantity).sum();
        let consumed_received_quantity: Decimal = receive_consumptions.iter().map(|c| c.consumed_quantity).sum();
        let received_quantity = total_received_quantity - consumed_received_quantity;

        info!(
            "   RECEIVED tokens: {} total - {} consumed = {} remaining",
            total_received_quantity,
            consumed_received_quantity,
            received_quantity
        );

        // Only create position if there are remaining tokens
        if bought_quantity <= Decimal::ZERO && received_quantity <= Decimal::ZERO {
            info!("   ℹ️  No remaining tokens - position is empty");
            return Ok(None);
        }

        let position = RemainingPosition {
            token_address: token_address.to_string(),
            token_symbol: token_symbol.to_string(),
            bought_quantity,
            received_quantity,
            avg_cost_basis_usd: avg_cost_basis,
            total_cost_basis_usd: total_bought_cost, // Only bought tokens have cost basis
        };

        info!(
            "✅ Remaining position: {} {} bought + {} {} received (only bought tokens affect unrealized P&L)",
            position.bought_quantity,
            position.token_symbol,
            position.received_quantity,
            position.token_symbol
        );

        if received_quantity > Decimal::ZERO {
            warn!(
                "⚠️  WARNING: {} {} received tokens remain unsold - these do NOT contribute to unrealized P&L!",
                received_quantity,
                token_symbol
            );
        }

        Ok(Some(position))
    }

    /// Calculate unrealized P&L for remaining positions
    /// Following documentation specification: (current_price - weighted_avg_cost_basis) × remaining_quantity
    fn calculate_unrealized_pnl(
        &self,
        remaining_position: &Option<RemainingPosition>,
        current_price: Option<Decimal>,
    ) -> Decimal {
        info!("💰 Calculating unrealized P&L");

        if remaining_position.is_none() {
            info!("   No remaining position - unrealized P&L = 0");
            return Decimal::ZERO;
        }

        if current_price.is_none() {
            info!("   No current price available - unrealized P&L = 0");
            return Decimal::ZERO;
        }

        if let (Some(position), Some(price)) = (remaining_position, current_price) {
            info!(
                "   Position: {} {} bought + {} {} received",
                position.bought_quantity,
                position.token_symbol,
                position.received_quantity,
                position.token_symbol
            );
            info!("   Current price: ${}", price);
            info!("   Avg cost basis: ${}", position.avg_cost_basis_usd);

            // Treat zero or negative prices as missing price data
            if price <= Decimal::ZERO {
                warn!(
                    "   ⚠️  Zero/negative price for {} - unrealized P&L = 0",
                    position.token_symbol
                );
                return Decimal::ZERO;
            }

            // Use the exact formula specified in documentation, but only for bought tokens:
            // (current_price - weighted_avg_cost_basis) × remaining_bought_quantity
            // Received tokens have no cost basis, so no unrealized P&L
            info!(
                "   🧮 Formula: (${} - ${}) × {} bought tokens = ???",
                price,
                position.avg_cost_basis_usd,
                position.bought_quantity
            );

            // Calculate price difference with overflow protection
            let price_diff = match price.checked_sub(position.avg_cost_basis_usd) {
                Some(diff) => diff,
                None => {
                    warn!(
                        "   ⚠️  Price difference overflow in unrealized P&L: price={}, avg_cost={}. Setting to 0.",
                        price,
                        position.avg_cost_basis_usd
                    );
                    return Decimal::ZERO;
                }
            };

            // Calculate unrealized P&L with overflow protection
            let unrealized_pnl = match price_diff.checked_mul(position.bought_quantity) {
                Some(pnl) => pnl,
                None => {
                    warn!(
                        "   ⚠️  Unrealized P&L multiplication overflow: price_diff={}, quantity={}. Setting to 0.",
                        price_diff,
                        position.bought_quantity
                    );
                    return Decimal::ZERO;
                }
            };

            // Sanity check for unrealistic values (> $100M)
            let hundred_million = Decimal::from(100_000_000);
            if unrealized_pnl.abs() > hundred_million {
                warn!(
                    "   ⚠️  UNREALISTIC unrealized P&L: ${} - treating as data error, setting to 0",
                    unrealized_pnl
                );
                return Decimal::ZERO;
            }

            if position.received_quantity > Decimal::ZERO {
                info!(
                    "   ✅ Unrealized P&L = ${:.2} (ONLY from {} bought tokens, {} received tokens EXCLUDED)",
                    unrealized_pnl,
                    position.bought_quantity,
                    position.received_quantity
                );
            } else {
                info!(
                    "   ✅ Unrealized P&L = ${:.2} (from {} bought tokens)",
                    unrealized_pnl,
                    position.bought_quantity
                );
            }

            unrealized_pnl
        } else {
            Decimal::ZERO
        }
    }

    /// Calculate cost basis from real (non-phantom) buy events only
    /// This provides the real cost basis for tokens, excluding phantom buys
    #[allow(dead_code)]
    fn calculate_real_cost_basis(
        &self,
        matched_trades: &[MatchedTrade],
        remaining_buys: &[NewFinancialEvent],
    ) -> (Decimal, Decimal) {
        // Calculate cost basis from matched trades (excluding phantom buy matches)
        let mut total_real_cost = Decimal::ZERO;
        let mut total_real_quantity = Decimal::ZERO;

        for trade in matched_trades {
            // Skip phantom buy matches (phantom buys have timestamps 1 second before sell)
            let time_diff = trade.sell_event.timestamp - trade.buy_event.timestamp;
            if time_diff == Duration::seconds(1) {
                debug!(
                    "Skipping phantom buy match for cost basis: {} {}",
                    trade.matched_quantity, trade.buy_event.token_symbol
                );
                continue;
            }

            // Include real buy events in cost basis calculation
            total_real_cost += trade.matched_quantity * trade.buy_event.usd_price_per_token;
            total_real_quantity += trade.matched_quantity;
        }

        // Add remaining (unmatched) real buys to cost basis
        for buy_event in remaining_buys {
            total_real_cost += buy_event.usd_value;
            total_real_quantity += buy_event.quantity;
        }

        let avg_cost_basis = if total_real_quantity > Decimal::ZERO {
            total_real_cost / total_real_quantity
        } else {
            Decimal::ZERO
        };

        debug!(
            "Real cost basis calculation: {} quantity @ ${} avg cost (total cost: ${})",
            total_real_quantity, avg_cost_basis, total_real_cost
        );

        (avg_cost_basis, total_real_quantity)
    }

    /// Calculate hold time statistics from matched trades
    fn calculate_hold_time_stats(
        &self,
        matched_trades: &[MatchedTrade],
    ) -> (Decimal, Decimal, Decimal) {
        if matched_trades.is_empty() {
            return (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO);
        }

        let hold_times_minutes: Vec<Decimal> = matched_trades
            .iter()
            .map(|t| Decimal::from(t.hold_time_seconds) / Decimal::from(60))
            .collect();

        let avg_hold_time =
            hold_times_minutes.iter().sum::<Decimal>() / Decimal::from(hold_times_minutes.len());
        let min_hold_time = hold_times_minutes
            .iter()
            .cloned()
            .min()
            .unwrap_or(Decimal::ZERO);
        let max_hold_time = hold_times_minutes
            .iter()
            .cloned()
            .max()
            .unwrap_or(Decimal::ZERO);

        (avg_hold_time, min_hold_time, max_hold_time)
    }

    /// Calculate winning and losing streak analytics from matched trades
    /// Returns (current_winning_streak, longest_winning_streak, current_losing_streak, longest_losing_streak)
    fn calculate_streak_analytics(&self, matched_trades: &[MatchedTrade]) -> (u32, u32, u32, u32) {
        if matched_trades.is_empty() {
            return (0, 0, 0, 0);
        }

        let mut current_winning_streak = 0u32;
        let mut longest_winning_streak = 0u32;
        let mut current_losing_streak = 0u32;
        let mut longest_losing_streak = 0u32;

        // Sort trades by sell timestamp to get chronological order
        let mut sorted_trades = matched_trades.to_vec();
        sorted_trades.sort_by(|a, b| a.sell_event.timestamp.cmp(&b.sell_event.timestamp));

        for trade in sorted_trades.iter() {
            if trade.realized_pnl_usd > Decimal::ZERO {
                // Winning trade
                current_winning_streak += 1;
                current_losing_streak = 0; // Reset losing streak

                if current_winning_streak > longest_winning_streak {
                    longest_winning_streak = current_winning_streak;
                }
            } else if trade.realized_pnl_usd < Decimal::ZERO {
                // Losing trade
                current_losing_streak += 1;
                current_winning_streak = 0; // Reset winning streak

                if current_losing_streak > longest_losing_streak {
                    longest_losing_streak = current_losing_streak;
                }
            }
            // If P&L is exactly zero (e.g., phantom buys), don't affect streaks
        }

        debug!(
            "Streak analytics: Current Win: {}, Longest Win: {}, Current Loss: {}, Longest Loss: {}",
            current_winning_streak, longest_winning_streak, current_losing_streak, longest_losing_streak
        );

        (
            current_winning_streak,
            longest_winning_streak,
            current_losing_streak,
            longest_losing_streak,
        )
    }

    /// Calculate portfolio-level streaks across all tokens chronologically
    fn calculate_portfolio_streaks(
        &self,
        token_results: &[TokenPnLResult],
    ) -> (u32, u32, u32, u32) {
        // Collect all matched trades from all tokens
        let mut all_trades: Vec<&MatchedTrade> = Vec::new();
        for token_result in token_results {
            for trade in &token_result.matched_trades {
                all_trades.push(trade);
            }
        }

        if all_trades.is_empty() {
            return (0, 0, 0, 0);
        }

        // Sort all trades by sell timestamp to get chronological order across all tokens
        all_trades.sort_by(|a, b| a.sell_event.timestamp.cmp(&b.sell_event.timestamp));

        let mut current_winning_streak = 0u32;
        let mut longest_winning_streak = 0u32;
        let mut current_losing_streak = 0u32;
        let mut longest_losing_streak = 0u32;

        for trade in all_trades {
            if trade.realized_pnl_usd > Decimal::ZERO {
                // Winning trade
                current_winning_streak += 1;
                current_losing_streak = 0; // Reset losing streak

                if current_winning_streak > longest_winning_streak {
                    longest_winning_streak = current_winning_streak;
                }
            } else if trade.realized_pnl_usd < Decimal::ZERO {
                // Losing trade
                current_losing_streak += 1;
                current_winning_streak = 0; // Reset winning streak

                if current_losing_streak > longest_losing_streak {
                    longest_losing_streak = current_losing_streak;
                }
            }
            // If P&L is exactly zero (e.g., phantom buys), don't affect streaks
        }

        (
            current_winning_streak,
            longest_winning_streak,
            current_losing_streak,
            longest_losing_streak,
        )
    }

    /// Calculate the number of distinct active trading days from all matched trades
    fn calculate_active_days_count(&self, token_results: &[TokenPnLResult]) -> u32 {
        use std::collections::HashSet;

        let mut trading_dates = HashSet::new();

        // Collect all unique trading dates (based on sell event timestamps)
        for token_result in token_results {
            for trade in &token_result.matched_trades {
                // Use the sell event timestamp for the trading date
                let trade_date = trade.sell_event.timestamp.date_naive();
                trading_dates.insert(trade_date);
            }
        }

        trading_dates.len() as u32
    }
}

