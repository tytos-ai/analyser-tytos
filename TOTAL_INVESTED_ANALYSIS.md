# Analysis: Incorrect total_invested Values

**Date:** 2025-10-26
**Issue:** Some instances show incorrect `total_invested_usd` values
**Scope:** Analysis only - no changes made

---

## How total_invested is Calculated

**File:** `pnl_core/src/new_pnl_engine.rs` (lines 676-700)

```rust
// 1. Filter for BUY events, excluding phantom buys
let buy_events_for_invested: Vec<&NewFinancialEvent> = events
    .iter()
    .filter(|e| e.event_type == NewEventType::Buy)
    .filter(|e| !e.transaction_hash.starts_with("phantom_buy_"))
    .collect();

// 2. Sum up the usd_value field of all BUY events
let total_invested_usd: Decimal = buy_events_for_invested
    .iter()
    .map(|e| e.usd_value)
    .sum();
```

**Logic:**
- Takes ALL BUY events (excluding phantom buys)
- Sums the `usd_value` field
- Does NOT recalculate, just sums existing values

**Therefore:** If `total_invested` is wrong, the problem is in the `usd_value` field of BUY events.

---

## Where usd_value is Set for BUY Events

### Path 1: Implicit Pricing (Multi-hop Swaps)
**File:** `zerion_client/src/lib.rs` (line 1548)

```rust
Some(NewFinancialEvent {
    // ...
    usd_value: Decimal::from_f64_retain(stable_side_value_usd).unwrap_or(Decimal::ZERO),
    // ...
})
```

**What is stable_side_value_usd?**
- It's the TOTAL USD value of ALL stable currency transfers in the trade pair
- For a BUY transaction: This is how much SOL/USDC you spent
- Calculated by summing all stable transfers (lines 1249-1301)

**Potential Issues:**

1. **Multiple volatile transfers counted separately**
   ```
   Example: Transaction has 3 volatile IN transfers
   - Transfer 1: 12M tokens → creates BUY event with usd_value = $800
   - Transfer 2: 700K tokens → creates BUY event with usd_value = $800
   - Transfer 3: 9.5M tokens → creates BUY event with usd_value = $800
   Total invested: $2,400 instead of $800 ❌
   ```
   **Status:** SHOULD BE PREVENTED by code at line 1339 (only 1 volatile transfer allowed)
   **But:** If code has bugs or edge cases, could still happen

2. **Stable side value calculated wrong**
   - If stable transfers have wrong values in Zerion data
   - If aggregation misses some transfers or counts extras
   - Precision loss from f64 conversion

3. **Same transaction processed multiple times**
   - Different act_id groups for same blockchain transaction
   - Could create multiple BUY events with same stable_side_value

### Path 2: Standard Conversion
**File:** `zerion_client/src/lib.rs` (lines 1908-1965)

```rust
let (usd_price_per_token, usd_value) = match (transfer.price, transfer.value) {
    (Some(price), Some(value)) => {
        // Use Zerion's value directly
        (Decimal::from_f64_retain(price).unwrap_or(Decimal::ZERO),
         Decimal::from_f64_retain(value).unwrap_or(Decimal::ZERO))
    }
    (Some(price), None) => {
        // Calculate: value = price * quantity
        let calculated_value = price * amount.to_f64().unwrap_or(0.0);
        (Decimal::from_f64_retain(price).unwrap_or(Decimal::ZERO),
         Decimal::from_f64_retain(calculated_value).unwrap_or(Decimal::ZERO))
    }
    (None, Some(value)) => {
        // Use value directly
        (calculated_price,
         Decimal::from_f64_retain(value).unwrap_or(Decimal::ZERO))
    }
}
```

**Potential Issues:**

1. **Zerion data wrong**
   - `transfer.value` field in Zerion API could be incorrect
   - Zerion might have bugs or stale data

2. **Calculation precision loss**
   ```rust
   let calculated_value = price * amount.to_f64().unwrap_or(0.0);
   ```
   - Converts Decimal → f64 → multiply → f64 → Decimal
   - Loses precision at each conversion
   - Could accumulate errors over many events

3. **Fee transfers counted as BUY events**
   - SOL fees might have direction="in"
   - Could be classified as BUY instead of fee
   - Would inflate total_invested with fee amounts

---

## Root Causes of Incorrect total_invested

### ❌ CRITICAL: Multiple Events from One Transaction

**Problem:** Same blockchain transaction creates multiple BUY events

