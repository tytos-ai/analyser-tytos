/// Solana transaction processing that replicates TypeScript accounts.ts logic
use crate::{ParseError, Result};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};
use tracing::{debug, trace, warn};

/// Known aggregator program IDs (from TypeScript aggregatorlist.ts)
pub const AGGREGATOR_PROGRAM_IDS: &[&str] = &[
    "JSW99DKmxNyREQM14SQLDykeBvEUG63TeohrvmofEiw",
    "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4",
    "JUP4Fb2cqiRUcaTHdrPC8h2gNsA2ETXiPDD33WcGuJB",
    "DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M",
    "j1o2qRpjcyUwEvwtcfhEQefh773ZgjxcVRry7LDqg5X",
    "6LtLpnUFNByNXLyCoK9wA2MykKAmQNZKBdY8s47dehDc",
    "2wT8Yq49kHgDzXuPxZSaeLaH1qbmGXtEyPy64bL7aD3c",
    "EewxydAPCCVuNEyrVN68PuSYdQ7wKn27V9Gjeoi8dy3S",
    "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo",
    "Eo7WjKq67rjJQSZxS6z3YkapzY3eMj6Xy8X5EQVn5UaB",
    "opnb2LAfJYbRMAHHvqjCwQxanZn7ReEHp1k81EohpZb",
    "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc",
    "DjVE6JNiYqPL2QXyCUUh8rNjHrbz9hXHNYt99MQ59qw1",
    "9W959DqEETiGZocYWCQPaJ6sBmUzgfxXfqGeTEdp3aQP",
    "PhoeNiXZ8ByJGLkxNfZRnkUfjvmuYqLR89jjFHGqdXY",
    "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P",
    "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA",
    "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8",
    "5quBtoiQqxF9Jv6KYKctB59NT3gtJD2Y65kdnB1Uev3h",
    "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK",
    "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C",
    "stkitrT1Uoy18Dk1fTrgPw8W6MVzoCfYoAFT4MLsmhq",
    "5ocnV1qiCgaQR8Jb8xWnVbApfaygJ8tNoZfgPwsgx9kx",
    "swapNyd8XiQwJ6ianp9snpu4brUqFxadzvHebnAXjJZ",
    "swapFpHWwjELNnjvThjajtiVmkz3yPQEHjLtka2fwHW",
];

/// Stable coin mints (from TypeScript)
pub const USDC_MINTS: &[&str] = &["EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"];
pub const USDT_MINTS: &[&str] = &["Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB"];
pub const SOL_MINT: &str = "So11111111111111111111111111111111111111112";

/// Transaction record as used in TypeScript accounts.ts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxRecord {
    pub txid: String,
    pub operation: String, // "buy" | "sell"
    pub main_operation: String, // "swap" | "transfer"
    pub mint_change: Decimal,
    pub sol: Decimal,
    pub block_time: i64, // Unix timestamp
}

/// Token change detected in a transaction
#[derive(Debug, Clone)]
pub struct TokenChange {
    pub mint: String,
    pub diff: Decimal,
}

/// Result of computing changes for a transaction
#[derive(Debug, Clone)]
pub struct TransactionChanges {
    pub sol_change: Decimal,
    pub token_changes: Vec<TokenChange>,
}

/// Check if transaction involves an aggregator program (TypeScript: isAggregatorSwap)
pub fn is_aggregator_swap(transaction_json: &JsonValue) -> bool {
    let account_keys = transaction_json
        .get("transaction")
        .and_then(|t| t.get("message"))
        .and_then(|m| m.get("accountKeys"))
        .and_then(|k| k.as_array());
    
    if let Some(keys) = account_keys {
        for key_obj in keys {
            if let Some(pubkey) = key_obj.get("pubkey").and_then(|p| p.as_str()) {
                if AGGREGATOR_PROGRAM_IDS.contains(&pubkey) {
                    trace!("Found aggregator program: {}", pubkey);
                    return true;
                }
            }
        }
    }
    
    false
}

