# Multi-Hop Swap P&L Deviation Fix Plan

## Problem Summary

There is a small deviation in P&L calculations for tokens purchased through multi-hop swaps (e.g., USDC → SOL → Token). The deviation occurs because:

1. **total_invested** correctly uses `swap_input_usd_value` (actual $ spent) after a previous fix
2. **realized_pnl** calculation still uses `usd_price_per_token` (based on market value) instead of actual cost

This causes realized_pnl to be slightly inflated for multi-hop swap purchases.

## Root Cause Analysis

### Location: `/home/mrima/tytos/wallet-analyser/pnl_core/src/new_pnl_engine.rs`

**Line 965** - Realized P&L Calculation:
```rust
let realized_pnl = (remaining_sell_event.usd_price_per_token - buy_event.usd_price_per_token) * matched_quantity;
```

**Problem**: Uses `buy_event.usd_price_per_token` which is calculated from market value (`usd_value / quantity`), not actual spend amount.

### Example Scenario:

Multi-hop swap: USDC → SOL → Token
- **Actual spend**: $105 USDC
- **Market value of tokens received**: $100
- **Quantity**: 1000 tokens

Current state:
- `swap_input_usd_value` = $105 (actual spend) ✓
- `usd_value` = $100 (market value)
- `usd_price_per_token` = $100 / 1000 = **$0.10** ← Used in P&L calc (WRONG)
- **Actual cost per token** = $105 / 1000 = **$0.105** ← Should be used

When selling at $0.12:
- **Current P&L**: ($0.12 - $0.10) × 1000 = **$20** (inflated by $5)
- **Correct P&L**: ($0.12 - $0.105) × 1000 = **$15**

## Affected Code Sections

### 1. Line 965 - Primary Issue
Realized P&L calculation doesn't account for multi-hop actual cost.

### 2. Lines 972-988 - Matched Buy Portion Creation
When creating `matched_buy_portion` for partial matches, `swap_input_usd_value` is copied as-is without proportional adjustment:
```rust
let matched_buy_portion = NewFinancialEvent {
    // ...
    quantity: matched_quantity,  // Only partial quantity
    usd_price_per_token: buy_event.usd_price_per_token,
    usd_value: matched_quantity * buy_event.usd_price_per_token,
    swap_input_usd_value: buy_event.swap_input_usd_value,  // ← NOT proportionally adjusted
    // ...
};
```

### 3. Lines 1019-1020 - Remaining Buy Event Update
When updating remaining buy quantities, `usd_value` is recalculated but `swap_input_usd_value` is not:
```rust
buy_event.quantity -= matched_quantity;
buy_event.usd_value = buy_event.quantity * buy_event.usd_price_per_token;
// Missing: proportional update to swap_input_usd_value
```

## Proposed Fix

### Fix 1: Use Actual Cost Per Token in P&L Calculation (Line 965)

```rust
// Calculate the true cost per token for this buy event
let buy_cost_per_token = if let Some(swap_input_value) = buy_event.swap_input_usd_value {
    // Multi-hop swap: use actual cost per token (what was really spent)
    swap_input_value / (buy_event.quantity + matched_quantity)  // Note: quantity already reduced from previous matches
} else {
    // Regular buy: use market-based price
    buy_event.usd_price_per_token
};

let realized_pnl = (remaining_sell_event.usd_price_per_token - buy_cost_per_token) * matched_quantity;
```

**Alternative approach** (cleaner):
Store a separate `cost_basis_per_token` field that's calculated once when the event is created.

### Fix 2: Proportionally Adjust swap_input_usd_value for Partial Matches (Line 987)

```rust
// Calculate proportional swap input value if this is a multi-hop swap
let proportional_swap_input = buy_event.swap_input_usd_value.map(|swap_value| {
    (swap_value / (buy_event.quantity + matched_quantity)) * matched_quantity
});

let matched_buy_portion = NewFinancialEvent {
    // ...
    quantity: matched_quantity,
    usd_price_per_token: buy_event.usd_price_per_token,
    usd_value: matched_quantity * buy_event.usd_price_per_token,
    swap_input_usd_value: proportional_swap_input,  // ← Proportionally adjusted
    // ...
};
```

### Fix 3: Update Remaining swap_input_usd_value (Line 1019)