**Example from previous analysis:**
```
Transaction: AvQb9vkf... (Oct 23 11:42:43)
Creates 3 BUY events:
- BUY 12,005,534 @ $0.0000341371 = $409.83
- BUY 702,739 @ $0.0000385638 = $27.10
- BUY 9,498,861 @ $0.0000385638 = $366.31
Total: $803.24 for ONE transaction

If each has usd_value = $800:
Total invested = $2,400 instead of $800 ❌
```

**Where This Happens:**
1. **Implicit pricing with 1 volatile transfer** (line 1339-1361)
   - Should only create ONE event
   - **Check:** Is volatile_transfers.len() actually 1? Or multiple?

2. **Standard conversion of multi-transfer trade pairs**
   - Processes ALL transfers (line 1399-1403)
   - If 3 volatile IN transfers → creates 3 BUY events
   - Each gets its own usd_value from Zerion

**Investigation Needed:**
- Check logs for transactions with multiple BUY events
- Verify they have different quantities
- Check if they all have full stable_side_value OR individual values

---

### ❌ HIGH: Precision Loss in f64 Conversions

**Problem:** Converting Decimal → f64 → Decimal loses precision

**Locations:**
1. **Implicit pricing** (line 1548):
   ```rust
   usd_value: Decimal::from_f64_retain(stable_side_value_usd).unwrap_or(Decimal::ZERO)
   ```
   - `stable_side_value_usd` is f64
   - Loses precision when converting to Decimal

2. **Calculated value** (line 1922):
   ```rust
   let calculated_value = price * amount.to_f64().unwrap_or(0.0);
   ```
   - amount.to_f64() loses precision
   - Multiplication in f64 loses more precision
   - Converting back to Decimal doesn't recover it

**Impact:**
- For small values: negligible
- For large values or many events: could accumulate to significant error
- Example: $10,000.123456789 → f64 → $10,000.12345 (lost 4 decimals)

**Investigation Needed:**
- Compare `usd_value` vs `usd_price_per_token * quantity`
- Check if they match or have discrepancies
- Sum up discrepancies across all BUY events

---

### ❌ MEDIUM: Fee Transfers Misclassified as BUY

**Problem:** SOL fee transfers marked as BUY events

**How it could happen:**
```
Transaction with 3 transfers:
- 1 IN: 1M TokenX (volatile, actual buy)
- 2 IN: 0.001 SOL (fee refund, misclassified?)
- 3 IN: 0.001 SOL (fee refund, misclassified?)

If fee refunds have direction="in" and are stable currencies:
- They might be counted in stable_side_value
- Or create separate BUY events in standard conversion
```

**Investigation Needed:**
- Look for BUY events with very small quantities of SOL/WSOL
- Check if they're actually fees, not real buys
- Verify implicit pricing excludes them from stable_side_value

---

### ❌ MEDIUM: Duplicates Not Caught by Deduplication

**Deduplication logic** (job_orchestrator/src/lib.rs:1377-1396):
```rust
let existing_event_keys: HashSet<(String, String, String)> = financial_events
    .iter()
    .map(|e| (
        e.transaction_hash.clone(),
        e.token_address.clone(),
        format!("{:?}", e.event_type)
    ))
    .collect();
```

**What it catches:**
- Events with EXACT same (tx_hash, token_address, event_type)

