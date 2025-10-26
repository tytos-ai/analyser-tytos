# Analysis: Zerion Edge Cases Causing Incorrect total_invested

**Date:** 2025-10-26
**Insight from User:** Zerion's intermediate event creation and value mismatches
**Status:** Analysis only - identifying specific edge cases

---

## Edge Case #1: Meme-to-Meme Direct Swaps

### The Problem

**User scenario:** Swap TokenA directly to TokenB (no SOL/USDC in reality)

**What Zerion does:** Creates intermediate events to show routing
```
Blockchain: ONE transaction (TokenA → TokenB direct swap)

Zerion representation:
├─ Event 1: SELL TokenA → get 100 SOL
└─ Event 2: BUY TokenB with 100 SOL
```

**What parser does:**
```rust
// Processes as TWO separate trades
Trade 1 (TokenA):
- SELL 1M TokenA @ $0.0001 = $100
- Records as: realized P&L from selling TokenA

Trade 2 (TokenB):
- BUY 2M TokenB @ $0.00005 = $100
- Records as: $100 invested in TokenB ❌

Result:
- TokenA: shows $100 returned (correct if it was initial investment)
- TokenB: shows $100 invested (WRONG - this wasn't new capital!)
- total_invested inflated by $100
```

### Where This Happens in Code

**File:** `zerion_client/src/lib.rs`

**Trade Pairing** (lines 1016-1050):
```rust
fn pair_trade_transfers<'a>(transfers: &'a [ZerionTransfer]) -> Vec<TradePair<'a>> {
    // Groups by act_id
    // If meme-to-meme swap has TWO act_ids → creates TWO trade pairs
    // Each processed separately
}
```

**Problem:**
- Zerion might assign different `act_id` to each leg of the swap
- Parser treats them as independent trades
- No logic to detect "this BUY is actually a reinvestment from a SELL"

### The Pattern to Detect

**Characteristics of meme-to-meme swap:**
1. Same transaction_hash (or very close timestamps)
2. SELL event (meme coin) followed by BUY event (different meme coin)
3. USD values roughly match (accounting for slippage/fees)
4. Intermediate stable currency (SOL/USDC) appears in between

**Example:**
```
tx: abc123... @ 10:30:00
├─ SELL 1M TokenA @ $0.0001 = $100 (stable OUT: 100 USDC)
└─ BUY 2M TokenB @ $0.00005 = $100 (stable IN: 100 USDC)

This is NOT:
- $100 invested originally ← Only count this
- $100 reinvested in TokenB ← Don't count this

This IS:
- ONE investment of $100 that moved from TokenA to TokenB
```

### Current Parser Behavior

**Implicit Pricing Path:**
```rust
// Line 1342: Found stable side, one volatile transfer
// Creates ONE event per volatile token

If swap has:
- 1 volatile OUT (TokenA)
- 1 volatile IN (TokenB)
- Stable currencies in between

Then creates:
- SELL event for TokenA with stable_out_value
- BUY event for TokenB with stable_in_value

Both count towards their respective totals ❌
```

**Standard Conversion Path:**
```rust
// Line 1399-1403: Process all transfers normally
// No awareness that OUT and IN are related
```

---

## Edge Case #2: Stable/Volatile Value Mismatches

### The Problem

**User observation:** "The USDC/SOL amount doesn't match the token value"

**Example from real data:**
```
Trade pair:
├─ 3 SOL transfers OUT: $800 total (stable side)
└─ 1 TokenX transfer IN: value=$600 in Zerion (volatile side)

Discrepancy: $800 vs $600 = $200 difference
```

**Where does the $200 go?**
1. **Fees** - DEX fees, network fees (most likely)
2. **Slippage** - Price moved during swap
3. **Zerion aggregation error** - Wrong calculation
4. **Multi-hop routing** - Actual path was more complex

### Current Parser Logic (Implicit Pricing)

**File:** `zerion_client/src/lib.rs:1249-1301`

```rust
// Sums ALL stable currency transfers
let stable_out_total: f64 = trade_pair.out_transfers.iter()
    .filter_map(|transfer| {
        if Self::is_stable_currency(address) {
            transfer.value  // Uses Zerion's value
        }
    })
    .sum();

// Then uses this TOTAL for volatile token pricing
let implicit_price = stable_side_value_usd / quantity_f64;
usd_value: Decimal::from_f64_retain(stable_side_value_usd)
```

