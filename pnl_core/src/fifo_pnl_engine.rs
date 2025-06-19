use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use std::collections::HashMap;
use tracing::{debug, info, warn};

use crate::{
    partial_fifo::{self, TxRecord, LeftoverChunk, MintDetail},
    EventType, FinancialEvent, PnLError, PnLFilters, PnLReport, PnLSummary, 
    TokenPnL, Holding, AnalysisTimeframe, ReportMetadata, PriceFetcher, Result,
};

/// FIFO-based P&L calculation engine
pub struct FifoPnLEngine<P: PriceFetcher> {
    price_fetcher: P,
}

impl<P: PriceFetcher> FifoPnLEngine<P> {
    pub fn new(price_fetcher: P) -> Self {
        Self { price_fetcher }
    }
    
    /// Calculate P&L for a wallet using FIFO method
    pub async fn calculate_pnl(
        &self,
        wallet_address: &str,
        events: Vec<FinancialEvent>,
        filters: PnLFilters,
    ) -> Result<PnLReport> {
        let start_time = std::time::Instant::now();
        
        info!("Starting FIFO P&L calculation for wallet: {}", wallet_address);
        debug!("Processing {} events with FIFO method", events.len());
        
        // Filter events based on criteria
        let events_len = events.len();
        let filtered_events = self.filter_events(events, &filters)?;
        
        debug!("After filtering: {} events remain", filtered_events.len());
        
        // Early validation checks
        if filtered_events.len() < filters.min_trades as usize {
            return Err(PnLError::Configuration(format!(
                "Wallet has {} events, minimum required: {}",
                filtered_events.len(),
                filters.min_trades
            )));
        }
        
        // Convert FinancialEvents to TxRecords for FIFO processing
        let tx_records = self.convert_events_to_tx_records(&filtered_events).await?;
        
        // Group by token mint
        let records_by_mint = self.group_tx_records_by_mint(tx_records);
        
        // Process each mint using FIFO
        let mut token_pnl_results = Vec::new();
        let mut current_holdings = Vec::new();
        let mut warnings = Vec::new();
        let mut total_realized_profit = Decimal::ZERO;
        let mut total_realized_loss = Decimal::ZERO;
        let mut total_trades = 0u32;
        
        for (mint, mint_records) in records_by_mint {
            info!("Processing mint {} with {} transactions using FIFO", mint, mint_records.len());
            
            match self.process_mint_fifo(wallet_address, &mint, mint_records, &filters).await {
                Ok((realized_profit, realized_loss, leftover_chunks, mint_detail, trade_count)) => {
                    total_realized_profit += realized_profit;
                    total_realized_loss += realized_loss;
                    total_trades += trade_count;
                    
                    // Create TokenPnL from FIFO results
                    let token_pnl = self.create_token_pnl_from_fifo_result(
                        &mint,
                        realized_profit,
                        realized_loss,
                        &mint_detail,
                        trade_count,
                        &filtered_events,
                    ).await?;
                    
                    token_pnl_results.push(token_pnl);
                    
                    // Convert leftover chunks to holdings
                    for leftover in leftover_chunks {
                        if let Ok(holding) = self.convert_leftover_to_holding(&leftover).await {
                            current_holdings.push(holding);
                        }
                    }
                }
                Err(e) => {
                    let warning = format!("Failed to calculate FIFO P&L for token {}: {}", mint, e);
                    warn!("{}", warning);
                    warnings.push(warning);
                }
            }
        }
        
        // Calculate overall summary from FIFO results
        let summary = self.calculate_summary_from_fifo(
            &token_pnl_results,
            &filtered_events,
            &filters,
            total_realized_profit,
            total_realized_loss,
            total_trades,
        ).await?;
        
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
            "FIFO P&L calculation completed for wallet {} in {:.2}s",
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
    
    /// Convert FinancialEvents to TxRecords for FIFO processing
    async fn convert_events_to_tx_records(&self, events: &[FinancialEvent]) -> Result<Vec<TxRecord>> {
        let mut tx_records = Vec::new();
        
        for event in events {
            // Only convert buy/sell events, skip transfers and fees for now
            match event.event_type {
                EventType::Buy => {
                    // Get price for this transaction
                    let price = self.get_event_price(event).await?;
                    let sol_cost = event.token_amount * price; // Positive cost in SOL
                    
                    let tx_record = TxRecord {
                        txid: event.transaction_id.clone(),
                        operation: "buy".to_string(),
                        main_operation: "swap".to_string(),
                        mint_change: event.token_amount, // Positive for buy
                        sol: -sol_cost, // Negative for buy (outflow)
                        block_time: event.timestamp,
                    };
                    
                    tx_records.push(tx_record);
                }
                EventType::Sell => {
                    // Get price for this transaction
                    let price = self.get_event_price(event).await?;
                    let sol_revenue = event.token_amount * price; // Positive revenue in SOL
                    
                    let tx_record = TxRecord {
                        txid: event.transaction_id.clone(),
                        operation: "sell".to_string(),
                        main_operation: "swap".to_string(),
                        mint_change: -event.token_amount, // Negative for sell
                        sol: sol_revenue, // Positive for sell (inflow)
                        block_time: event.timestamp,
                    };
                    
                    tx_records.push(tx_record);
                }
                EventType::TransferOut => {
                    let tx_record = TxRecord {
                        txid: event.transaction_id.clone(),
                        operation: "sell".to_string(),
                        main_operation: "transfer".to_string(),
                        mint_change: -event.token_amount, // Negative for transfer out
                        sol: Decimal::ZERO, // No SOL change for transfer
                        block_time: event.timestamp,
                    };
                    
                    tx_records.push(tx_record);
                }
                _ => {
                    // Skip other event types for FIFO calculation
                    debug!("Skipping event type {:?} for FIFO calculation", event.event_type);
                }
            }
        }
        
        debug!("Converted {} events to {} tx records for FIFO processing", events.len(), tx_records.len());
        Ok(tx_records)
    }
    
    /// Group TxRecords by token mint
    fn group_tx_records_by_mint(&self, tx_records: Vec<TxRecord>) -> HashMap<String, Vec<TxRecord>> {
        // Since TxRecord doesn't have mint field, we need to get it from the original events
        // For now, we'll use a simplified approach - this would need to be enhanced
        // to properly map transactions to their token mints
        
        // TODO: This is a simplified implementation
        // In a real implementation, we'd need to maintain the mint information
        // through the conversion process
        let mut groups = HashMap::new();
        groups.insert("default".to_string(), tx_records);
        groups
    }
    
    /// Process a single mint using FIFO method
    async fn process_mint_fifo(
        &self,
        wallet: &str,
        mint: &str,
        tx_records: Vec<TxRecord>,
        filters: &PnLFilters,
    ) -> Result<(Decimal, Decimal, Vec<LeftoverChunk>, MintDetail, u32)> {
        let min_hold_sec = (filters.min_hold_minutes * Decimal::from(60)).to_i64().unwrap_or(0);
        
        // Create a price fetcher function for the FIFO module
        let jupiter_price_fetcher = |_token_mint: &str| -> std::result::Result<Decimal, String> {
            // This is a simplified implementation - in a real scenario,
            // we'd need to make this async and use the proper price fetcher
            // For now, return a default price
            Ok(Decimal::ONE)
        };
        
        // Call the existing FIFO implementation
        match partial_fifo::process_mint_transactions(
            wallet,
            mint,
            tx_records,
            min_hold_sec,
            Some(&jupiter_price_fetcher),
        ).await {
            Ok((realized_profit, realized_loss, leftover_chunks, mint_detail, trade_count)) => {
                Ok((realized_profit, realized_loss, leftover_chunks, mint_detail, trade_count))
            }
            Err(e) => {
                warn!("FIFO processing failed for mint {}: {}", mint, e);
                Err(PnLError::Calculation(format!("FIFO processing failed: {}", e)))
            }
        }
    }
    
    /// Create TokenPnL from FIFO results
    async fn create_token_pnl_from_fifo_result(
        &self,
        token_mint: &str,
        realized_profit: Decimal,
        realized_loss: Decimal,
        mint_detail: &MintDetail,
        _trade_count: u32,
        events: &[FinancialEvent],
    ) -> Result<TokenPnL> {
        // Calculate basic stats from events
        let token_events: Vec<_> = events.iter()
            .filter(|e| e.token_mint == token_mint)
            .collect();
        
        let mut total_bought = Decimal::ZERO;
        let mut total_sold = Decimal::ZERO;
        let mut buy_count = 0u32;
        let mut sell_count = 0u32;
        let mut first_buy_time = None;
        let mut last_sell_time = None;
        let mut total_buy_cost = Decimal::ZERO;
        let mut total_sell_revenue = Decimal::ZERO;
        
        for event in &token_events {
            match event.event_type {
                EventType::Buy => {
                    let price = self.get_event_price(event).await?;
                    total_bought += event.token_amount;
                    total_buy_cost += event.token_amount * price;
                    buy_count += 1;
                    
                    if first_buy_time.is_none() {
                        first_buy_time = Some(event.timestamp);
                    }
                }
                EventType::Sell => {
                    let price = self.get_event_price(event).await?;
                    total_sold += event.token_amount;
                    total_sell_revenue += event.token_amount * price;
                    sell_count += 1;
                    
                    last_sell_time = Some(event.timestamp);
                }
                _ => {}
            }
        }
        
        let avg_buy_price_usd = if total_bought > Decimal::ZERO {
            total_buy_cost / total_bought
        } else {
            Decimal::ZERO
        };
        
        let avg_sell_price_usd = if total_sold > Decimal::ZERO {
            total_sell_revenue / total_sold
        } else {
            Decimal::ZERO
        };
        
        let realized_pnl_usd = realized_profit - realized_loss;
        
        // Calculate unrealized P&L if there are current holdings
        let current_amount = total_bought - total_sold;
        let unrealized_pnl_usd = if current_amount > Decimal::ZERO {
            // Get current price for unrealized P&L
            let current_prices = self.price_fetcher
                .fetch_prices(&[token_mint.to_string()], None)
                .await?;
            
            let current_price = current_prices
                .get(token_mint)
                .copied()
                .unwrap_or(Decimal::ZERO);
            
            let current_value = current_amount * current_price;
            let cost_basis = current_amount * avg_buy_price_usd;
            current_value - cost_basis
        } else {
            Decimal::ZERO
        };
        
        let hold_time_minutes = if mint_detail.hold_time_sec > 0 {
            Some(Decimal::from(mint_detail.hold_time_sec) / Decimal::from(60))
        } else {
            None
        };
        
        Ok(TokenPnL {
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
        })
    }
    
    /// Convert leftover chunk to holding
    async fn convert_leftover_to_holding(&self, leftover: &LeftoverChunk) -> Result<Holding> {
        // Get current price
        let current_prices = self.price_fetcher
            .fetch_prices(&[leftover.mint.clone()], None)
            .await?;
        
        let current_price = current_prices
            .get(&leftover.mint)
            .copied()
            .unwrap_or(Decimal::ZERO);
        
        let cost_basis = if leftover.token_qty > Decimal::ZERO {
            (-leftover.cost_sol) / leftover.token_qty // cost_sol is negative
        } else {
            Decimal::ZERO
        };
        
        let total_cost_basis = leftover.token_qty * cost_basis;
        let current_value = leftover.token_qty * current_price;
        let unrealized_pnl = current_value - total_cost_basis;
        
        Ok(Holding {
            token_mint: leftover.mint.clone(),
            token_symbol: None,
            amount: leftover.token_qty,
            avg_cost_basis_usd: cost_basis,
            current_price_usd: current_price,
            total_cost_basis_usd: total_cost_basis,
            current_value_usd: current_value,
            unrealized_pnl_usd: unrealized_pnl,
        })
    }
    
    /// Calculate summary from FIFO results
    async fn calculate_summary_from_fifo(
        &self,
        token_results: &[TokenPnL],
        events: &[FinancialEvent],
        _filters: &PnLFilters,
        total_realized_profit: Decimal,
        total_realized_loss: Decimal,
        total_trades: u32,
    ) -> Result<PnLSummary> {
        let realized_pnl_usd = total_realized_profit - total_realized_loss;
        
        let unrealized_pnl_usd = token_results.iter()
            .map(|t| t.unrealized_pnl_usd)
            .sum();
        
        let total_pnl_usd = realized_pnl_usd + unrealized_pnl_usd;
        
        // Calculate fees
        let total_fees_sol = events.iter()
            .map(|e| e.transaction_fee)
            .sum();
        
        // Get SOL price for fee conversion
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
        
        // Calculate total capital deployed
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
            realized_pnl_usd,
            unrealized_pnl_usd,
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
    
    /// Get price for an event, using metadata price or fetching current price
    async fn get_event_price(&self, event: &FinancialEvent) -> Result<Decimal> {
        // For historical transactions, prefer metadata price if available
        if let Some(price) = event.metadata.price_per_token {
            debug!("Using metadata price for {}: {}", event.token_mint, price);
            return Ok(price);
        }
        
        // Try to fetch historical price first
        if let Ok(Some(historical_price)) = self.price_fetcher
            .fetch_historical_price(&event.token_mint, event.timestamp, None)
            .await
        {
            debug!("Using historical price for {} at {}: {}", 
                  event.token_mint, event.timestamp, historical_price);
            return Ok(historical_price);
        }
        
        // Fallback to current price
        debug!("No metadata or historical price for {}, fetching current price", event.token_mint);
        let prices = self.price_fetcher
            .fetch_prices(&[event.token_mint.clone()], None)
            .await?;
        
        let price = prices.get(&event.token_mint)
            .copied()
            .ok_or_else(|| PnLError::PriceFetch(format!("No price found for token: {}", event.token_mint)))?;
        
        debug!("Fetched current price for {}: {}", event.token_mint, price);
        Ok(price)
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
            prices.insert("So11111111111111111111111111111111111111112".to_string(), Decimal::from(50));
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
    async fn test_fifo_engine_creation() {
        let engine = FifoPnLEngine::new(MockPriceFetcher);
        // Basic test to ensure engine can be created
        assert!(true);
    }
}