**What it MISSES:**
- Same transaction with DIFFERENT timestamps (shouldn't happen but might)
- Same transaction with DIFFERENT quantities (like AvQb9vkf with 3 different quantities)
- Events from implicit pricing vs enrichment with SAME tx_hash but different usd_values

**Investigation Needed:**
- Check if deduplication actually runs (logs should show "filtered X duplicates")
- Check if enriched events have different usd_values than implicit pricing events
- Verify enrichment doesn't create events with different prices

---

### ❌ LOW: Enrichment with Different Prices

**Scenario:**
1. Parser creates BUY event via implicit pricing: usd_value = $800
2. Same transaction marked for enrichment (price=NULL in raw data)
3. BirdEye enriches it with different price
4. Enriched event: usd_value = $820 (different!)
5. Deduplication filters it out ✅
6. BUT: What if deduplication fails?

**Investigation Needed:**
- Check logs for enriched events that were filtered
- Verify their usd_values match the implicit pricing values
- If they don't match, deduplication is working
- If they do match, no issue

---

### ❌ LOW: Zerion Data Quality Issues

**Problem:** Zerion API returns wrong values

**Possible causes:**
- Bug in Zerion's backend
- Stale price data
- Wrong currency conversion (uses wrong exchange rate)
- Multi-transfer aggregation done incorrectly by Zerion

**Investigation Needed:**
- Compare Zerion values vs blockchain data (Solscan)
- For suspicious transactions, verify on-chain values
- Check if Zerion's `transfer.value` matches actual SOL spent

---

## Diagnostic Steps

### 1. Find Transactions with Inflated total_invested

**Look for:**
- Tokens where `total_invested >> total_sold` (e.g., 10x ratio)
- Warning in logs: "EXTREME BUY/SELL IMBALANCE detected"

**From logs (line 721-735):**
```
⚠️  EXTREME BUY/SELL IMBALANCE detected for [TOKEN]: $X buy vs $Y sell (Zx ratio)
    → This likely indicates a transaction parsing error (multi-hop swaps, duplicates, etc.)
    → Review transaction logs for this token to identify erroneous BUY events
```

### 2. Analyze BUY Events for Problem Tokens

**Check:**
- How many BUY events for the token?
- Do multiple events share same transaction_hash?
- Do they have same or different usd_values?
- Do they have same or different quantities?

**Example investigation:**
```
Token: Solano (6AiuSc3...)
BUY events:
1. tx: AvQb9vkf... @ 11:42:43, qty: 12M, usd_value: $409.83 ✓
2. tx: AvQb9vkf... @ 11:42:43, qty: 700K, usd_value: $27.10 ✓
3. tx: AvQb9vkf... @ 11:42:43, qty: 9.5M, usd_value: $366.31 ✓
Total: $803.24 from ONE transaction

Expected: ONE BUY event with total quantity 22M @ $803.24
Actual: THREE BUY events totaling $803.24

Conclusion: Parser creates separate events for each IN transfer
Issue: If implicit pricing sets usd_value = stable_side_value for EACH,
       would get 3x the actual investment
```

### 3. Check Implicit Pricing Logic

**Question:** When there are multiple volatile IN transfers, what happens?

**Code check needed:**
- Line 1268-1296: How are volatile_transfers collected?
- Line 1339: Does `volatile_transfers.len() == 1` prevent multi-event creation?
- OR: Does multi-transfer case (line 1387-1409) create multiple events?

### 4. Verify usd_value Consistency

**For each BUY event, check:**
```rust
expected_value = usd_price_per_token * quantity
actual_value = usd_value

if (expected_value - actual_value).abs() > $0.01:
    warn!("Value mismatch in BUY event!")
```

**This reveals:**
- Precision loss issues
- Calculation errors
- Data quality problems

---

## Summary of Potential Root Causes

### Most Likely (in order):

1. **Multiple BUY events from one transaction** ⭐⭐⭐⭐⭐
   - Each has correct individual usd_value
   - But summing them counts the same purchase multiple times
   - Example: 3 events from AvQb9vkf

2. **Precision loss in f64 conversions** ⭐⭐⭐⭐
   - Decimal → f64 → Decimal loses precision
   - Accumulates across many events
   - Could explain modest inflation (5-10%)

3. **Fee transfers counted as BUY** ⭐⭐⭐
   - SOL fee refunds with direction="in"
   - Counted in stable_side_value or as separate events
   - Would show as many small BUY events

4. **Enrichment duplicates** ⭐⭐
   - Deduplication should catch this
   - But if prices differ, might create separate events
   - Check logs for "filtered X duplicates"

5. **Zerion data quality** ⭐
   - Less likely (affects all users)
   - But possible for specific tokens/chains

---

## Recommended Investigation Process

### Step 1: Examine Logs for Problem Token
```
grep "total_invested" latestlog.txt | grep -A5 "IMBALANCE"
```

### Step 2: Extract All BUY Events for That Token
```
grep "BUY.*[TOKEN_SYMBOL]" latestlog.txt
```

### Step 3: Group by Transaction Hash
- Count events per transaction
- Check if multiple events from same tx
- Compare usd_values

### Step 4: Verify Against Blockchain
- Use Solscan to check actual SOL spent
- Compare with parser's calculated values
- Identify discrepancies

### Step 5: Check Parser Logic for That Transaction Type
- If multi-transfer swap, check implicit pricing path
- If standard conversion, check Zerion value usage
- Verify volatile_transfers.len() check

---

## What to Look For in User's Data

**User should provide:**
1. **Token address** with incorrect total_invested
2. **Expected value** (from manual calculation or Zerion UI)
3. **Actual value** from system output
4. **Transaction hashes** for all BUY events of that token

**Then investigate:**
- Do multiple BUY events share transaction hashes?
- Are usd_values correct individually but wrong when summed?
- Are there unexpected BUY events (fees, etc.)?
- Do precision checks show discrepancies?

---

**Status:** Analysis complete - awaiting user data for specific investigation
