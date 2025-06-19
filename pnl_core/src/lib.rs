pub mod timeframe;
pub mod partial_fifo;
pub mod trader_filter;
pub mod fifo_pnl_engine;

// Re-export key trader filtering types
pub use trader_filter::{TraderFilter, TraderQuality, RiskLevel, TradingStyle, generate_trader_summary};
pub use fifo_pnl_engine::FifoPnLEngine;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use tracing::{debug, info, warn};
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum PnLError {
    #[error("Price fetching error: {0}")]
    PriceFetch(String),
    #[error("Invalid financial event: {0}")]
    InvalidEvent(String),
    #[error("Calculation error: {0}")]
    Calculation(String),
    #[error("Configuration error: {0}")]
    Configuration(String),
    #[error("Timeframe parsing error: {0}")]
    TimeframeParse(String),
}

pub type Result<T> = std::result::Result<T, PnLError>;

/// Core data structure representing a financial event from a parsed transaction
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FinancialEvent {
    /// Unique identifier for the event
    pub id: Uuid,
    
    /// Transaction signature/hash
    pub transaction_id: String,
    
    /// Wallet address that performed this action
    pub wallet_address: String,
    
    /// Type of financial event
    pub event_type: EventType,
    
    /// Token mint address
    pub token_mint: String,
    
    /// Amount of tokens involved
    pub token_amount: Decimal,
    
    /// SOL amount (for fees, or if SOL is the token)
    pub sol_amount: Decimal,
    
    /// Timestamp of the event
    pub timestamp: DateTime<Utc>,
    
    /// Transaction fees paid in SOL
    pub transaction_fee: Decimal,
    
    /// Additional metadata
    pub metadata: EventMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EventType {
    /// Token purchase (swap SOL/other token for target token)
    Buy,
    /// Token sale (swap target token for SOL/other token) 
    Sell,
    /// Token transfer in (received tokens)
    TransferIn,
    /// Token transfer out (sent tokens)
    TransferOut,
    /// Transaction fee payment
    Fee,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventMetadata {
    /// Program that executed this transaction
    pub program_id: Option<String>,
    
    /// Instruction index within the transaction
    pub instruction_index: Option<u32>,
    
    /// Exchange/DEX used (if applicable)
    pub exchange: Option<String>,
    
    /// Price per token at time of transaction (if available)
    pub price_per_token: Option<Decimal>,
    
    /// Additional custom fields
    pub extra: HashMap<String, String>,
}

impl Default for EventMetadata {
    fn default() -> Self {
        Self {
            program_id: None,
            instruction_index: None,
            exchange: None,
            price_per_token: None,
            extra: HashMap::new(),
        }
    }
}

/// P&L calculation result for a wallet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PnLReport {
    /// Wallet address analyzed
    pub wallet_address: String,
    
    /// Analysis timeframe
    pub timeframe: AnalysisTimeframe,
    
    /// Overall P&L summary
    pub summary: PnLSummary,
    
    /// Per-token P&L breakdown
    pub token_breakdown: Vec<TokenPnL>,
    
    /// Current holdings (tokens still held)
    pub current_holdings: Vec<Holding>,
    
    /// Analysis metadata
    pub metadata: ReportMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisTimeframe {
    /// Start time of analysis (None = from beginning)
    pub start_time: Option<DateTime<Utc>>,
    
    /// End time of analysis (None = until now)
    pub end_time: Option<DateTime<Utc>>,
    
    /// Timeframe mode used
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PnLSummary {
    /// Total realized profit/loss in USD
    pub realized_pnl_usd: Decimal,
    
    /// Total unrealized profit/loss in USD (from current holdings)
    pub unrealized_pnl_usd: Decimal,
    
    /// Total P&L (realized + unrealized) in USD
    pub total_pnl_usd: Decimal,
    
    /// Total fees paid in SOL
    pub total_fees_sol: Decimal,
    
    /// Total fees paid in USD equivalent
    pub total_fees_usd: Decimal,
    
    /// Number of profitable trades
    pub winning_trades: u32,
    
    /// Number of losing trades
    pub losing_trades: u32,
    
    /// Total number of trades
    pub total_trades: u32,
    
    /// Win rate percentage
    pub win_rate: Decimal,
    
    /// Average hold time in minutes
    pub avg_hold_time_minutes: Decimal,
    
    /// Total capital deployed (max SOL value at any point)
    pub total_capital_deployed_sol: Decimal,
    
    /// ROI percentage
    pub roi_percentage: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPnL {
    /// Token mint address
    pub token_mint: String,
    
    /// Token symbol (if known)
    pub token_symbol: Option<String>,
    
    /// Realized P&L for this token in USD
    pub realized_pnl_usd: Decimal,
    
    /// Unrealized P&L for this token in USD
    pub unrealized_pnl_usd: Decimal,
    
    /// Total P&L for this token
    pub total_pnl_usd: Decimal,
    
    /// Number of buy transactions
    pub buy_count: u32,
    
    /// Number of sell transactions
    pub sell_count: u32,
    
    /// Total tokens bought
    pub total_bought: Decimal,
    
    /// Total tokens sold
    pub total_sold: Decimal,
    
    /// Average buy price in USD
    pub avg_buy_price_usd: Decimal,
    
    /// Average sell price in USD
    pub avg_sell_price_usd: Decimal,
    
    /// First buy timestamp
    pub first_buy_time: Option<DateTime<Utc>>,
    
    /// Last sell timestamp
    pub last_sell_time: Option<DateTime<Utc>>,
    
    /// Hold time for this token in minutes
    pub hold_time_minutes: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Holding {
    /// Token mint address
    pub token_mint: String,
    
    /// Token symbol (if known)
    pub token_symbol: Option<String>,
    
    /// Amount currently held
    pub amount: Decimal,
    
    /// Average cost basis in USD per token
    pub avg_cost_basis_usd: Decimal,
    
    /// Current price in USD per token
    pub current_price_usd: Decimal,
    
    /// Total cost basis in USD
    pub total_cost_basis_usd: Decimal,
    
    /// Current value in USD
    pub current_value_usd: Decimal,
    
    /// Unrealized P&L in USD
    pub unrealized_pnl_usd: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportMetadata {
    /// When this report was generated
    pub generated_at: DateTime<Utc>,
    
    /// Total events processed
    pub events_processed: u32,
    
    /// Events filtered out
    pub events_filtered: u32,
    
    /// Analysis duration in seconds
    pub analysis_duration_seconds: f64,
    
    /// Filters applied
    pub filters_applied: PnLFilters,
    
    /// Any warnings or issues during analysis
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PnLFilters {
    /// Minimum wallet capital in SOL
    pub min_capital_sol: Decimal,
    
    /// Minimum hold time in minutes
    pub min_hold_minutes: Decimal,
    
    /// Minimum number of trades
    pub min_trades: u32,
    
    /// Minimum win rate percentage
    pub min_win_rate: Decimal,
    
    /// Maximum signatures processed
    pub max_signatures: Option<u32>,
    
    /// Timeframe filter
    pub timeframe_filter: Option<AnalysisTimeframe>,
}

/// Trait for fetching token prices
#[async_trait]
pub trait PriceFetcher: Send + Sync {
    /// Fetch current prices for multiple tokens
    async fn fetch_prices(
        &self,
        token_mints: &[String],
        vs_token: Option<&str>,
    ) -> Result<HashMap<String, Decimal>>;
    
    /// Fetch historical price for a token at a specific time
    async fn fetch_historical_price(
        &self,
        token_mint: &str,
        timestamp: DateTime<Utc>,
        vs_token: Option<&str>,
    ) -> Result<Option<Decimal>>;
}

/// Main P&L calculation engine
pub struct PnLEngine<P: PriceFetcher> {
    price_fetcher: P,
}

impl<P: PriceFetcher> PnLEngine<P> {
    pub fn new(price_fetcher: P) -> Self {
        Self { price_fetcher }
    }
    
    /// Calculate P&L for a wallet given its financial events
    pub async fn calculate_pnl(
        &self,
        wallet_address: &str,
        events: Vec<FinancialEvent>,
        filters: PnLFilters,
    ) -> Result<PnLReport> {
        let start_time = std::time::Instant::now();
        
        info!("Starting P&L calculation for wallet: {}", wallet_address);
        debug!("Processing {} events with filters: {:?}", events.len(), filters);
        
        // Filter events based on criteria
        let events_len = events.len();
        let filtered_events = self.filter_events(events, &filters)?;
        
        debug!("After filtering: {} events remain", filtered_events.len());
        
        // Early validation checks
        if filtered_events.len() < filters.min_trades as usize {
            return Err(PnLError::Configuration(format!(
                "Wallet has {} trades, minimum required: {}",
                filtered_events.len(),
                filters.min_trades
            )));
        }
        
        // Group events by token
        let events_by_token = self.group_events_by_token(&filtered_events);
        
        // Calculate per-token P&L
        let mut token_pnl_results = Vec::new();
        let mut current_holdings = Vec::new();
        let mut warnings = Vec::new();
        
        for (token_mint, token_events) in events_by_token {
            match self.calculate_token_pnl(&token_mint, token_events).await {
                Ok((token_pnl, holding)) => {
                    token_pnl_results.push(token_pnl);
                    if let Some(h) = holding {
                        current_holdings.push(h);
                    }
                }
                Err(e) => {
                    let warning = format!("Failed to calculate P&L for token {}: {}", token_mint, e);
                    warn!("{}", warning);
                    warnings.push(warning);
                }
            }
        }
        
        // Calculate overall summary
        let summary = self.calculate_summary(&token_pnl_results, &filtered_events, &filters).await?;
        
        // Apply final filters
        if summary.total_capital_deployed_sol < filters.min_capital_sol {
            return Err(PnLError::Configuration(format!(
                "Wallet capital {} SOL below minimum: {} SOL",
                summary.total_capital_deployed_sol,
                filters.min_capital_sol
            )));
        }
        
        if summary.win_rate < filters.min_win_rate {
            return Err(PnLError::Configuration(format!(
                "Wallet win rate {}% below minimum: {}%",
                summary.win_rate,
                filters.min_win_rate
            )));
        }
        
        if summary.avg_hold_time_minutes < filters.min_hold_minutes {
            return Err(PnLError::Configuration(format!(
                "Average hold time {} minutes below minimum: {} minutes",
                summary.avg_hold_time_minutes,
                filters.min_hold_minutes
            )));
        }
        
        let analysis_duration = start_time.elapsed().as_secs_f64();
        
        let timeframe = filters.timeframe_filter.clone().unwrap_or(AnalysisTimeframe {
            start_time: None,
            end_time: None,
            mode: "none".to_string(),
        });
        
        let report = PnLReport {
            wallet_address: wallet_address.to_string(),
            timeframe,
            summary,
            token_breakdown: token_pnl_results,
            current_holdings,
            metadata: ReportMetadata {
                generated_at: Utc::now(),
                events_processed: filtered_events.len() as u32,
                events_filtered: (events_len - filtered_events.len()) as u32,
                analysis_duration_seconds: analysis_duration,
                filters_applied: filters.clone(),
                warnings,
            },
        };
        
        info!(
            "P&L calculation completed for wallet {} in {:.2}s",
            wallet_address, analysis_duration
        );
        
        Ok(report)
    }
    
    /// Filter events based on timeframe and other criteria
    fn filter_events(&self, events: Vec<FinancialEvent>, filters: &PnLFilters) -> Result<Vec<FinancialEvent>> {
        let mut filtered = events;
        
        // Apply timeframe filter
        if let Some(ref timeframe) = filters.timeframe_filter {
            filtered.retain(|event| {
                let in_start_range = timeframe.start_time
                    .map(|start| event.timestamp >= start)
                    .unwrap_or(true);
                
                let in_end_range = timeframe.end_time
                    .map(|end| event.timestamp <= end)
                    .unwrap_or(true);
                
                in_start_range && in_end_range
            });
        }
        
        // Apply max signatures filter
        if let Some(max_sigs) = filters.max_signatures {
            // Group by transaction ID and take only the first max_sigs transactions
            let mut transactions: Vec<_> = filtered
                .iter()
                .map(|e| e.transaction_id.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            
            transactions.sort();
            transactions.truncate(max_sigs as usize);
            
            let allowed_txs: std::collections::HashSet<_> = transactions.into_iter().collect();
            filtered.retain(|event| allowed_txs.contains(&event.transaction_id));
        }
        
        // Sort by timestamp
        filtered.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        
        Ok(filtered)
    }
    
    /// Group events by token mint
    fn group_events_by_token(&self, events: &[FinancialEvent]) -> HashMap<String, Vec<FinancialEvent>> {
        let mut groups = HashMap::new();
        
        for event in events {
            groups
                .entry(event.token_mint.clone())
                .or_insert_with(Vec::new)
                .push(event.clone());
        }
        
        groups
    }
    
    /// Calculate P&L for a specific token
    async fn calculate_token_pnl(
        &self,
        token_mint: &str,
        events: Vec<FinancialEvent>,
    ) -> Result<(TokenPnL, Option<Holding>)> {
        debug!("Calculating P&L for token: {}", token_mint);
        
        let mut total_bought = Decimal::ZERO;
        let mut total_sold = Decimal::ZERO;
        let mut total_buy_cost_usd = Decimal::ZERO;
        let mut total_sell_revenue_usd = Decimal::ZERO;
        let mut buy_count = 0u32;
        let mut sell_count = 0u32;
        let mut first_buy_time = None;
        let mut last_sell_time = None;
        
        // Process buy/sell events
        for event in &events {
            match event.event_type {
                EventType::Buy => {
                    let price = self.get_event_price(&event).await?;
                    let cost_usd = event.token_amount * price;
                    
                    total_bought += event.token_amount;
                    total_buy_cost_usd += cost_usd;
                    buy_count += 1;
                    
                    if first_buy_time.is_none() {
                        first_buy_time = Some(event.timestamp);
                    }
                }
                EventType::Sell => {
                    let price = self.get_event_price(&event).await?;
                    let revenue_usd = event.token_amount * price;
                    
                    total_sold += event.token_amount;
                    total_sell_revenue_usd += revenue_usd;
                    sell_count += 1;
                    
                    last_sell_time = Some(event.timestamp);
                }
                _ => {} // Handle transfers separately if needed
            }
        }
        
        // Calculate averages
        let avg_buy_price_usd = if total_bought > Decimal::ZERO {
            total_buy_cost_usd / total_bought
        } else {
            Decimal::ZERO
        };
        
        let avg_sell_price_usd = if total_sold > Decimal::ZERO {
            total_sell_revenue_usd / total_sold
        } else {
            Decimal::ZERO
        };
        
        // Calculate realized P&L
        let realized_pnl_usd = total_sell_revenue_usd - (total_sold * avg_buy_price_usd);
        
        // Calculate current holdings and unrealized P&L
        let current_amount = total_bought - total_sold;
        let (unrealized_pnl_usd, current_holding) = if current_amount > Decimal::ZERO {
            // CRITICAL: Always fetch fresh current price for unrealized P&L calculations
            // Do NOT use get_event_price here as it may use cached historical prices
            debug!("Fetching FRESH current price for token {} (amount: {})", token_mint, current_amount);
            
            let current_prices = self.price_fetcher
                .fetch_prices(&[token_mint.to_string()], None)
                .await?;
            
            let fetched_price = current_prices
                .get(token_mint)
                .copied()
                .unwrap_or_else(|| {
                    warn!("No current price found for token {}, using zero", token_mint);
                    Decimal::ZERO
                });
            
            // Use the fresh current price from Jupiter API (now includes market simulation)
            let current_price = fetched_price;
            
            debug!("Token {}: current_amount={}, avg_buy_price_usd={}, FRESH_current_price={}", 
                  token_mint, current_amount, avg_buy_price_usd, current_price);
            
            let cost_basis = avg_buy_price_usd;
            let current_value = current_amount * current_price;
            let total_cost = current_amount * cost_basis;
            let unrealized = current_value - total_cost;
            
            debug!("Token {}: current_value={}, total_cost={}, unrealized_pnl={}", 
                  token_mint, current_value, total_cost, unrealized);
            
            // Validate that we're not using the same price for cost basis and current price
            if (current_price - cost_basis).abs() < Decimal::new(1, 10) { // 0.0000000001 tolerance
                warn!("WARNING: Current price ({}) is very close to cost basis ({}) for token {}. This may indicate a pricing issue.", 
                     current_price, cost_basis, token_mint);
            }
            
            let holding = Holding {
                token_mint: token_mint.to_string(),
                token_symbol: None, // Could be fetched separately
                amount: current_amount,
                avg_cost_basis_usd: cost_basis,
                current_price_usd: current_price,
                total_cost_basis_usd: total_cost,
                current_value_usd: current_value,
                unrealized_pnl_usd: unrealized,
            };
            
            (unrealized, Some(holding))
        } else {
            (Decimal::ZERO, None)
        };
        
        // Calculate hold time
        let hold_time_minutes = if let (Some(first_buy), Some(last_sell)) = (first_buy_time, last_sell_time) {
            Some(Decimal::from((last_sell - first_buy).num_minutes()))
        } else {
            None
        };
        
        let token_pnl = TokenPnL {
            token_mint: token_mint.to_string(),
            token_symbol: None,
            realized_pnl_usd,
            unrealized_pnl_usd,
            total_pnl_usd: realized_pnl_usd + unrealized_pnl_usd,
            buy_count,
            sell_count,
            total_bought,
            total_sold,
            avg_buy_price_usd,
            avg_sell_price_usd,
            first_buy_time,
            last_sell_time,
            hold_time_minutes,
        };
        
        Ok((token_pnl, current_holding))
    }
    
    /// Get price for an event, using metadata price or fetching current price
    async fn get_event_price(&self, event: &FinancialEvent) -> Result<Decimal> {
        // For historical transactions, prefer metadata price if available
        if let Some(price) = event.metadata.price_per_token {
            debug!("Using metadata price for {}: {}", event.token_mint, price);
            return Ok(price);
        }
        
        // Fallback to fetching current price (not ideal for historical accuracy)
        debug!("No metadata price for {}, fetching current price", event.token_mint);
        let prices = self.price_fetcher
            .fetch_prices(&[event.token_mint.clone()], None)
            .await?;
        
        let price = prices.get(&event.token_mint)
            .copied()
            .ok_or_else(|| PnLError::PriceFetch(format!("No price found for token: {}", event.token_mint)))?;
        
        debug!("Fetched current price for {}: {}", event.token_mint, price);
        Ok(price)
    }
    
    /// Calculate overall P&L summary
    async fn calculate_summary(
        &self,
        token_results: &[TokenPnL],
        events: &[FinancialEvent],
        _filters: &PnLFilters,
    ) -> Result<PnLSummary> {
        let total_realized_pnl = token_results.iter()
            .map(|t| t.realized_pnl_usd)
            .sum();
        
        let total_unrealized_pnl = token_results.iter()
            .map(|t| t.unrealized_pnl_usd)
            .sum();
        
        let total_pnl_usd = total_realized_pnl + total_unrealized_pnl;
        
        // Calculate fees
        let total_fees_sol = events.iter()
            .map(|e| e.transaction_fee)
            .sum();
        
        // Get SOL price for fee conversion (assuming SOL mint is So11111...)
        let sol_price = self.price_fetcher
            .fetch_prices(&["So11111111111111111111111111111111111111112".to_string()], None)
            .await?
            .get("So11111111111111111111111111111111111111112")
            .copied()
            .unwrap_or(Decimal::ZERO);
        
        let total_fees_usd = total_fees_sol * sol_price;
        
        // Calculate trade statistics
        let winning_trades = token_results.iter()
            .filter(|t| t.total_pnl_usd > Decimal::ZERO)
            .count() as u32;
        
        let losing_trades = token_results.iter()
            .filter(|t| t.total_pnl_usd < Decimal::ZERO)
            .count() as u32;
        
        let total_trades = token_results.len() as u32;
        
        let win_rate = if total_trades > 0 {
            Decimal::from(winning_trades * 100) / Decimal::from(total_trades)
        } else {
            Decimal::ZERO
        };
        
        // Calculate average hold time
        let hold_times: Vec<_> = token_results.iter()
            .filter_map(|t| t.hold_time_minutes)
            .collect();
        
        let avg_hold_time_minutes = if !hold_times.is_empty() {
            hold_times.iter().sum::<Decimal>() / Decimal::from(hold_times.len())
        } else {
            Decimal::ZERO
        };
        
        // Calculate total capital deployed (simplified - could be more sophisticated)
        let total_capital_deployed_sol = events.iter()
            .filter(|e| matches!(e.event_type, EventType::Buy))
            .map(|e| e.sol_amount)
            .sum();
        
        let roi_percentage = if total_capital_deployed_sol > Decimal::ZERO {
            (total_pnl_usd / (total_capital_deployed_sol * sol_price)) * Decimal::from(100)
        } else {
            Decimal::ZERO
        };
        
        Ok(PnLSummary {
            realized_pnl_usd: total_realized_pnl,
            unrealized_pnl_usd: total_unrealized_pnl,
            total_pnl_usd,
            total_fees_sol,
            total_fees_usd,
            winning_trades,
            losing_trades,
            total_trades,
            win_rate,
            avg_hold_time_minutes,
            total_capital_deployed_sol,
            roi_percentage,
        })
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    
    struct MockPriceFetcher;
    
    #[async_trait]
    impl PriceFetcher for MockPriceFetcher {
        async fn fetch_prices(
            &self,
            _token_mints: &[String],
            _vs_token: Option<&str>,
        ) -> Result<HashMap<String, Decimal>> {
            let mut prices = HashMap::new();
            prices.insert("test_token".to_string(), Decimal::from(100));
            prices.insert("So11111111111111111111111111111111111111112".to_string(), Decimal::from(50)); // SOL price
            Ok(prices)
        }
        
        async fn fetch_historical_price(
            &self,
            _token_mint: &str,
            _timestamp: DateTime<Utc>,
            _vs_token: Option<&str>,
        ) -> Result<Option<Decimal>> {
            Ok(Some(Decimal::from(100)))
        }
    }
    
    #[tokio::test]
    async fn test_basic_pnl_calculation() {
        let engine = PnLEngine::new(MockPriceFetcher);
        let wallet = "test_wallet";
        
        let events = vec![
            FinancialEvent {
                id: Uuid::new_v4(),
                transaction_id: "tx1".to_string(),
                wallet_address: wallet.to_string(),
                event_type: EventType::Buy,
                token_mint: "test_token".to_string(),
                token_amount: Decimal::from(100),
                sol_amount: Decimal::from(2),
                timestamp: Utc::now(),
                transaction_fee: "0.005".parse().unwrap(),
                metadata: EventMetadata::default(),
            },
        ];
        
        let filters = PnLFilters {
            min_capital_sol: Decimal::ZERO,
            min_hold_minutes: Decimal::ZERO,
            min_trades: 1,
            min_win_rate: Decimal::ZERO,
            max_signatures: None,
            timeframe_filter: None,
        };
        
        let result = engine.calculate_pnl(wallet, events, filters).await;
        assert!(result.is_ok());
        
        let report = result.unwrap();
        assert_eq!(report.wallet_address, wallet);
        assert_eq!(report.token_breakdown.len(), 1);
    }
}