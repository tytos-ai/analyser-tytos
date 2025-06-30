use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tracing::{debug, trace};

/// Represents an open buy position for FIFO matching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenBuyChunk {
    /// When this buy occurred
    pub block_time: DateTime<Utc>,
    
    /// Cost in SOL (negative value, as it's an outflow)
    pub cost_sol: Decimal,
    
    /// Token quantity purchased
    pub token_qty: Decimal,
}

/// Represents a leftover (unsold) position
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeftoverChunk {
    /// Wallet address
    pub wallet: String,
    
    /// Token mint
    pub mint: String,
    
    /// Token quantity remaining
    pub token_qty: Decimal,
    
    /// Cost basis in SOL (negative)
    pub cost_sol: Decimal,
}

/// Transaction record as processed by the TypeScript system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxRecord {
    /// Transaction ID
    pub txid: String,
    
    /// Token mint address - CRITICAL for proper FIFO grouping
    pub token_mint: String,
    
    /// Buy or sell operation
    pub operation: String, // "buy" | "sell"
    
    /// Main operation type
    pub main_operation: String, // "swap" | "transfer"
    
    /// Change in token amount (positive for buy, negative for sell)
    pub mint_change: Decimal,
    
    /// SOL change (negative for buy, positive for sell)
    pub sol: Decimal,
    
    /// Block timestamp
    pub block_time: DateTime<Utc>,
}

/// Partial result for a single wallet
#[derive(Debug, Clone)]
pub struct PartialResult {
    pub wallet: String,
    pub realized_profit: Decimal,
    pub realized_loss: Decimal,
    pub leftover: Vec<LeftoverChunk>,
    pub mint_details: Vec<MintDetail>,
    pub trade_count: u32,
}

/// Details for a specific mint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintDetail {
    pub mint: String,
    pub hold_time_sec: i64,
    pub net_sol: Decimal,
}

/// Stable coin mints (excluded from leftover calculations)
const STABLE_MINTS: &[&str] = &[
    "So11111111111111111111111111111111111111112", // Wrapped SOL
    "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", // USDC
    "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB", // USDT
];

/// Hidden time limit for short hold detection (replicates TypeScript logic)
fn hidden_time_limit() -> i64 {
    10000 - 9800 // = 200 seconds
}

/// Implements the exact partial FIFO logic from TypeScript
pub fn close_partial_fifo(
    open_buys: &mut Vec<OpenBuyChunk>,
    total_to_close: Decimal,
    sol_from_this_sell: Decimal,
    sell_block_time: DateTime<Utc>,
    min_hold_sec: i64,
) -> Decimal {
    let mut remains = total_to_close;
    let mut total_profit = Decimal::ZERO;
    
    if remains <= Decimal::new(1, 9) { // 1e-9
        debug!("totalToClose=0 or negligible => skipping");
        return total_profit;
    }
    
    debug!(
        "Starting partial-FIFO. totalToClose={}, solFromThisSell={}, minHoldSec={}",
        total_to_close, sol_from_this_sell, min_hold_sec
    );
    
    for open_buy in open_buys.iter_mut() {
        if remains <= Decimal::new(1, 9) { // 1e-9
            break;
        }
        
        if open_buy.token_qty <= Decimal::new(1, 9) { // 1e-9
            continue;
        }
        
        let chunk_qty = open_buy.token_qty;
        let hold_sec = sell_block_time.timestamp() - open_buy.block_time.timestamp();
        
        trace!(
            "FIFO-ITER: chunkQty={}, costSol={}, holdSec={}, remainsToClose={}",
            chunk_qty, open_buy.cost_sol, hold_sec, remains
        );
        
        if chunk_qty <= remains + Decimal::new(1, 9) { // 1e-9
            // Use entire chunk
            let fraction = chunk_qty / total_to_close;
            
            if hold_sec >= min_hold_sec {
                let chunk_sale_value = fraction * sol_from_this_sell;
                let profit = chunk_sale_value + open_buy.cost_sol; // cost_sol is negative
                total_profit += profit;
                
                debug!(
                    "chunk fully used: fraction={}, chunkSaleValue={}, profit={} (HOLD OK: holdSec={} >= minHoldSec={})",
                    fraction, chunk_sale_value, profit, hold_sec, min_hold_sec
                );
            } else {
                debug!(
                    "chunk fully used, but holdSec={} < minHoldSec={} => skipping profit",
                    hold_sec, min_hold_sec
                );
            }
            
            remains -= chunk_qty;
            open_buy.token_qty = Decimal::ZERO;
            open_buy.cost_sol = Decimal::ZERO;
        } else {
            // Use partial chunk
            if hold_sec >= min_hold_sec {
                let fraction_of_tx = remains / total_to_close;
                let fraction_of_chunk = remains / chunk_qty;
                let chunk_sale_value = fraction_of_tx * sol_from_this_sell;
                let partial_cost = fraction_of_chunk * open_buy.cost_sol;
                let profit = chunk_sale_value + partial_cost; // partial_cost is negative
                total_profit += profit;
                
                debug!(
                    "partial chunk usage: fractionOfTx={}, fractionOfChunk={}, chunkSaleValue={}, partialCost={}, profit={} (HOLD OK: holdSec={} >= minHoldSec={})",
                    fraction_of_tx, fraction_of_chunk, chunk_sale_value, partial_cost, profit, hold_sec, min_hold_sec
                );
            } else {
                debug!(
                    "partial chunk usage, but holdSec={} < minHoldSec={} => skipping profit",
                    hold_sec, min_hold_sec
                );
            }
            
            open_buy.token_qty -= remains;
            let cost_fraction = remains / chunk_qty;
            open_buy.cost_sol -= cost_fraction * open_buy.cost_sol;
            remains = Decimal::ZERO;
        }
    }
    
    total_profit
}