/// Compute SOL and token balance changes (TypeScript: computeChanges)
pub fn compute_changes(
    transaction_json: &JsonValue,
    wallet_index: usize,
    wallet_pubkey: &str,
    price_map: &HashMap<String, Decimal>,
) -> Result<TransactionChanges> {
    let meta = transaction_json.get("meta")
        .ok_or_else(|| ParseError::InvalidTransaction("Missing meta field".to_string()))?;
    
    // Compute SOL change
    let pre_lamports = meta.get("preBalances")
        .and_then(|b| b.as_array())
        .and_then(|arr| arr.get(wallet_index))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    
    let post_lamports = meta.get("postBalances")
        .and_then(|b| b.as_array())
        .and_then(|arr| arr.get(wallet_index))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    
    let mut sol_change = Decimal::from(post_lamports - pre_lamports) / Decimal::from(1_000_000_000); // Convert lamports to SOL
    
    // Process token balance changes
    let empty_vec = vec![];
    let pre_token_balances = meta.get("preTokenBalances")
        .and_then(|b| b.as_array())
        .unwrap_or(&empty_vec);
    
    let empty_vec2 = vec![];
    let post_token_balances = meta.get("postTokenBalances")
        .and_then(|b| b.as_array())
        .unwrap_or(&empty_vec2);
    
    let mut pre_map: HashMap<String, Decimal> = HashMap::new();
    let mut post_map: HashMap<String, Decimal> = HashMap::new();
    
    // Build pre-transaction token balances
    for balance in pre_token_balances {
        if let (Some(owner), Some(mint), Some(amount)) = (
            balance.get("owner").and_then(|o| o.as_str()),
            balance.get("mint").and_then(|m| m.as_str()),
            balance.get("uiTokenAmount").and_then(|ui| ui.get("uiAmount")).and_then(|a| a.as_f64())
        ) {
            if owner == wallet_pubkey {
                pre_map.insert(mint.to_string(), Decimal::try_from(amount).unwrap_or_default());
            }
        }
    }
    
    // Build post-transaction token balances
    for balance in post_token_balances {
        if let (Some(owner), Some(mint), Some(amount)) = (
            balance.get("owner").and_then(|o| o.as_str()),
            balance.get("mint").and_then(|m| m.as_str()),
            balance.get("uiTokenAmount").and_then(|ui| ui.get("uiAmount")).and_then(|a| a.as_f64())
        ) {
            if owner == wallet_pubkey {
                post_map.insert(mint.to_string(), Decimal::try_from(amount).unwrap_or_default());
            }
        }
    }
    
    // Compute token changes and stable coin SOL equivalent
    let mut token_changes = Vec::new();
    let mut stable_coin_sol_equivalent = Decimal::ZERO;
    
    let mut all_mints = HashSet::new();
    all_mints.extend(pre_map.keys().cloned());
    all_mints.extend(post_map.keys().cloned());
    
    for mint in all_mints {
        let pre_amount = pre_map.get(&mint).copied().unwrap_or_default();
        let post_amount = post_map.get(&mint).copied().unwrap_or_default();
        let diff = post_amount - pre_amount;
        
        if diff.is_zero() {
            continue;
        }
        
        // Handle stable coins by converting to SOL equivalent
        if USDC_MINTS.contains(&mint.as_str()) {
            let usdc_to_sol = price_map.get(&mint).copied().unwrap_or_default();
            stable_coin_sol_equivalent += diff * usdc_to_sol;
            debug!("USDC change: {} * {} SOL = {} SOL equivalent", diff, usdc_to_sol, diff * usdc_to_sol);
        } else if USDT_MINTS.contains(&mint.as_str()) {
            let usdt_to_sol = price_map.get(&mint).copied().unwrap_or_default();
            stable_coin_sol_equivalent += diff * usdt_to_sol;
            debug!("USDT change: {} * {} SOL = {} SOL equivalent", diff, usdt_to_sol, diff * usdt_to_sol);
        } else {
            // Regular token change
            token_changes.push(TokenChange {
                mint,
                diff,
            });
        }
    }
    
    // Add stable coin SOL equivalent to total SOL change
    sol_change += stable_coin_sol_equivalent;
    
    debug!(
        "Transaction changes: SOL change = {}, token changes = {}, stable coin SOL equiv = {}",
        sol_change, token_changes.len(), stable_coin_sol_equivalent
    );
    
    Ok(TransactionChanges {
        sol_change,
        token_changes,
    })
}

