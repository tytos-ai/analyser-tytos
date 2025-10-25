# ULTRA-DEEP BUG ANALYSIS & FIX PLAN

## Evidence Summary

### From Blockchain (Solscan):
- **AvQb9vkf**: 1 Solano IN (12,005,534), 3 SOL OUT (~$419)
- **261axxdq**: 1 Solano OUT (9,498,861 - SELL), 1 WSOL IN
- **36Ws79Xf**: 1 Solano OUT (12,708,273 - SELL), 1 WSOL IN

### From Zerion API:
- 11 buy transactions totaling 22,207,135 Solano
- 2 sell transactions totaling 22,207,134 Solano
- Net position should be ~0

### From System Output:
- **3 matched trades**, all with `tx_hash = AvQb9vkf`:
  1. Buy 12,005,534 @ $404.99 (correct qty, wrong price)
  2. Buy 702,739 @ $27.10 (PHANTOM - not in ANY transaction!)
  3. Buy 9,498,861 @ $366.31 (PHANTOM - this is SELL qty from 261axxdq!)
- **Remaining position**: 22,207,135 tokens
- **Double-counting**: Same 22M counted in matched + remaining

---

## Root Cause Hypotheses

### Hypothesis #1: Parser Creates Phantom BUY Events ⭐ MOST LIKELY
**Evidence:**
- All 3 matched buys have `tx_hash = AvQb9vkf`
- Quantities 702,739 and 9,498,861 don't match AvQb9vkf's actual transfer
- 702,739 doesn't exist in ANY Zerion transaction

**Theory:**
When processing AvQb9vkf (1 Solano IN, 3 SOL OUT), the parser somehow creates 3 SOLANO BUY events instead of 1.

**Possible Bug Locations:**
1. `pair_trade_transfers()` - Creates multiple trade pairs when it should create 1
2. `convert_trade_pair_to_events()` - Creates multiple BUY events for the same transfer
3. Implicit pricing logic - Incorrectly processing each SOL OUT as a separate BUY

### Hypothesis #2: Transaction Mixing
**Evidence:**
- Quantity 9,498,861 is from transaction 261axxdq
- It appears as a BUY under AvQb9vkf

**Theory:**
The parser is somehow mixing transfers from different transactions.

**Counter-evidence:**
- `pair_trade_transfers()` only receives transfers from ONE transaction
- Would need a bug in the caller that passes wrong data

### Hypothesis #3: Direction Inversion
**Evidence:**
- 261axxdq is a SELL (Solano OUT)
- But it creates a BUY event

**Theory:**
The direction determination is wrong for certain transaction types.

**Counter-evidence:**
- Code at line 1726-1728 looks correct: "in" → Buy, "out" → Sell
- Would affect ALL transactions, not just this one

### Hypothesis #4: Double-Counting in PNL Engine
**Evidence:**
- Same 22M appears in matched + remaining
- Total invested = matched + remaining

**Theory:**
The PNL engine calculates remaining position from the original event list instead of the mutated list after FIFO matching.

**Supporting evidence:**
- total_invested uses original `events` list (line 663-668 in new_pnl_engine.rs)
- remaining position should use MUTATED `buy_events` after matching

---

## The Smoking Gun: 702,739

**This quantity does NOT exist in any Zerion transaction.**

Possible explanations:
1. **Calculated value**:
   - 12,005,534 - 11,302,795 = 702,739
   - But where does 11,302,795 come from?

2. **Proportional split**:
   - Splitting 12M across 3 SOL transfers?
   - Tested: Doesn't match

3. **Implicit price calculation error**:
   - Using SOL quantity instead of Solano quantity?
   - Let me test: $27.10 / $0.0000386 = 702,073 tokens ≈ 702,739 ✓✓✓

**BREAKTHROUGH!**

$27.10 is close to one of the matched buy values! Let me check:
- Matched buy #2: 702,739 tokens @ $0.0000386 = $27.10

And $0.0000386 is the price for matched buys #2 and #3!