```rust
buy_event.quantity -= matched_quantity;
buy_event.usd_value = buy_event.quantity * buy_event.usd_price_per_token;

// Proportionally reduce swap_input_usd_value for multi-hop swaps
if let Some(swap_input) = buy_event.swap_input_usd_value {
    let proportion_remaining = buy_event.quantity / (buy_event.quantity + matched_quantity);
    buy_event.swap_input_usd_value = Some(swap_input * proportion_remaining);
}
```

## Testing Plan

### Phase 1: Pre-Fix Baseline
1. Submit batch job for wallet: `6qWUokEUNc9Tcpn43viUyt41BarU47BtRqDSHmw8Znzu`
2. Extract and save results for token: `8qGi6DpLzGs16PBTrEqfvRJP7jLePAYQnQG36oPjG7Cw` (Kitty)
3. Record baseline metrics:
   - `total_invested_usd`
   - `total_returned_usd`
   - `realized_pnl_usd`
   - `unrealized_pnl_usd`
   - `total_pnl_usd`
   - Number of multi-hop swap adjustments

### Phase 2: Apply Fix
1. Implement Fix 1 (primary P&L calculation)
2. Implement Fix 2 (proportional swap_input for matched portions)
3. Implement Fix 3 (update remaining swap_input)
4. Add debug logging to show:
   - When multi-hop cost basis is used
   - Difference between market price and actual cost per token
   - Impact on realized_pnl calculation

### Phase 3: Post-Fix Comparison
1. Submit same batch job again
2. Extract Kitty token results
3. Compare metrics:
   - `total_invested_usd` should remain the same ✓
   - `realized_pnl_usd` should be slightly lower (more accurate)
   - `total_pnl_usd` should be more accurate
4. Verify the deviation is eliminated

### Expected Outcome
- `total_invested` should remain unchanged (already correct)
- `realized_pnl` should decrease slightly for tokens bought via multi-hop swaps
- `total_pnl` should be more accurate (deviation eliminated)
- The difference should match the cumulative slippage from multi-hop swaps

## Implementation Notes

### Consideration: Store cost_basis_per_token Field
Instead of calculating on-the-fly, consider adding a `cost_basis_per_token` field to `NewFinancialEvent`:
```rust
pub struct NewFinancialEvent {
    // ... existing fields ...

    /// True cost basis per token (uses swap_input for multi-hop, usd_price_per_token otherwise)
    /// This is what should be used for P&L calculations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_basis_per_token: Option<Decimal>,
}
```

Benefits:
- Cleaner code
- Single source of truth
- No repeated calculations
- Easier to debug

### Alternative: Calculate at Event Creation
Could calculate and cache the true cost basis when events are first created in the parser, avoiding repeated calculations during matching.

## Risk Assessment

**Low Risk** - Changes are isolated to P&L calculation logic and don't affect:
- Event parsing
- FIFO matching logic
- Order of operations
- Total invested calculation (already fixed)

**Testing Coverage Needed**:
- Tokens with only regular buys (no multi-hop) → Should see NO change
- Tokens with multi-hop buys → Should see corrected P&L
- Tokens with mix of regular and multi-hop buys → Partial correction

## Files to Modify

1. `/home/mrima/tytos/wallet-analyser/pnl_core/src/new_pnl_engine.rs`
   - Lines 965: Primary P&L calculation fix
   - Lines 972-988: Proportional swap_input for matched portions
   - Lines 1019-1020: Update remaining swap_input

Optional:
2. `/home/mrima/tytos/wallet-analyser/pnl_core/src/new_parser.rs`
   - Add `cost_basis_per_token` field to `NewFinancialEvent` struct
   - Calculate at event creation time

## Success Criteria

1. ✓ P&L calculations use actual cost basis for multi-hop swaps
2. ✓ Deviation between expected and actual P&L is eliminated
3. ✓ total_invested remains unchanged (already correct)
4. ✓ Proportional adjustments work correctly for partial matches
5. ✓ No regression for regular (non-multi-hop) purchases
6. ✓ Debug logs show when multi-hop cost basis is applied

## Timeline

- **Phase 1** (Baseline): ~5 minutes
- **Phase 2** (Implementation): ~20 minutes
- **Phase 3** (Testing): ~5 minutes
- **Total**: ~30 minutes

---

**Status**: Plan created, awaiting execution
**Date**: 2025-10-27
**Wallet for Testing**: 6qWUokEUNc9Tcpn43viUyt41BarU47BtRqDSHmw8Znzu
**Token for Comparison**: 8qGi6DpLzGs16PBTrEqfvRJP7jLePAYQnQG36oPjG7Cw (Kitty)
