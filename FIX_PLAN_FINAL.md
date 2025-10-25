# Final Fix Plan - Eliminate Duplicate Events from Enrichment

## Root Cause

**File:** `job_orchestrator/src/lib.rs:1370`
**Line:** `financial_events.extend(enriched_events);`

**Problem Flow:**
1. Transaction AvQb9vkf has:
   - 1 Solano transfer (price=NULL in raw data)
   - 3 SOL transfers (have prices)

2. Parser's implicit pricing (zerion_client):
   - Detects stable OUT transfers ($409.83 from 3 SOL)
   - Calculates implicit price for Solano
   - Creates 4 events: 1 Solano BUY + 3 SOL SELL

3. Extract skipped transactions (zerion_client):
   - Sees Solano transfer with price=NULL
   - Adds to skipped_txs list (doesn't know event was already created)

4. Enrichment (job_orchestrator):
   - Enriches the "skipped" Solano transfer via BirdEye
   - Creates ANOTHER Solano BUY event

5. Merge (job_orchestrator):
   - `financial_events.extend(enriched_events)`
   - Now we have 2 Solano BUY events for the same transaction!

## The Fix

**Option 1: Filter duplicates before extending (RECOMMENDED)**

In `job_orchestrator/src/lib.rs` around line 1370:

```rust
// Before extending, filter out enriched events that duplicate existing events
// by transaction hash + token address + direction
let existing_event_keys: HashSet<(String, String, String)> = financial_events
    .iter()
    .map(|e| (
        e.transaction_hash.clone(),
        e.token_address.clone(),
        format!("{:?}", e.event_type)
    ))
    .collect();

let unique_enriched_events: Vec<NewFinancialEvent> = enriched_events
    .into_iter()
    .filter(|e| {
        let key = (
            e.transaction_hash.clone(),
            e.token_address.clone(),
            format!("{:?}", e.event_type)
        );
        !existing_event_keys.contains(&key)
    })
    .collect();

if !unique_enriched_events.is_empty() {
    info!("✅ Adding {} unique enriched events (filtered {} duplicates)",
        unique_enriched_events.len(),
        enriched_events.len() - unique_enriched_events.len());
    financial_events.extend(unique_enriched_events);
} else {
    info!("ℹ️  All enriched events were duplicates - skipping");
}
```

**Option 2: Mark processed transfers during implicit pricing**

Track which transfers were processed in `ConversionResult`:

```rust
pub struct ConversionResult {
    pub events: Vec<NewFinancialEvent>,
    pub processed_transfer_ids: HashSet<String>,  // NEW
    pub skipped_count: u32,
    pub incomplete_trades_count: u32,
}
```

Then in `extract_skipped_transaction_info`, skip transfers that are in `processed_transfer_ids`.

## Expected Result

**Before Fix:**
- 12 Solano transactions
- Each creates 1 event via implicit pricing
- 10 of these also create enriched events
- Total: 12 + 10 = 22 events (but logs show 20, need to verify)

**After Fix:**
- 12 Solano transactions
- Each creates 1 event (either via implicit pricing OR enrichment, not both)
- Total: 12 events
- 10 BUY + 2 SELL = 12 ✅

**Impact on Metrics:**
- Total buy events: 20 → 10 (50% reduction)
- Total tokens bought: 44M → 22M (50% reduction)
- Remaining position: 22M → 0 (eliminates double-counting!)
- Total invested: $13,950 → $7,128 (49% reduction)

## Implementation

I'll implement **Option 1** as it's safer and doesn't require changing the parser interface.
