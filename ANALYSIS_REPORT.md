# P&L Calculation Issues - Analysis Report

## Executive Summary

After analyzing the parser and PnL engine, I've identified **critical bugs** that cause incorrect `total_invested_usd` and `total_pnl_usd` calculations for some wallets.

## Observed Symptoms

From `token_6AiuSc3pYM5rpbKPq8JfSQoMoUsfDpzvP5PmMSEKpump_analysis.json`:
- **total_invested_usd**: $13,871.97 ❌
- **Matched trades** (3 trades): Buy events total only **$798** ($405 + $27 + $366)
- **Remaining position cost basis**: $13,073.56
- **Math check**: $798 (matched) + $13,073 (remaining) = $13,871 ✓ (internally consistent but WRONG)

**The Problem**: There are $13,073 worth of "phantom" buy events that never actually happened!

---

## Root Cause Analysis

### Issue #1: Multi-Hop Swap Mishandling (CRITICAL)

**Location**: `zerion_client/src/lib.rs` lines 1070-1088

**The Bug**:
```rust
// Detect multi-hop swap: 3+ unique assets including stable currency
if unique_assets.len() >= 3 && has_stable {
    // Process ALL transfers individually (no implicit pricing)
    for transfer in trade_pair.in_transfers.iter().chain(trade_pair.out_transfers.iter()) {
        if let Some(event) = self.convert_transfer_to_event(tx, transfer, wallet_address, chain_id) {
            events.push(event);
        }
    }
    return events;
}
```

**What Happens**:
For transactions with 3+ unique tokens (e.g., swap TokenA → SOL → TokenB via Raydium):
1. Zerion records:
   - **IN**: 50 TokenB (received)
   - **OUT**: 100 TokenA (sold)
   - **OUT**: 0.5 SOL (routing fee or intermediary)

2. The code creates:
   - ✅ **BUY** event: 50 TokenB
   - ✅ **SELL** event: 100 TokenA
   - ❌ **SELL** event: 0.5 SOL ← **THIS IS WRONG!**

3. The SOL "SELL" is an intermediary transfer in the multi-hop swap, NOT a separate trade!

**Impact**:
- Creates spurious SELL events for intermediary tokens (SOL, USDC)
- These sells get matched against either:
  - Old legitimate BUY events → **Inflates total_returned_usd**
  - Implicit receives (pre-existing holdings) → **Inflates receive consumptions**

---

### Issue #2: Duplicate Event Creation in Complex Swaps

**Location**: `zerion_client/src/lib.rs` lines 1103-1188

**The Bug**:
When processing trade pairs with implicit swap pricing:
1. Lines 1108-1125: Find stable currency in OUT transfers, take `first()` IN transfer as volatile
2. Lines 1128-1145: If not found in OUT, find stable in IN transfers, take `first()` OUT transfer as volatile
3. Lines 1149-1171: Create events for stable + ONE volatile transfer
4. **Problem**: If there are MULTIPLE volatile transfers, only ONE is processed!

**Example Scenario**:
Transaction: Buy TokenB by selling both TokenA and SOL
- **OUT**: 100 TokenA, 10 SOL
- **IN**: 500 TokenB

What should happen:
- Combined sell value: TokenA ($100) + SOL ($10) = $110
- Buy: 500 TokenB at $0.22 each = $110
- Net: 1 BUY, 2 SELLs (or aggregate them)

What actually happens:
- Takes `first()` OUT transfer (TokenA)
- Creates SELL for TokenA only
- **Ignores** the SOL OUT transfer!
- If SOL OUT transfer is later processed separately → Creates duplicate/incorrect events

---

### Issue #3: Implicit Receive Inflation

**Location**: `pnl_core/src/new_pnl_engine.rs` lines 993-1021

**The Logic** (PHASE 3 of FIFO matching):
```rust
if remaining_sell_event.quantity > Decimal::ZERO {
    let implicit_receive = NewFinancialEvent {
        // Creates a "phantom receive" for pre-existing holdings
        transaction_hash: format!("implicit_receive_{}", remaining_sell_event.transaction_hash),
        usd_price_per_token: Decimal::ZERO, // No cost basis
        // ...
    };
    // Match sell against this implicit receive → Zero P&L
}
```