**The Issue:**
- Uses stable side TOTAL value ($800)
- Ignores volatile side value ($600) if it exists
- Assumes stable side is accurate (might not be!)
- Doesn't account for fees embedded in stable transfers

**Why stable side might be higher:**
```
Real swap: Bought 1M TokenX for $600
Blockchain transfers show:
├─ 0.05 SOL OUT ($150) - to DEX
├─ 0.02 SOL OUT ($60) - to fee account
├─ 0.02 SOL OUT ($60) - to another fee account
└─ 530 SOL OUT ($530) - actual swap amount
Total: $800

But only $530 actually went to buying TokenX!
$270 was fees and routing overhead.

Parser counts $800 as investment ❌
Should count $530 or $600 (volatile side value) ✓
```

### Where This Happens in Code

**Stable Side Aggregation** (lines 1249-1264):
```rust
let stable_out_total: f64 = trade_pair.out_transfers.iter()
    .filter_map(|transfer| {
        // Checks if token is stable (SOL, USDC, etc.)
        if Self::is_stable_currency(address) {
            transfer.value  // ← Includes EVERYTHING, even fees
        }
    })
    .sum();
```

**No validation against volatile side:**
```rust
// Line 1548: Sets usd_value to stable side total
usd_value: Decimal::from_f64_retain(stable_side_value_usd)

// Doesn't check:
// - Does volatile transfer have its own value?
// - If yes, does it match stable side?
// - If mismatch, which one is correct?
```

### The Pattern to Detect

**Characteristics of mismatched values:**
1. Stable side total > volatile side value (common)
2. Difference > 5% of total (significant mismatch)
3. Multiple small stable transfers (likely fees)
4. Volatile transfer has explicit value in Zerion

**Example detection:**
```rust
stable_total = $800
volatile_value = $600  // From transfer.value
difference = $200 (25% mismatch)

Heuristic:
if difference > max(5% of stable_total, $10):
    warn!("Significant value mismatch detected")
    // Which to use?
    // Option 1: Use volatile value (actual token value)
    // Option 2: Use stable average (stable_total / count, exclude outliers)
    // Option 3: Use minimum of both (conservative)
```

---

## Edge Case #3: Fee Transfers Included in Stable Side

### The Problem

**SOL fees are stable currency** → included in stable_side_value

**Example:**
```
Trade pair (act_id: 123):
├─ OUT: 0.5 SOL to TokenX pool ($150) - actual swap
├─ OUT: 0.001 SOL to fee account ($0.30) - network fee
├─ OUT: 0.002 SOL to DEX ($0.60) - DEX fee
└─ IN: 1M TokenX (volatile)

Current calculation:
stable_side_total = $150 + $0.30 + $0.60 = $150.90
usd_value for TokenX BUY = $150.90 ❌

Correct calculation:
Only the swap amount = $150
Fees should be excluded ✓
```

### How to Identify Fees

**Fee transfer characteristics:**
1. **Very small amounts** (< $5 typically)
2. **Different direction from main flow**
3. **System addresses** (known fee collector addresses)
4. **Consistent values** (e.g., always 0.000005 SOL)

**Current parser:** No fee detection logic

**Needed:**
```rust
fn is_likely_fee_transfer(transfer: &ZerionTransfer) -> bool {
    // Check 1: Very small USD value
    if let Some(value) = transfer.value {
        if value < 5.0 {  // Less than $5
            // Check 2: Common fee amounts
            let qty = parse_quantity(&transfer.quantity.numeric);
            if qty == 0.000005 || qty == 0.0001 || qty == 0.002 {
                return true;
            }
        }
    }
    false
}
```

---

## Edge Case #4: Multi-Hop with Multiple Volatile Tokens

### The Problem

**Complex routing:** TokenA → TokenB → TokenC

**Zerion representation:**
```
Transaction XYZ:
├─ OUT: 1M TokenA
├─ IN: 500K TokenB (intermediate)
├─ OUT: 500K TokenB (intermediate)
└─ IN: 2M TokenC
```

**Current parser:**
```rust
// Multi-hop detection (lines 1070-1220)
// Uses net transfer analysis
// Filters intermediaries with net_qty < 0.001

For TokenB:
net_qty = 500K IN - 500K OUT = 0 (filtered out) ✓

Creates events:
- SELL TokenA
- BUY TokenC

Looks correct!
```

**But what if intermediary isn't perfectly balanced?**
```
├─ IN: 500K TokenB
└─ OUT: 499K TokenB (1K difference due to rounding/slippage)

net_qty = 1K (> 0.001 threshold)
Parser creates BUY event for 1K TokenB ❌

Now total_invested includes:
- Original TokenA investment
- Phantom 1K TokenB investment from rounding error
```

