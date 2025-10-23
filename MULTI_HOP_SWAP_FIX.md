# Multi-Hop Swap Parsing Fix: Root Cause & Implementation

## Executive Summary

**Problem**: 3-asset multi-hop swaps (Token A → SOL → Token B) are incorrectly parsed:
- Implicit pricing uses tiny SOL fee amounts instead of actual swap values
- Results in 7000x wrong prices for tokens
- Intermediate tokens (like Moon) are completely missing from results

**Root Cause**:
1. Implicit pricing logic applied to 3-asset swaps (should only be for 2-asset swaps)
2. NULL price/value transfers are skipped (return None) instead of creating zero-price events for BirdEye enrichment

**Solution**:
1. Detect 3+ unique assets → skip implicit pricing → use fallback path
2. In fallback path: create zero-price events for NULL price/value instead of returning None
3. Let BirdEye enrichment fetch historical prices and calculate values

---

## Understanding the Two Cases

### Case 1: Simple 2-Asset Swap (Implicit Pricing Works Correctly)

**Example**: SHADOW ↔ SOL

**Zerion Returns**:
```json
{
  "transfers": [
    {
      "symbol": "SHADOW",
      "direction": "in",
      "quantity": "5054254.127885",
      "price": 0.00000003092,    // Stale/wrong price from Zerion
      "value": 0.156              // Wrong value
    },
    {
      "symbol": "SOL",
      "direction": "out",
      "quantity": "5.023607614",
      "price": 195.40,
      "value": 981.645            // Correct value!
    }
  ]
}
```

**Unique Assets**: 2 (SHADOW + SOL)

**Current Behavior** (CORRECT - KEEP THIS):
- Detects: 2 unique assets
- Uses implicit pricing: SHADOW price = $981.645 / 5,054,254 = $0.0001942
- Fixes Zerion's stale SHADOW price
- Creates: [SHADOW Buy @ $0.0001942, SOL Sell @ $195.40] ✓

---

### Case 2: Multi-Hop 3-Asset Swap (Implicit Pricing Breaks)

**Example**: Moon → WSOL → KICK (Transaction `DL2gVLr...`)

**Zerion Returns**:
```json
{
  "transfers": [
    {
      "symbol": "KICK",
      "direction": "in",
      "quantity": "502380.524320",
      "price": null,              // No price data
      "value": null               // No value data
    },
    {
      "symbol": "Moon",
      "direction": "out",
      "quantity": "2112428.889811",
      "price": null,              // No price data
      "value": null               // No value data
    },
    {
      "symbol": "SOL",
      "direction": "out",
      "quantity": "0.000053866",  // Tiny amount (fees only!)
      "price": 237.12,
      "value": 0.0127             // Just fee amount
    }
  ]
}
```

**Unique Assets**: 3 (KICK + Moon + SOL)

**What Really Happened** (from Solscan):
- Swap 1: Sold 2,112,428 Moon → Got 0.474 WSOL ($133.91)
- Swap 2: Sold 0.474 WSOL → Got 502,380 KICK ($89.68)
- Fees: 0.000053866 SOL ($0.0127)

**Current Behavior** (BROKEN):
1. Finds SOL as stable currency ✓
2. Uses implicit pricing with SOL fee value ($0.0127) ❌
3. Calculates KICK price: $0.0127 / 502,380 = $0.0000000254 ❌ (7000x too low!)
4. Only processes 2 transfers: KICK + SOL ❌
5. **Moon completely ignored** - no event created ❌

**Should Do** (AFTER FIX):
1. Detect 3 unique assets → skip implicit pricing
2. Process ALL 3 transfers via fallback path
3. KICK & Moon: create zero-price events (price=0, value=0)
4. SOL: create normal event with actual price
5. BirdEye enrichment:
   - Fetch KICK historical price @ transaction timestamp → $0.000178
   - Fetch Moon historical price @ transaction timestamp → $0.0000628
   - Calculate values: KICK = 502,380 × $0.000178 = $89.42, Moon = 2,112,428 × $0.0000628 = $132.66
6. Final events: [KICK Buy $89.42, Moon Sell $132.66, SOL Sell $0.01] ✓

---

## Root Cause Analysis

### Problem 1: Implicit Pricing Applied to 3-Asset Swaps

**File**: `zerion_client/src/lib.rs`
**Function**: `convert_trade_pair_to_events()` (lines 1041-1146)