**When This Happens**:
- If a SELL event has no matching BUY events (from Issues #1 or #2 above)
- System creates an "implicit receive" (assumes tokens were held before analysis period)
- Sell is matched against this receive → **Zero P&L impact**

**The Compound Effect**:
1. Issue #1 creates spurious SELL events (multi-hop intermediaries)
2. These sells can't find matching BUYs
3. System creates implicit receives
4. Original legitimate SELL events might match against wrong BUYs
5. **Result**: Total invested gets inflated, P&L gets distorted

---

### Issue #4: Total Invested Calculation Includes Implicitly Received Positions

**Location**: `pnl_core/src/new_pnl_engine.rs` lines 663-668 and 1094-1162

**The Code**:
```rust
// Line 663-668: Calculate total invested
let total_invested_usd: Decimal = events
    .iter()
    .filter(|e| e.event_type == NewEventType::Buy)
    .filter(|e| !e.transaction_hash.starts_with("phantom_buy_")) // Exclude phantom buys
    .map(|e| e.usd_value)
    .sum();
```

**The Problem**:
This sums ALL buy events from the original events list, which includes:
- Legitimate BUY events from actual purchases ✅
- BUY events from multi-hop swap intermediaries (Issue #1) ❌
- Duplicated BUY events (Issue #2) ❌

**But wait**: The calculation at lines 1094-1162 for remaining position uses `buy_events` which gets MUTATED during FIFO matching. However, the `total_invested_usd` calculation uses the ORIGINAL immutable `events` list!

**Remaining Position**:
```rust
// Lines 1094-1104: Calculates bought_quantity from MUTATED buy_events
let bought_quantity: Decimal = remaining_buys
    .iter()
    .filter(|e| !e.transaction_hash.starts_with("phantom_buy_"))
    .map(|e| e.quantity)
    .sum();
```

**The Math**:
- Remaining position cost basis = Sum of UNMATCHED buy events' usd_value
- If many buy events are erroneous (from Issues #1-2), they remain UNMATCHED
- These inflate the remaining position cost basis → Inflates total_invested_usd

---

## Specific Example Walkthrough

**Wallet**: `token_6AiuSc3pYM5rpbKPq8JfSQoMoUsfDpzvP5PmMSEKpump_analysis.json`

**Transaction 1**: User swaps SOL for Solano token (3 transfers due to multi-hop routing)
- Zerion records:
  - IN: 22,000,000 Solano
  - OUT: 500 SOL (to AMM)
  - IN: 10 SOL (routing refund)

**What the buggy code does**:
1. Detects 3 unique assets → Multi-hop swap logic triggers
2. Creates:
   - BUY: 22,000,000 Solano @ $0.0006 = $13,200 ❌ (Uses inflated implicit price)
   - SELL: 500 SOL
   - BUY: 10 SOL ❌ (Routing refund treated as separate buy!)
3. Later, the SOL routing refund creates a spurious BUY event with inflated price
4. This BUY never gets matched → Stays in remaining position → Inflates total_invested

**Actual legitimate trades** (3 trades shown in matched_trades):
- Only $798 of real investment
- The remaining $13,073 is from erroneous parsing!

---

## Recommended Fixes

### Fix #1: Improve Multi-Hop Swap Detection

**File**: `zerion_client/src/lib.rs` lines 1070-1088

**Current approach**: Processes ALL transfers individually when 3+ assets detected
**Problem**: Treats intermediary transfers as separate trades

**Solution**:
```rust
// Instead of processing ALL transfers, identify and EXCLUDE intermediaries
if unique_assets.len() >= 3 && has_stable {
    info!("Multi-hop swap detected - using net transfer analysis");

    // Calculate NET transfers per token
    let mut net_transfers: HashMap<String, (Decimal, Option<f64>)> = HashMap::new();

    for transfer in trade_pair.in_transfers.iter() {
        // Add to net (positive)
        // Track: (quantity, value)
    }

    for transfer in trade_pair.out_transfers.iter() {
        // Subtract from net (negative)
    }

    // Only create events for tokens with NON-ZERO net transfers
    // Ignore tokens where net ≈ 0 (intermediaries)
}
```

### Fix #2: Handle Multiple Volatile Transfers in Implicit Pricing

**File**: `zerion_client/src/lib.rs` lines 1103-1171

**Solution**: Instead of taking `first()` volatile transfer, process ALL volatile transfers:

```rust
// After finding stable_transfer and stable_value...

// Process ALL volatile transfers (not just first)
let volatile_transfers: Vec<&ZerionTransfer> = if stable_was_out {
    &trade_pair.in_transfers  // All IN transfers are volatile
} else {
    &trade_pair.out_transfers  // All OUT transfers are volatile
};

for volatile_xfer in volatile_transfers {
    if let Some(event) = self.convert_transfer_with_implicit_price(
        tx, volatile_xfer, wallet_address, chain_id, stable_value
    ) {
        events.push(event);
    }
}
```

### Fix #3: Add Net Transfer Validation

**File**: `pnl_core/src/new_pnl_engine.rs` - Add validation after parsing

```rust
// After parsing all events, validate that total BUYs ≈ total SELLs (per token)
for (token_address, events) in &events_by_token {
    let total_buy_value: Decimal = events.iter()
        .filter(|e| e.event_type == Buy)
        .map(|e| e.usd_value)
        .sum();

    let total_sell_value: Decimal = events.iter()
        .filter(|e| e.event_type == Sell)
        .map(|e| e.usd_value)
        .sum();

    // If buys >> sells with large remaining position, warn about possible parsing errors
    let imbalance_ratio = total_buy_value / total_sell_value.max(Decimal::ONE);
    if imbalance_ratio > Decimal::from(10) {
        warn!(
            "⚠️  Extreme buy/sell imbalance detected for {}: {} buy value vs {} sell value ({}x)",
            token_address, total_buy_value, total_sell_value, imbalance_ratio
        );
    }
}
```

### Fix #4: Add Debugging Output

**File**: `pnl_core/src/new_pnl_engine.rs` lines 663-668

```rust
// Enhanced logging for total_invested calculation
let buy_events_for_invested: Vec<&NewFinancialEvent> = events
    .iter()
    .filter(|e| e.event_type == NewEventType::Buy)
    .filter(|e| !e.transaction_hash.starts_with("phantom_buy_"))
    .collect();

debug!(
    "📊 Total Invested Calculation for {}: {} buy events",
    token_symbol,
    buy_events_for_invested.len()
);

for (i, event) in buy_events_for_invested.iter().enumerate() {
    debug!(
        "  Buy #{}: {} {} @ ${} = ${} (tx: {})",
        i + 1,
        event.quantity,
        event.token_symbol,
        event.usd_price_per_token,
        event.usd_value,
        &event.transaction_hash[..8]
    );
}

let total_invested_usd: Decimal = buy_events_for_invested.iter().map(|e| e.usd_value).sum();
```

---

## Testing Recommendations

1. **Create test cases for multi-hop swaps**:
   - Token A → SOL → Token B (3 transfers)
   - Token A + Token B → SOL → Token C (4 transfers)

2. **Validate against known wallets**:
   - Find wallets with simple, verifiable transactions
   - Manually calculate expected P&L
   - Compare against system output

3. **Add assertions**:
   - Total BUYs ≈ Total SELLs + Remaining Position (within reasonable tolerance)
   - No single token should have >10x buy/sell value imbalance without user holding large position

---

## Priority

**CRITICAL** - These bugs cause fundamental P&L miscalculations. Fix immediately before production use.

---

## Next Steps

1. Implement Fix #1 (multi-hop swap detection) - **HIGHEST PRIORITY**
2. Implement Fix #2 (multiple volatile transfers)
3. Add Fix #3 (validation) for early warning
4. Add Fix #4 (debugging) for transparency
5. Re-test problematic wallets
6. Validate against reference implementation (JS/TS version)

