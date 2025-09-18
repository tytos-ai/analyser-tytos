use crate::EnrichedTransaction;
use pnl_core::{HistoryBalanceChange, HistoryTransaction};

/// Implementation of HistoryTransaction trait for EnrichedTransaction
/// This allows the history parser to work with enriched transactions from dex_client
impl HistoryTransaction for EnrichedTransaction {
    fn get_tx_hash(&self) -> &str {
        &self.original.tx_hash
    }

    fn get_main_action(&self) -> &str {
        &self.original.main_action
    }

    fn get_block_time(&self) -> &str {
        &self.original.block_time
    }

    fn get_enriched_balance_changes(&self) -> Vec<HistoryBalanceChange> {
        self.enriched_balance_changes
            .iter()
            .map(|change| HistoryBalanceChange {
                amount: change.original.amount,
                symbol: change.original.symbol.clone(),
                address: change.original.address.clone(),
                decimals: change.original.decimals,
                price_per_token: change.price_per_token,
                price_resolved: change.price_resolved,
            })
            .collect()
    }
}
