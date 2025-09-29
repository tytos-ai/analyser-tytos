use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info, trace, warn};

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
    pub bought_quantity: Decimal,

    /// Remaining quantity from received tokens
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
        // Check if this is an exchange currency based on trading patterns:
        // 1. All trades have very short hold times (1-2 seconds = phantom trades)
        // 2. All trades have $0 P&L (phantom buy-sell pairs)

        let is_phantom_pattern = token_result.avg_hold_time_minutes < Decimal::new(1, 1) && // 0.1 minutes = 6 seconds avg
            token_result.total_realized_pnl_usd.abs() < Decimal::new(1, 2) && // 0.01 = ~$0 P&L
            token_result.total_trades > 0;

        // Also check for known exchange currency addresses across chains
        let is_known_exchange_currency = matches!(
            token_result.token_address.as_str(),
            // Solana
            // "So11111111111111111111111111111111111111112" | // SOL - Removed to align with Universal Token Treatment principle
            "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB" | // USDT on Solana
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v" | // USDC on Solana

            // Ethereum
            "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2" | // WETH
            "0xdAC17F958D2ee523a2206206994597C13D831ec7" | // USDT on Ethereum
            "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48" | // USDC on Ethereum

            // Binance Smart Chain
            "0xbb4CdB9CBd36B01bD1cBaEBF2De08d9173bc095c" | // WBNB
            "0x55d398326f99059fF775485246999027B3197955" | // USDT on BSC
            "0x8AC76a51cc950d9822D68b83fE1Ad97B32Cd580d" | // USDC on BSC

            // Base
            "0x4200000000000000000000000000000000000006" | // WETH on Base
            "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913" // USDC on Base
        );

        is_phantom_pattern || is_known_exchange_currency
    }

    /// Enable balance fetching by setting the balance fetcher
    pub fn set_balance_fetcher(&mut self, balance_fetcher: ZerionBalanceFetcher) {
        self.balance_fetcher = Some(balance_fetcher);
    }

    /// Calculate portfolio P&L from financial events
    /// This is the main entry point for P&L calculation
    pub async fn calculate_portfolio_pnl(
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
        let mut total_receive_consumptions = 0u32;

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

            match self.calculate_token_pnl(events, current_price).await {
                Ok(token_result) => {
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

        // Calculate portfolio investment metrics (exclude exchange currencies to avoid double counting)
        let total_invested_usd: Decimal = token_results
            .iter()
            .filter(|t| !Self::is_exchange_currency_token(t))
            .map(|t| t.total_invested_usd)
            .sum();

        let total_returned_usd: Decimal = token_results
            .iter()
            .filter(|t| !Self::is_exchange_currency_token(t))
            .map(|t| t.total_returned_usd)
            .sum();

        // Calculate portfolio-level streaks (continue across all tokens chronologically)
        let (
            current_winning_streak,
            longest_winning_streak,
            current_losing_streak,
            longest_losing_streak,
        ) = self.calculate_portfolio_streaks(&token_results);

        // Calculate profit percentage
        let profit_percentage = if total_invested_usd > Decimal::ZERO {
            ((total_pnl / total_invested_usd) * Decimal::from(100)).round_dp(2)
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
    pub async fn calculate_token_pnl(
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
        debug!(
            "Event summary for {}: {} buys, {} sells, {} receives",
            token_symbol,
            buy_events.len(),
            sell_events.len(),
            receive_events.len()
        );

        if buy_events.is_empty() && receive_events.is_empty() && !sell_events.is_empty() {
            warn!(
                "⚠️  Token {} ({}): No buy or receive events found but {} sell events exist. This will result in phantom buys with zero P&L.",
                token_symbol,
                token_address,
                sell_events.len()
            );
        }

        // Calculate investment metrics (exclude phantom buys from total_invested!)
        let total_invested_usd: Decimal = events
            .iter()
            .filter(|e| e.event_type == NewEventType::Buy)
            .filter(|e| !e.transaction_hash.starts_with("phantom_buy_")) // Exclude phantom buys
            .map(|e| e.usd_value)
            .sum();

        let total_returned_usd: Decimal = sell_events
            .iter()
            .filter(|e| {
                // Exclude sells that are part of phantom buy pairs (exchange currency swaps)
                // These are sells of exchange currencies (SOL, ETH, etc) to buy target tokens
                !events.iter().any(|buy| {
                    buy.transaction_hash.starts_with("phantom_buy_")
                        && buy.transaction_hash.contains(&e.transaction_hash)
                })
            })
            .map(|e| e.usd_value)
            .sum();

        // Perform enhanced FIFO matching that handles received tokens separately
        let (matched_trades, receive_consumptions) = self.perform_enhanced_fifo_matching(&mut buy_events, &sell_events, &receive_events)?;

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

        // Calculate unrealized P&L using real balances (fixed phantom buy bug)
        let total_unrealized_pnl = self
            .calculate_unrealized_pnl_with_real_balance(
                &token_address,
                &token_symbol,
                &matched_trades,
                &buy_events,
                current_price,
            )
            .await;

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
        let total_received_quantity: Decimal = receive_events.iter().map(|e| e.quantity).sum();
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

    /// Perform FIFO matching between buy and sell events
    /// Creates phantom buys for any sells that can't be matched against existing buys
    /// Returns all matched trades (including phantom buy matches)
    fn perform_fifo_matching(
        &self,
        buy_events: &mut Vec<NewFinancialEvent>,
        sell_events: &[NewFinancialEvent],
    ) -> Result<Vec<MatchedTrade>, String> {
        let mut matched_trades = Vec::new();

        // Create working copies of buy events to track remaining quantities
        let mut buy_queue: Vec<(NewFinancialEvent, Decimal)> =
            buy_events.iter().map(|e| (e.clone(), e.quantity)).collect();

        for sell_event in sell_events {
            let mut remaining_sell_quantity = sell_event.quantity;

            trace!(
                "Processing sell: {} {} @ ${}",
                sell_event.quantity,
                sell_event.token_symbol,
                sell_event.usd_price_per_token
            );

            // Match against oldest available buys (FIFO)
            // Only match against buys that occurred before this sell
            while remaining_sell_quantity > Decimal::ZERO && !buy_queue.is_empty() {
                let buy_index = buy_queue.iter().position(|(buy_event, remaining_qty)| {
                    *remaining_qty > Decimal::ZERO && buy_event.timestamp <= sell_event.timestamp
                });

                if let Some(idx) = buy_index {
                    let (buy_event, available_buy_quantity) = &mut buy_queue[idx];

                    // Determine how much we can match
                    let matched_quantity = remaining_sell_quantity.min(*available_buy_quantity);

                    // Calculate P&L for this matched portion
                    let realized_pnl = (sell_event.usd_price_per_token
                        - buy_event.usd_price_per_token)
                        * matched_quantity;

                    // Calculate hold time
                    let hold_time_seconds =
                        sell_event.timestamp.timestamp() - buy_event.timestamp.timestamp();

                    let matched_trade = MatchedTrade {
                        buy_event: buy_event.clone(),
                        sell_event: sell_event.clone(),
                        matched_quantity,
                        realized_pnl_usd: realized_pnl,
                        hold_time_seconds,
                    };

                    matched_trades.push(matched_trade);

                    // Update remaining quantities
                    *available_buy_quantity -= matched_quantity;
                    remaining_sell_quantity -= matched_quantity;

                    trace!(
                        "Matched {} tokens: Buy @ ${} -> Sell @ ${}, P&L: ${}, Hold: {}s",
                        matched_quantity,
                        buy_event.usd_price_per_token,
                        sell_event.usd_price_per_token,
                        realized_pnl,
                        hold_time_seconds
                    );
                } else {
                    // No more buys available
                    break;
                }
            }

            // Handle any unmatched sell quantity by creating chronological phantom buy
            if remaining_sell_quantity > Decimal::ZERO {
                // Create phantom buy event chronologically before the sell
                let phantom_buy_timestamp = sell_event.timestamp - Duration::seconds(1);
                let phantom_buy_usd_value =
                    remaining_sell_quantity * sell_event.usd_price_per_token;

                let phantom_buy_event = NewFinancialEvent {
                    wallet_address: sell_event.wallet_address.clone(),
                    token_address: sell_event.token_address.clone(),
                    token_symbol: sell_event.token_symbol.clone(),
                    chain_id: sell_event.chain_id.clone(),
                    event_type: NewEventType::Buy,
                    quantity: remaining_sell_quantity,
                    usd_price_per_token: sell_event.usd_price_per_token, // Same price = zero P&L
                    usd_value: phantom_buy_usd_value,
                    timestamp: phantom_buy_timestamp,
                    transaction_hash: format!("phantom_buy_{}", sell_event.transaction_hash),
                };

                // Calculate hold time (will be 1 second)
                let hold_time_seconds =
                    sell_event.timestamp.timestamp() - phantom_buy_timestamp.timestamp();

                // Create matched trade with zero P&L
                let matched_trade = MatchedTrade {
                    buy_event: phantom_buy_event.clone(),
                    sell_event: sell_event.clone(),
                    matched_quantity: remaining_sell_quantity,
                    realized_pnl_usd: Decimal::ZERO, // Same buy/sell price = zero P&L
                    hold_time_seconds,
                };

                matched_trades.push(matched_trade);

                debug!(
                    "Created phantom buy: {} {} @ ${} (1 second before sell, zero P&L)",
                    remaining_sell_quantity,
                    sell_event.token_symbol,
                    sell_event.usd_price_per_token
                );
            }
        }

        // Update the original buy_events to remove consumed quantities
        buy_events.clear();
        for (buy_event, remaining_qty) in buy_queue {
            if remaining_qty > Decimal::ZERO {
                let mut updated_buy = buy_event;
                updated_buy.quantity = remaining_qty;
                // Update USD value proportionally to maintain correct cost basis
                updated_buy.usd_value = remaining_qty * updated_buy.usd_price_per_token;
                buy_events.push(updated_buy);
            }
        }

        debug!(
            "FIFO matching completed: {} matched trades (including phantom buy matches)",
            matched_trades.len()
        );

        Ok(matched_trades)
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

        // Process each sell event
        for sell_event in sell_events {
            let mut remaining_sell_quantity = sell_event.quantity;

            // Phase 1: First consume received tokens (FIFO within receives)
            for received_token in &mut remaining_receives {
                if remaining_sell_quantity <= Decimal::ZERO {
                    break;
                }

                if received_token.remaining_quantity > Decimal::ZERO {
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

                    debug!(
                        "Consumed {} {} from receive event for sell (no P&L impact)",
                        consumed_quantity, sell_event.token_symbol
                    );
                }
            }

            // Phase 2: If there's still sell quantity remaining, match with bought tokens
            if remaining_sell_quantity > Decimal::ZERO {
                // Create a partial sell event for the remaining quantity
                let mut remaining_sell_event = sell_event.clone();
                remaining_sell_event.quantity = remaining_sell_quantity;
                remaining_sell_event.usd_value = remaining_sell_quantity * sell_event.usd_price_per_token;

                // Match against bought tokens using traditional FIFO
                for buy_event in buy_events.iter_mut() {
                    if remaining_sell_event.quantity <= Decimal::ZERO {
                        break;
                    }

                    if buy_event.quantity > Decimal::ZERO && buy_event.timestamp <= remaining_sell_event.timestamp {
                        let matched_quantity = remaining_sell_event.quantity.min(buy_event.quantity);

                        let realized_pnl = (remaining_sell_event.usd_price_per_token - buy_event.usd_price_per_token) * matched_quantity;

                        let hold_time_seconds = (remaining_sell_event.timestamp - buy_event.timestamp)
                            .num_seconds()
                            .max(0);

                        matched_trades.push(MatchedTrade {
                            buy_event: buy_event.clone(),
                            sell_event: remaining_sell_event.clone(),
                            matched_quantity,
                            realized_pnl_usd: realized_pnl,
                            hold_time_seconds,
                        });

                        // Update remaining quantities
                        buy_event.quantity -= matched_quantity;
                        remaining_sell_event.quantity -= matched_quantity;
                        remaining_sell_event.usd_value -= matched_quantity * remaining_sell_event.usd_price_per_token;

                        debug!(
                            "Matched {} {} (buy: ${:.4} -> sell: ${:.4}, P&L: ${:.2})",
                            matched_quantity,
                            remaining_sell_event.token_symbol,
                            buy_event.usd_price_per_token,
                            remaining_sell_event.usd_price_per_token,
                            realized_pnl
                        );
                    }
                }

                // If there's still unmatched sell quantity, create phantom buy
                if remaining_sell_event.quantity > Decimal::ZERO {
                    let phantom_buy = NewFinancialEvent {
                        wallet_address: remaining_sell_event.wallet_address.clone(),
                        transaction_hash: format!("phantom_buy_{}", remaining_sell_event.transaction_hash),
                        timestamp: remaining_sell_event.timestamp - chrono::Duration::seconds(1),
                        event_type: NewEventType::Buy,
                        token_address: remaining_sell_event.token_address.clone(),
                        token_symbol: remaining_sell_event.token_symbol.clone(),
                        quantity: remaining_sell_event.quantity,
                        usd_price_per_token: remaining_sell_event.usd_price_per_token,
                        usd_value: remaining_sell_event.usd_value,
                        chain_id: remaining_sell_event.chain_id.clone(),
                    };

                    matched_trades.push(MatchedTrade {
                        buy_event: phantom_buy,
                        sell_event: remaining_sell_event.clone(),
                        matched_quantity: remaining_sell_event.quantity,
                        realized_pnl_usd: Decimal::ZERO,
                        hold_time_seconds: 1,
                    });

                    debug!(
                        "Created phantom buy for unmatched sell of {} {} (zero P&L)",
                        remaining_sell_event.quantity, remaining_sell_event.token_symbol
                    );
                }
            }
        }

        debug!(
            "Enhanced FIFO matching completed: {} matched trades, {} receive consumptions",
            matched_trades.len(),
            receive_consumptions.len()
        );

        Ok((matched_trades, receive_consumptions))
    }

    /// Calculate remaining position from unmatched buy events (LEGACY - includes phantom buys)
    /// This method is kept for backward compatibility with the legacy unrealized P&L calculation
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
        // Calculate remaining bought tokens
        let bought_quantity: Decimal = remaining_buys.iter().map(|e| e.quantity).sum();
        let total_bought_cost: Decimal = remaining_buys.iter().map(|e| e.usd_value).sum();
        let avg_cost_basis = if bought_quantity > Decimal::ZERO {
            total_bought_cost / bought_quantity
        } else {
            Decimal::ZERO
        };

        // Calculate remaining received tokens
        let total_received_quantity: Decimal = receive_events.iter().map(|e| e.quantity).sum();
        let consumed_received_quantity: Decimal = receive_consumptions.iter().map(|c| c.consumed_quantity).sum();
        let received_quantity = total_received_quantity - consumed_received_quantity;

        // Only create position if there are remaining tokens
        if bought_quantity <= Decimal::ZERO && received_quantity <= Decimal::ZERO {
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

        debug!(
            "Enhanced remaining position: {} {} bought + {} {} received @ avg cost ${} (total cost: ${})",
            position.bought_quantity,
            position.token_symbol,
            position.received_quantity,
            position.token_symbol,
            position.avg_cost_basis_usd,
            position.total_cost_basis_usd
        );

        Ok(Some(position))
    }

    /// Calculate unrealized P&L for remaining positions
    /// Following documentation specification: (current_price - weighted_avg_cost_basis) × remaining_quantity
    fn calculate_unrealized_pnl(
        &self,
        remaining_position: &Option<RemainingPosition>,
        current_price: Option<Decimal>,
    ) -> Decimal {
        if let (Some(position), Some(price)) = (remaining_position, current_price) {
            // Treat zero or negative prices as missing price data
            if price <= Decimal::ZERO {
                debug!(
                    "Zero/negative price for {}: treating as missing price data, unrealized P&L = 0",
                    position.token_symbol
                );
                return Decimal::ZERO;
            }

            // Use the exact formula specified in documentation, but only for bought tokens:
            // (current_price - weighted_avg_cost_basis) × remaining_bought_quantity
            // Received tokens have no cost basis, so no unrealized P&L
            let unrealized_pnl = (price - position.avg_cost_basis_usd) * position.bought_quantity;

            // Sanity check for unrealistic values (> $100M)
            let hundred_million = Decimal::from(100_000_000);
            if unrealized_pnl.abs() > hundred_million {
                warn!(
                    "Unrealistic unrealized P&L detected for {}: {} bought @ ${} vs cost basis ${} = P&L: ${} - treating as data error",
                    position.token_symbol,
                    position.bought_quantity,
                    price,
                    position.avg_cost_basis_usd,
                    unrealized_pnl
                );
                return Decimal::ZERO;
            }

            debug!(
                "Unrealized P&L for {}: {} bought + {} received @ ${} vs cost basis ${} = P&L: ${} (only bought tokens generate P&L)",
                position.token_symbol,
                position.bought_quantity,
                position.received_quantity,
                price,
                position.avg_cost_basis_usd,
                unrealized_pnl
            );

            unrealized_pnl
        } else {
            Decimal::ZERO
        }
    }

    /// Calculate cost basis from real (non-phantom) buy events only
    /// This provides the real cost basis for tokens, excluding phantom buys
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

    /// Calculate unrealized P&L using real wallet balances instead of calculated positions
    /// This is the new method that fixes the phantom buy unrealized P&L bug
    async fn calculate_unrealized_pnl_with_real_balance(
        &self,
        token_address: &str,
        token_symbol: &str,
        matched_trades: &[MatchedTrade],
        remaining_buys: &[NewFinancialEvent],
        current_price: Option<Decimal>,
    ) -> Decimal {
        // Get real balance from Zerion API if available, otherwise fall back to calculated positions
        let real_balance = if let Some(balance_fetcher) = &self.balance_fetcher {
            // Try Zerion API
            match balance_fetcher
                .get_token_ui_amount(&self.wallet_address, token_address)
                .await
            {
                Ok(balance) => balance,
                Err(e) => {
                    warn!(
                        "Failed to fetch real balance from Zerion for {}: {}, falling back to calculated position",
                        token_symbol, e
                    );
                    let remaining_position = self
                        .calculate_remaining_position(remaining_buys, token_address, token_symbol)
                        .unwrap_or(None);
                    return self.calculate_unrealized_pnl(&remaining_position, current_price);
                }
            }
        } else {
            // No balance fetcher available, use calculated positions
            debug!("No balance fetcher available, using legacy position calculation");
            let remaining_position = self
                .calculate_remaining_position(remaining_buys, token_address, token_symbol)
                .unwrap_or(None);
            return self.calculate_unrealized_pnl(&remaining_position, current_price);
        };

        debug!("Real wallet balance for {}: {}", token_symbol, real_balance);

        // If real balance is zero or negligible, unrealized P&L is zero
        if real_balance <= Decimal::new(1, 6) {
            // 0.000001
            debug!(
                "Negligible real balance for {}, unrealized P&L = 0",
                token_symbol
            );
            return Decimal::ZERO;
        }

        // Calculate cost basis using only real (non-phantom) buys
        let (avg_cost_basis, _calculated_quantity) =
            self.calculate_real_cost_basis(matched_trades, remaining_buys);

        // If we have no cost basis (no real buys), unrealized P&L is zero
        if avg_cost_basis <= Decimal::ZERO {
            debug!("No cost basis for {}, unrealized P&L = 0", token_symbol);
            return Decimal::ZERO;
        }

        // Calculate unrealized P&L using real balance and real cost basis
        let current_price = match current_price {
            Some(price) if price > Decimal::ZERO => price,
            _ => {
                debug!(
                    "No valid current price for {}, unrealized P&L = 0",
                    token_symbol
                );
                return Decimal::ZERO;
            }
        };

        let unrealized_pnl = (current_price - avg_cost_basis) * real_balance;

        // Sanity check for unrealistic values
        let hundred_million = Decimal::from(100_000_000);
        if unrealized_pnl.abs() > hundred_million {
            warn!(
                "Unrealistic unrealized P&L detected for {} using real balance: {} @ ${} vs cost basis ${} = P&L: ${} - treating as data error",
                token_symbol,
                real_balance,
                current_price,
                avg_cost_basis,
                unrealized_pnl
            );
            return Decimal::ZERO;
        }

        debug!(
            "Unrealized P&L with real balance for {}: {} @ ${} vs cost basis ${} = P&L: ${}",
            token_symbol, real_balance, current_price, avg_cost_basis, unrealized_pnl
        );

        unrealized_pnl
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