---

## Edge Case #5: Same Token on Both Sides (Aggregation Trades)

### The Problem

**Aggregation trade:** Buying same token from multiple sources

**Example:**
```
Transaction: Buy TokenX from 3 DEXs simultaneously
├─ OUT: 100 USDC to Raydium → IN: 500K TokenX
├─ OUT: 100 USDC to Orca → IN: 480K TokenX
└─ OUT: 100 USDC to Jupiter → IN: 520K TokenX

Total: 300 USDC spent, 1.5M TokenX bought
```

**If same act_id:** Parser groups correctly
```rust
stable_side_total = $300 ✓
volatile_transfers.len() = 3 ❌

Line 1387: "Multiple volatile transfers - skipping implicit pricing"
Falls back to standard conversion
```

**Standard conversion behavior:**
```rust
// Processes each transfer individually
for transfer in trade_pair.in_transfers {
    // TokenX transfer 1: value=$100
    // TokenX transfer 2: value=$100
    // TokenX transfer 3: value=$100
    // Creates 3 BUY events, each with Zerion's value
}

If each transfer.value = $100:
total_invested = $300 ✓

But if Zerion doesn't set values and parser calculates:
Each might get stable_total / 3 = $100 ✓
OR each might get full stable_total = $300 ❌ ← BUG
```

**Check needed:** Line 1399-1403 standard conversion
- Does it use transfer.value from Zerion?
- Or does it recalculate somehow?

---

## Summary of Edge Cases

### 🔴 CRITICAL: Meme-to-Meme Swaps (Edge Case #1)
**Impact:** Double-counts investment when tokens are swapped
**Frequency:** HIGH (common trading pattern)
**Detection:** Same tx has SELL + BUY with matching USD values
**Fix needed:** Track "reinvestment" vs "new capital"

### 🔴 CRITICAL: Stable/Volatile Mismatches (Edge Case #2)
**Impact:** Inflates investment by including fees in stable side
**Frequency:** MEDIUM (happens with complex swaps)
**Detection:** |stable_total - volatile_value| > 5% threshold
**Fix needed:** Prefer volatile value or exclude fee transfers

### 🟡 MAJOR: Fee Transfers Counted (Edge Case #3)
**Impact:** Adds $1-20 per transaction (small but accumulates)
**Frequency:** HIGH (every swap has fees)
**Detection:** Small SOL/USDC transfers (< $5)
**Fix needed:** Filter fee transfers from stable_side_total

### 🟡 MAJOR: Multi-Hop Rounding Errors (Edge Case #4)
**Impact:** Creates phantom investments from rounding differences
**Frequency:** LOW (only when intermediary not perfectly balanced)
**Detection:** Net transfer close to zero but > threshold
**Fix needed:** Widen net_threshold or ignore dust amounts

### 🟢 MINOR: Aggregation Trades (Edge Case #5)
**Impact:** Depends on how standard conversion handles it
**Frequency:** LOW (sophisticated trading)
**Detection:** Multiple volatile transfers of SAME token
**Fix needed:** Verify standard conversion doesn't multiply values

---

## Root Cause: Parser Trusts Zerion's Structure Too Much

**Fundamental issue:**
- Parser assumes Zerion's transfer groupings are semantic
- But Zerion optimizes for UI display, not accounting accuracy
- Intermediate events are visual aids, not real trades
- Transfer values include routing overhead, not just swap amounts

**What parser needs:**
1. **Semantic understanding** of what's a real trade vs routing
2. **Value validation** between stable and volatile sides
3. **Fee detection** to exclude overhead from investment
4. **Reinvestment tracking** to avoid double-counting swaps

---

## Diagnostic Questions for User

To confirm these are the issues:

1. **For meme-to-meme swaps:**
   - Do you see cases where SELL TokenA + BUY TokenB in same transaction?
   - Are both counted in total_invested?
   - Should only the original TokenA purchase count?

2. **For value mismatches:**
   - Can you provide example where stable side > volatile side significantly?
   - Does the volatile side value seem more accurate?
   - Are there multiple small SOL transfers that look like fees?

3. **For your specific problem tokens:**
   - Can you share transaction hashes?
   - What's the expected total_invested?
   - What's the actual total_invested shown?
   - What's the difference?

---

**Next step:** User should provide specific examples so I can trace exact parser behavior and propose targeted fixes.