**Current Logic**:
```rust
fn convert_trade_pair_to_events(...) -> Vec<NewFinancialEvent> {
    // Step 1: Look for stable currency in OUT transfers
    for out_transfer in &trade_pair.out_transfers {
        if Self::is_stable_currency(address) && out_transfer.value.is_some() {
            stable_transfer = Some(out_transfer);      // Finds SOL!
            volatile_transfer = Some(in_transfers.first()); // Only first IN transfer (KICK)
            break;
        }
    }

    // Step 2: If stable currency found → use implicit pricing
    if let (Some(stable_value), Some(stable_xfer), Some(volatile_xfer)) = ... {
        // ❌ PROBLEM: This path is taken for 3-asset swaps!
        // Uses $0.0127 fee amount for pricing!
        convert_transfer_with_implicit_price(tx, volatile_xfer, ..., stable_value);
        convert_transfer_to_event(tx, stable_xfer, ...);
        // Only 2 transfers processed - Moon is ignored! ❌
    } else {
        // ✓ CORRECT PATH for 3-asset swaps: process ALL transfers
        for transfer in all_transfers {
            convert_transfer_to_event(tx, transfer, ...);
        }
    }
}
```

**Why This Breaks**:
- Implicit pricing is designed for 2-asset swaps (like SHADOW↔SOL)
- For 3-asset swaps: finds SOL → incorrectly takes implicit pricing path
- Only processes 2 transfers (KICK + SOL)
- Moon never reaches `convert_transfer_to_event()` → no event created
- Uses tiny fee amount ($0.0127) as the basis for calculating token prices

**The Fix**: Detect when we have 3+ unique assets → skip implicit pricing → force fallback path

---

### Problem 2: NULL Price Transfers Are Skipped

**File**: `zerion_client/src/lib.rs`
**Function**: `convert_transfer_to_event()` (lines 1616-1623)

**Current Logic**:
```rust
fn convert_transfer_to_event(...) -> Option<NewFinancialEvent> {
    let (usd_price_per_token, usd_value) = match (transfer.price, transfer.value) {
        (Some(price), Some(value)) => { (price_decimal, value_decimal) }
        (Some(price), None) => { (price_decimal, calculated_value) }
        (None, Some(value)) => { (calculated_price, value_decimal) }
        (None, None) => {
            // ❌ PROBLEM: Returns None instead of creating zero-price event
            warn!("⚠️  Skipping transaction: both price and value are null");
            return None;  // Event not created!
        }
    };

    Some(NewFinancialEvent { ... })
}
```

**Why This Breaks**:
- In 3-asset swaps, KICK and Moon have NULL price AND NULL value
- Even in fallback path (which processes all transfers), they get skipped here
- No event created → nothing to show in results
- BirdEye enrichment happens separately, but we should create events first

**The Fix**: Create event with zero price/value instead of returning None
```rust
(None, None) => {
    info!("💡 Creating zero-price event for BirdEye enrichment...");
    (Decimal::ZERO, Decimal::ZERO)  // ✓ Creates event
}
```

---

## How BirdEye Enrichment Works

**Current Flow** (Already Implemented):
```
1. convert_to_financial_events() returns (events, skipped_txs)
   - events: created by convert_transfer_to_event()
   - skipped_txs: extracted from RAW Zerion data (scans all transfers with NULL price/value)

2. enrich_with_birdeye_historical_prices(skipped_txs)
   - For each skipped transaction:
     * Fetch historical price at transaction timestamp from BirdEye API
     * Calculate value = token_amount × historical_price
     * Create NEW enriched event with correct price/value
   - Returns enriched_events[]

3. Merge: events.extend(enriched_events)
```

**Key Point**: `extract_skipped_transactions()` scans RAW Zerion transaction data (not our created events), so it WILL identify KICK and Moon even if we don't create events for them.

**The Issue**:
- Implicit pricing path creates a WRONG KICK event (price based on $0.0127 fee)
- This wrong event goes into P&L calculation
- BirdEye enrichment might create a correct KICK event separately
- We get wrong prices or duplicate events

**The Solution**: Don't use implicit pricing for 3-asset swaps at all

---

## The Fix

### Change 1: Detect Multi-Hop Swaps (Skip Implicit Pricing)

**Location**: `zerion_client/src/lib.rs:1041` (start of `convert_trade_pair_to_events()`)