/// Process a single mint's transactions to calculate P&L
pub async fn process_mint_transactions(
    wallet: &str,
    mint: &str,
    mut records: Vec<TxRecord>,
    min_hold_sec: i64,
    jupiter_price_fetcher: Option<fn(&str) -> std::result::Result<Decimal, String>>,
) -> std::result::Result<(Decimal, Decimal, Vec<LeftoverChunk>, MintDetail, u32), String> {
    debug!("Processing mint={}, #records={} => sorting by blockTime...", mint, records.len());
    
    // Sort by block time (ascending)
    records.sort_by_key(|r| r.block_time);
    
    let mut mint_profit = Decimal::ZERO;
    let mut mint_loss = Decimal::ZERO;
    let mut net_sol = Decimal::ZERO;
    let mut open_buys: Vec<OpenBuyChunk> = Vec::new();
    let mut earliest_buy_time = None;
    let mut last_close_time = None;
    let mut has_bought = false;
    let mut trade_count = 0u32;
    
    for tx in &records {
        trace!(
            "TX: mainOp={}, op={}, mintChange={}, sol={}, time={}, txid={}",
            tx.main_operation, tx.operation, tx.mint_change, tx.sol, tx.block_time, tx.txid
        );
        
        if tx.operation == "buy" {
            if tx.main_operation == "swap" {
                open_buys.push(OpenBuyChunk {
                    block_time: tx.block_time,
                    cost_sol: tx.sol, // Should be negative for buys
                    token_qty: tx.mint_change, // Should be positive for buys
                });
                
                if !has_bought {
                    earliest_buy_time = Some(tx.block_time);
                    has_bought = true;
                }
                
                debug!("Recorded buy: costSol={}, tokenQty={}", tx.sol, tx.mint_change);
            }
        } else {
            // Sell operation
            let total_to_close = tx.mint_change.abs();
            
            if tx.main_operation == "swap" {
                trade_count += 1;
                debug!("SELL via swap: totalToClose={}, tradeCount increment to={}", total_to_close, trade_count);
                
                let profit = close_partial_fifo(
                    &mut open_buys,
                    total_to_close,
                    tx.sol,
                    tx.block_time,
                    min_hold_sec,
                );
                
                if profit >= Decimal::ZERO {
                    mint_profit += profit;
                } else {
                    mint_loss += profit.abs();
                }
                net_sol += profit;
                
                debug!("partialFIFO: appliedProfit={}, netSol now={}", profit, net_sol);
                
                if has_bought {
                    last_close_time = Some(tx.block_time);
                }
            } else {
                // Transfer sell
                debug!("SELL via transfer: totalToClose={}", total_to_close);
                
                // Check for short hold + similar quantity logic
                let mut handled = false;
                for ob in &open_buys {
                    if ob.token_qty <= Decimal::new(1, 9) {
                        continue;
                    }
                    
                    let hold_sec = tx.block_time.timestamp() - ob.block_time.timestamp();
                    let ratio_sold = total_to_close / ob.token_qty;
                    let short_hold = hold_sec < hidden_time_limit();
                    let near_identical_qty = ratio_sold >= Decimal::new(96, 2); // 0.96
                    
                    if short_hold && near_identical_qty {
                        let fraction = std::cmp::min(
                            Decimal::ONE,
                            total_to_close / ob.token_qty
                        );
                        let computed_sol = ob.cost_sol * fraction * Decimal::NEGATIVE_ONE;
                        
                        debug!(
                            "shortHold & ~identical. Using buy cost => computedSol={}",
                            computed_sol
                        );
                        
                        let profit = close_partial_fifo(
                            &mut open_buys,
                            total_to_close,
                            computed_sol,
                            tx.block_time,
                            min_hold_sec,
                        );
                        
                        if profit >= Decimal::ZERO {
                            mint_profit += profit;
                        } else {
                            mint_loss += profit.abs();
                        }
                        net_sol += profit;
                        
                        debug!("partialFIFO: appliedProfit={}, netSol now={}", profit, net_sol);
                        
                        if has_bought {
                            last_close_time = Some(tx.block_time);
                        }
                        
                        handled = true;
                        break;
                    } else {
                        // Use Jupiter price for transfer
                        if let Some(price_fetcher) = jupiter_price_fetcher {
                            match price_fetcher(mint) {
                                Ok(transfer_sell_price) => {
                                    let computed_sol = total_to_close * transfer_sell_price;
                                    
                                    debug!(
                                        "transfer-sell => ignoring tx.sol ({}), using computedSol={} (price={})",
                                        tx.sol, computed_sol, transfer_sell_price
                                    );
                                    
                                    let profit = close_partial_fifo(
                                        &mut open_buys,
                                        total_to_close,
                                        computed_sol,
                                        tx.block_time,
                                        min_hold_sec,
                                    );
                                    
                                    if profit >= Decimal::ZERO {
                                        mint_profit += profit;
                                    } else {
                                        mint_loss += profit.abs();
                                    }
                                    net_sol += profit;
                                    
                                    debug!("partialFIFO: appliedProfit={}, netSol now={}", profit, net_sol);
                                    
                                    if has_bought {
                                        last_close_time = Some(tx.block_time);
                                    }
                                    
                                    handled = true;
                                    break;
                                }
                                Err(e) => {
                                    debug!("Failed to fetch Jupiter price for {}: {}", mint, e);
                                }
                            }
                        }
                    }
                }
                
                if !handled {
                    debug!("Transfer sell not handled - no matching open buys or price fetch failed");
                }
            }
        }
    }
    
    // Create leftover chunks for remaining open buys
    let mut leftover_chunks = Vec::new();
    for ob in &open_buys {
        if ob.token_qty > Decimal::new(1, 9) {
            if !STABLE_MINTS.contains(&mint) {
                leftover_chunks.push(LeftoverChunk {
                    wallet: wallet.to_string(),
                    mint: mint.to_string(),
                    token_qty: ob.token_qty,
                    cost_sol: ob.cost_sol,
                });
                
                debug!(
                    "leftover BUY chunk for mint={}, tokenQty={}, unrealized costSol={}",
                    mint, ob.token_qty, ob.cost_sol
                );
            } else {
                debug!("leftover chunk is stable token, skipping leftover for mint={}", mint);
            }
        }
    }
    
    let mint_net = mint_profit - mint_loss;
    let hold_time_sec = if let (Some(earliest), Some(latest)) = (earliest_buy_time, last_close_time) {
        if latest > earliest {
            latest.timestamp() - earliest.timestamp()
        } else {
            0
        }
    } else {
        0
    };
    
    let mint_detail = MintDetail {
        mint: mint.to_string(),
        hold_time_sec,
        net_sol: mint_net,
    };
    
    debug!(
        "minted detail updated: mint={}, holdTimeSec={}, netSol={}",
        mint, hold_time_sec, mint_net
    );
    
    Ok((mint_profit, mint_loss, leftover_chunks, mint_detail, trade_count))
}