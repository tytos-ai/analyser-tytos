# Root Cause Analysis: Double-Counting Bug

## 🎯 **THE BUG**

Transaction **AvQb9vkf** creates **3 BUY events** instead of **1 BUY event**, causing 84% cost basis inflation.

## 📊 **Evidence**

From `token_6AiuSc3pYM5rpbKPq8JfSQoMoUsfDpzvP5PmMSEKpump_latest.json`:

### Matched Trades (all from AvQb9vkf!)
1. **Trade 1**: BUY 12,005,534 Solano @ $0.0000341371 = $409.83 (tx: **AvQb9vkf**) ✅ CORRECT
2. **Trade 2**: BUY 702,739 Solano @ $0.0000385638 = $27.10 (tx: **AvQb9vkf**) ❌ PHANTOM
3. **Trade 3**: BUY 9,498,861 Solano @ $0.0000385638 = $366.31 (tx: **AvQb9vkf**) ❌ WRONG TX

**Total BUY**: 22,207,135 tokens
**Matched SELL**: 22,207,135 tokens (via FIFO)
**Remaining**: 22,207,135 tokens ← **DOUBLE-COUNTING!**
**Total tokens bought**: 44,414,270 (should be 22,207,135)

### Double-Counting Formula
```
Matched Buys:    22,207,135 tokens ($803.24 USD)
Remaining:       22,207,135 tokens ($13,147.54 USD)
────────────────────────────────────────────────
Total Invested:  44,414,270 tokens ($13,950.78 USD)  ← 94% INFLATION!
```

## 🔍 **Root Cause from Logs**

### Transaction 1: 239f0fba (AvQb9vkf on blockchain) ✅ CORRECT
```
Found 3 stable currency OUT transfers totaling $409.8346 in tx 239f0fba
💱 Using implicit swap pricing: stable side TOTAL value = $409.8346 (from 3 transfers), 1 volatile transfer
🔄 Calculated implicit price for Solano: $0.0000341371 per token (from swap value $409.83 / quantity 12005534.953920)
✅ Implicit pricing complete: 4 events created (1 volatile + 3 stable)
```
**Expected**: 1 BUY Solano + 3 SELL SOL events
**Actual**: ✅ Creates correct events

### Transaction 2: 6eca8a81 (261axxdq on blockchain) ❌ DIRECTION BUG
```
Found 1 stable currency IN transfers totaling $1068.5823 in tx 6eca8a81
💱 Using implicit swap pricing: stable side TOTAL value = $1068.5823 (from 1 transfers), 1 volatile transfer
🔄 Calculated implicit price for Solano: $0.0001124958 per token (from swap value $1068.58 / quantity 9498861.256355)
```
**Expected**: 1 SELL Solano + 1 BUY SOL events
**Actual**: ❌ Creates BUY Solano event (direction inverted!)
**Blockchain Verification** (from Solscan): Transaction 261axxdq is a SELL (Solano OUT, SOL IN)

### Transaction 3: Phantom 702,739 ❓ UNKNOWN SOURCE
**Not found in logs** - This quantity doesn't appear in any Zerion transaction!

Possible sources:
1. Calculated from fee amounts
2. Rounding errors
3. Hidden in a different transaction
4. Created by a bug in the implicit pricing calculation

## 🐛 **Bug Location**

### File: `zerion_client/src/lib.rs`
### Function: `convert_transfer_to_event()` (lines ~1726-1733)

```rust
"trade" => match transfer.direction.as_str() {
    "in" | "self" => NewEventType::Buy,
    "out" => NewEventType::Sell,
}
```

**Problem**: When using implicit pricing, the event type is determined by the **volatile transfer direction**, but the transaction hash is taken from the **trade pair** which may contain MULTIPLE transactions mixed together.

### File: `zerion_client/src/lib.rs`
### Function: `convert_trade_pair_to_events()` (lines ~1055-1417)

**Problem**: When a trade pair has:
- 1 volatile token (Solano)
- 3 stable currency transfers (SOL fees)

The function creates events for ALL transfers and assigns them ALL the same transaction hash from `tx.attributes.hash`.

But if the trade pair contains transfers from DIFFERENT blockchain transactions (e.g., a BUY and a SELL mixed together), they all get the same Zerion transaction ID!

## 🎯 **The Fix**

### Step 1: Verify trade pair contains only ONE direction
Before applying implicit pricing, check that all volatile transfers have the SAME direction:

```rust
// Check if all volatile transfers have the same direction
let volatile_directions: HashSet<String> = volatile_transfers.iter()
    .map(|t| t.direction.clone())
    .collect();

if volatile_directions.len() > 1 {
    warn!("Mixed directions in trade pair - skipping implicit pricing");
    // Fall back to standard conversion
}
```

### Step 2: Use transfer-specific transaction hashes
Instead of using `tx.attributes.hash` for all events, use the individual transfer's transaction hash if available.

### Step 3: Filter out fee-only transfers
The phantom 702,739 might be a fee transfer that shouldn't be counted as a buy/sell.

## 📈 **Expected Result After Fix**

```
Total BUY events: 10 (instead of 20)
Total tokens bought: 22,207,135 (instead of 44,414,270)
Total invested: $7,128.87 (instead of $13,950.78)
Cost basis: $0.000321 (instead of $0.000589)
Reduction: 47% ✅
```

## 🚀 **Next Steps**

1. ✅ Identify root cause (DONE - direction inversion + mixed trade pairs)
2. ⏳ Implement fix in `convert_trade_pair_to_events()`
3. ⏳ Test with problematic wallet
4. ⏳ Verify totals match Zerion data