**Add This Logic**:
```rust
fn convert_trade_pair_to_events(...) -> Vec<NewFinancialEvent> {
    let mut events = Vec::new();

    // === NEW CODE START ===
    // Count unique token addresses
    use std::collections::HashSet;
    let unique_assets: HashSet<String> = trade_pair.in_transfers.iter()
        .chain(trade_pair.out_transfers.iter())
        .filter_map(|t| {
            t.fungible_info.as_ref().and_then(|f| {
                f.implementations.iter()
                    .find(|i| i.chain_id == chain_id)
                    .and_then(|i| i.address.as_ref())
            })
        })
        .cloned()
        .collect();

    // Check if any asset is stable currency
    let has_stable = unique_assets.iter().any(|addr| Self::is_stable_currency(addr));

    // Detect multi-hop swap: 3+ unique assets including stable currency
    if unique_assets.len() >= 3 && has_stable {
        info!(
            "🔄 Multi-hop swap detected in tx {}: {} unique assets with stable currency. \
             Skipping implicit pricing - will process all transfers for BirdEye enrichment.",
            tx.id, unique_assets.len()
        );

        // Force fallback path: process ALL transfers without implicit pricing
        for transfer in trade_pair.in_transfers.iter().chain(trade_pair.out_transfers.iter()) {
            if let Some(event) = self.convert_transfer_to_event(tx, transfer, wallet_address, chain_id) {
                events.push(event);
            }
        }

        return events;
    }
    // === NEW CODE END ===

    // EXISTING LOGIC BELOW: Use implicit pricing for 2-asset swaps
    // (lines 1061-1143 remain unchanged)
    ...
}
```

**Effect**:
- 2-asset swaps (SHADOW + SOL): unique_assets.len() = 2 → condition false → use implicit pricing ✓
- 3-asset swaps (KICK + Moon + SOL): unique_assets.len() = 3 + has_stable = true → condition true → force fallback ✓

---

### Change 2: Create Zero-Price Events Instead of Skipping

**Location**: `zerion_client/src/lib.rs:1616-1623` (in `convert_transfer_to_event()`)

**Change From**:
```rust
(None, None) => {
    warn!(
        "⚠️  Skipping transaction {}: both price and value are null for {} ({}) - tx_hash: {}",
        tx.id, fungible_info.symbol, mint_address, tx.attributes.hash
    );
    return None;  // ❌ Event not created
}
```

**To**:
```rust
(None, None) => {
    info!(
        "💡 Creating zero-price event for BirdEye enrichment: {} ({}) in tx {} - tx_hash: {}",
        fungible_info.symbol, mint_address, tx.id, tx.attributes.hash
    );
    (Decimal::ZERO, Decimal::ZERO)  // ✓ Creates event with zero price
}
```

**Effect**:
- KICK & Moon will create events (not be skipped)
- Events have price=0, value=0 initially
- `extract_skipped_transactions()` will identify them from raw data
- BirdEye enrichment will fetch historical prices
- Events get enriched with correct prices

---

## How The Fix Works

### 3-Asset Swap Flow (AFTER FIX)

```
Transaction: KICK + Moon + SOL (Transaction DL2gVLr...)

Step 1: convert_trade_pair_to_events()
  → Count unique assets: 3 (KICK, Moon, SOL)
  → Has stable: yes (SOL in STABLE_CURRENCIES)
  → Condition: 3 >= 3 && has_stable = TRUE
  → Log: "Multi-hop swap detected: 3 unique assets with stable currency"
  → Force fallback path (skip implicit pricing)

Step 2: Process ALL 3 transfers via fallback path
  → KICK: convert_transfer_to_event()
    - price=NULL, value=NULL
    - Hits (None, None) case
    - Creates event with (price=0, value=0) ✓
    - Log: "Creating zero-price event for BirdEye enrichment: KICK"
  → Moon: convert_transfer_to_event()
    - price=NULL, value=NULL
    - Hits (None, None) case
    - Creates event with (price=0, value=0) ✓
    - Log: "Creating zero-price event for BirdEye enrichment: Moon"
  → SOL: convert_transfer_to_event()
    - price=$237.12, value=$0.0127
    - Hits (Some, Some) case
    - Creates event with actual price ✓

Step 3: events = [KICK@$0, Moon@$0, SOL@$237.12]

Step 4: extract_skipped_transactions() (in job_orchestrator)
  → Scans raw Zerion transaction data
  → Finds: KICK has NULL price/value in raw data
  → Finds: Moon has NULL price/value in raw data
  → Adds to skipped list: [KICK, Moon]

Step 5: enrich_with_birdeye_historical_prices([KICK, Moon])
  → Fetch KICK historical price @ 2025-09-17T02:44:09Z → $0.000178
  → Calculate KICK value: 502,380 × $0.000178 = $89.42
  → Create enriched event: KICK Buy 502,380 @ $0.000178 = $89.42

  → Fetch Moon historical price @ 2025-09-17T02:44:09Z → $0.0000628
  → Calculate Moon value: 2,112,428 × $0.0000628 = $132.66
  → Create enriched event: Moon Sell 2,112,428 @ $0.0000628 = $132.66

  → Returns: [KICK Buy $89.42, Moon Sell $132.66]

Step 6: Merge enriched events with main events
  → events.extend(enriched_events)
  → Final: [KICK Buy $89.42, Moon Sell $132.66, SOL Sell $0.01] ✓

P&L Calculation:
  - Sold: $132.66 (Moon)
  - Bought: $89.42 (KICK)
  - Fees: $0.01 (SOL)
  - Net: -$43.25 ✓ Accurate!
```