/// Process a single parsed transaction to create TxRecords (TypeScript: handleParsedTransaction)
pub fn process_parsed_transaction(
    transaction_json: &JsonValue,
    wallet_address: &str,
    price_map: &HashMap<String, Decimal>,
) -> Result<Vec<TxRecord>> {
    let mut records = Vec::new();
    
    // Extract basic transaction info
    let block_time = transaction_json.get("blockTime")
        .and_then(|bt| bt.as_i64())
        .unwrap_or(0);
    
    let txid = transaction_json.get("transaction")
        .and_then(|t| t.get("signatures"))
        .and_then(|sigs| sigs.as_array())
        .and_then(|arr| arr.get(0))
        .and_then(|sig| sig.as_str())
        .unwrap_or("unknown")
        .to_string();
    
    let message = transaction_json.get("transaction")
        .and_then(|t| t.get("message"))
        .ok_or_else(|| ParseError::InvalidTransaction("Missing transaction message".to_string()))?;
    
    let account_keys = message.get("accountKeys")
        .and_then(|k| k.as_array())
        .ok_or_else(|| ParseError::InvalidTransaction("Missing account keys".to_string()))?;
    
    // Find wallet index
    let wallet_index = account_keys.iter().position(|key| {
        key.get("pubkey").and_then(|p| p.as_str()) == Some(wallet_address)
    }).ok_or_else(|| ParseError::InvalidTransaction("Wallet not found in account keys".to_string()))?;
    
    // Determine if this is a swap or transfer
    let main_operation = if is_aggregator_swap(transaction_json) {
        "swap"
    } else {
        "transfer"
    };
    
    // Compute changes
    let changes = compute_changes(transaction_json, wallet_index, wallet_address, price_map)?;
    
    // Create records for each token change
    for token_change in changes.token_changes {
        let operation = if token_change.diff < Decimal::ZERO {
            "sell"
        } else {
            "buy"
        };
        
        let record = TxRecord {
            txid: txid.clone(),
            operation: operation.to_string(),
            main_operation: main_operation.to_string(),
            mint_change: token_change.diff,
            sol: changes.sol_change,
            block_time,
        };
        
        records.push(record);
        
        trace!(
            "Created record: {} {} {} tokens, SOL change: {}, main_op: {}",
            operation, token_change.diff.abs(), token_change.mint, changes.sol_change, main_operation
        );
    }
    
    Ok(records)
}

/// Create account amounts data structure (TypeScript: accamounts:{wallet}:{mint} format)
pub fn create_account_amounts_data(records: &[TxRecord]) -> JsonValue {
    serde_json::json!({
        "records": records
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    #[test]
    fn test_is_aggregator_swap() {
        let tx_with_aggregator = json!({
            "transaction": {
                "message": {
                    "accountKeys": [
                        {"pubkey": "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4"},
                        {"pubkey": "other_program"}
                    ]
                }
            }
        });
        
        assert!(is_aggregator_swap(&tx_with_aggregator));
        
        let tx_without_aggregator = json!({
            "transaction": {
                "message": {
                    "accountKeys": [
                        {"pubkey": "some_other_program"},
                        {"pubkey": "another_program"}
                    ]
                }
            }
        });
        
        assert!(!is_aggregator_swap(&tx_without_aggregator));
    }
    
    #[test]
    fn test_stable_coin_detection() {
        assert!(USDC_MINTS.contains(&"EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"));
        assert!(USDT_MINTS.contains(&"Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB"));
        assert!(!USDC_MINTS.contains(&"some_other_mint"));
    }
}