What if the parser is:
1. Processing AvQb9vkf
2. Summing SOL values: $404.99 + $4.09 + $0.75 = $409.83
3. Creating BUY events using INDIVIDUAL SOL values instead of the sum?

Let me test this theory:
- SOL transfer 1: $404.99 / (implicit price from $404.99) = 12,005,534? No...
- SOL transfer 2: $4.09 / (some price) = 702,739?
  - Price = $4.09 / 702,739 = $0.00000582
- SOL transfer 3: $0.75 / (some price) = ???

This doesn't quite work either.

Wait, let me reconsider. What if my recent "fix" that sums ALL stable transfers is actually WRONG, and it should only use the FIRST one?

If implicit pricing uses ONLY the first SOL transfer ($404.99) but there are multiple volatile transfers somehow, it might create multiple buys.

But Solscan shows only 1 Solano IN transfer!

---

## The Fix Plan

### Phase 1: Add Comprehensive Debug Logging ⚠️ CRITICAL

Add logging to trace EVERY event created:

**In `convert_to_financial_events()` (line 1502):**
```rust
debug!("🔍 Processing transaction {}/{}: {} (hash: {})",
    tx_index + 1, transactions.len(), tx.id, tx.attributes.hash);
```

**In `pair_trade_transfers()` (line 1016):**
```rust
fn pair_trade_transfers<'a>(transfers: &'a [ZerionTransfer]) -> Vec<TradePair<'a>> {
    debug!("📦 Pairing {} transfers", transfers.len());
    // existing code...
    debug!("📦 Created {} trade pairs", pairs_map.len());
    for (act_id, pair) in &pairs_map {
        debug!("  Pair {}: {} IN, {} OUT",
            act_id, pair.in_transfers.len(), pair.out_transfers.len());
    }
    // return
}
```

**In `convert_trade_pair_to_events()` (line 1041):**
```rust
fn convert_trade_pair_to_events(...) -> Vec<NewFinancialEvent> {
    debug!("💱 Converting trade pair (act_id: {}) for tx {}",
        trade_pair.act_id, tx.attributes.hash);
    debug!("  IN transfers: {}", trade_pair.in_transfers.len());
    debug!("  OUT transfers: {}", trade_pair.out_transfers.len());

    // At the end, before return:
    debug!("  ✅ Created {} events from this trade pair", events.len());
    for (i, event) in events.iter().enumerate() {
        debug!("    Event {}: {} {} {} @ ${:.10} = ${:.2}",
            i+1,
            if event.event_type == NewEventType::Buy { "BUY" } else { "SELL" },
            event.quantity,
            event.token_symbol,
            event.usd_price_per_token,
            event.usd_value
        );
    }
    events
}
```

### Phase 2: Run Debug Test

Test command:
```bash
RUST_LOG=debug cargo run -p api_server 2>&1 | tee debug_full.log
```

Then filter for AvQb9vkf:
```bash
grep -A 50 "AvQb9vkf" debug_full.log
```

Look for:
1. How many trade pairs are created from AvQb9vkf
2. How many events each pair creates
3. The exact quantities and symbols of each event

### Phase 3: Fix Based on Findings

**Scenario A**: Multiple trade pairs created
- Fix: Ensure all transfers with same act_id go in one pair

**Scenario B**: One pair creates multiple BUY events
- Fix: Ensure implicit pricing only creates 1 event per volatile transfer

**Scenario C**: Wrong tokens being processed
- Fix: Filter to only process the target token (Solano)

**Scenario D**: PNL engine double-counting
- Fix: Use mutated buy_events for remaining position, not original events

### Phase 4: Verify Fix

Re-run analysis on the problematic wallet and check:
- total_invested should be ~$7,128 (not $13,871)
- Matched trades should show buys from MULTIPLE transactions (not just AvQb9vkf)
- Remaining position should be ~0 (sold everything)
- No double-counting

---

## Immediate Action Required

1. **Add the debug logging code above**
2. **Run with RUST_LOG=debug on the problematic wallet**
3. **Examine the output for transaction AvQb9vkf**
4. **Report back what events are actually created**

This will reveal the exact bug location.