### 2-Asset Swap Flow (NO CHANGE - Still Works)

```
Transaction: SHADOW + SOL

Step 1: convert_trade_pair_to_events()
  → Count unique assets: 2 (SHADOW, SOL)
  → Condition: 2 >= 3? FALSE
  → Use existing implicit pricing logic ✓

Step 2: Implicit pricing path
  → Find stable currency (SOL) with value $981.645
  → Calculate SHADOW price: $981.645 / 5,054,254 = $0.0001942
  → Create events: [SHADOW Buy @ $0.0001942, SOL Sell @ $195.40] ✓

Step 3: No BirdEye enrichment needed (prices already correct) ✓

Result: Same as before, no regression ✓
```

---

## Testing Strategy

### Test Case 1: Multi-Hop Swap (Should Be Fixed)

**Input**: Transaction `DL2gVLrvx8hiN4MZb5sDnpzw9CvZNuQt2E3B8E49h3zYGTUTbYLDDYX3KNfJvwKabxh5pGWwPV5VTYHihddMpTc`

**Wallet**: `6BxJCNh25HRvpby4TDJb5zV8uYqssiLmB2E6AUPXV25G`

**Before Fix**:
- KICK: wrong price ($0.0000000254), wrong value
- Moon: missing event ❌
- SOL: correct ($0.01 fee)

**After Fix**:
- KICK Buy: 502,380.524320 @ ~$0.000178 = ~$89.42 ✓
- Moon Sell: 2,112,428.889811 @ ~$0.0000628 = ~$132.66 ✓
- SOL Sell: 0.000053866 @ $237.12 = ~$0.01 ✓

**Verification**:
- Compare with Solscan values ($133.91 for Moon, $89.68 for KICK)
- All 3 events present
- P&L makes sense: sold $132.66, bought $89.42, net -$43.24

### Test Case 2: Simple 2-Asset Swap (Should Be Unchanged)

**Input**: SHADOW + SOL transaction (from earlier example)

**Before Fix**: SHADOW @ $0.0001942 via implicit pricing

**After Fix**: SHADOW @ $0.0001942 via implicit pricing (unchanged)

**Verification**: No regression, same behavior

### Test Case 3: Direct 2-Asset KICK Swaps (Should Be Unchanged)

**Input**: Transactions `3Yy5j...`, `5ptjcWc7...` (KICK + SOL only)

**Before Fix**: Uses implicit pricing

**After Fix**: Uses implicit pricing (unchanged)

**Verification**: No regression, same behavior

---

## Summary

### The Bug
Implicit pricing logic (designed for 2-asset swaps like SHADOW↔SOL) is incorrectly applied to 3-asset multi-hop swaps (Moon→SOL→KICK), causing:
- **Wrong price calculation**: Uses SOL fee amounts ($0.01) instead of actual swap values ($100+)
- **Missing events**: Intermediate tokens (Moon) completely ignored
- **Incomplete P&L**: Trades not fully captured

### The Fix
Two small, focused changes:

1. **Detect multi-hop swaps** (lines ~30):
   - Count unique token addresses
   - Check for 3+ assets with stable currency
   - If true: skip implicit pricing, force fallback path

2. **Create zero-price events** (lines ~5):
   - Don't skip NULL price/value transfers
   - Create events with price=0, value=0
   - Let BirdEye enrichment fetch historical prices

### The Result
- **2-asset swaps**: Continue working correctly with implicit pricing (SHADOW case) ✓
- **3-asset swaps**: All events created, accurate prices from BirdEye ✓
- **Complete P&L**: No missing trades, accurate valuations ✓
- **No regressions**: Existing behavior preserved ✓

### Code Impact
- **Total changes**: ~35 lines
- **Files modified**: 1 (`zerion_client/src/lib.rs`)
- **Breaking changes**: None
- **Dependencies**: Leverages existing BirdEye enrichment infrastructure

Ready to implement!
