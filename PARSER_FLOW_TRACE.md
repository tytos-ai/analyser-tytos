# Parser Flow Trace: Zerion Edge Cases

**Date:** 2025-10-26
**Purpose:** Detailed code flow analysis showing how Zerion edge cases cause incorrect total_invested
**Method:** Step-by-step trace through actual parser code paths

---

## Edge Case #1: Meme-to-Meme Swap - Detailed Trace

### Scenario
User swaps 1M TokenA → 2M TokenB (single blockchain transaction)

### What Zerion Returns

```json
{
  "id": "tx_abc123",
  "timestamp": "2025-10-26T10:00:00Z",
  "changes": [
    {
      "asset": {
        "fungible_info": { "name": "TokenA", "symbol": "TKNA" }
      },
      "quantity": { "numeric": "1000000" },
      "direction": "out",
      "operation_type": "trade",
      "value": 100.0,
      "price": 0.0001,
      "address_from": "user_wallet",
      "address_to": "raydium_pool",
      "metadata": { "act_id": "act_001" }
    },
    {
      "asset": {
        "fungible_info": { "name": "Solana", "symbol": "SOL" }
      },
      "quantity": { "numeric": "0.5" },
      "direction": "in",
      "operation_type": "trade",
      "value": 100.0,
      "price": 200.0,
      "address_from": "raydium_pool",
      "address_to": "user_wallet",
      "metadata": { "act_id": "act_001" }
    },
    {
      "asset": {
        "fungible_info": { "name": "Solana", "symbol": "SOL" }
      },
      "quantity": { "numeric": "0.5" },
      "direction": "out",
      "operation_type": "trade",
      "value": 100.0,
      "price": 200.0,
      "address_from": "user_wallet",
      "address_to": "raydium_pool",
      "metadata": { "act_id": "act_002" }
    },
    {
      "asset": {
        "fungible_info": { "name": "TokenB", "symbol": "TKNB" }
      },
      "quantity": { "numeric": "2000000" },
      "direction": "in",
      "operation_type": "trade",
      "value": 100.0,
      "price": 0.00005,
      "address_from": "raydium_pool",
      "address_to": "user_wallet",
      "metadata": { "act_id": "act_002" }
    }
  ]
}
```

**Key Observation:** Zerion creates TWO act_ids for ONE blockchain transaction

---

### Parser Code Flow

#### Step 1: Group Transfers by act_id
**File:** `zerion_client/src/lib.rs:1016-1050`

```rust
fn pair_trade_transfers<'a>(transfers: &'a [ZerionTransfer]) -> Vec<TradePair<'a>> {
    let mut pairs_map: HashMap<String, TradePair<'a>> = HashMap::new();

    for transfer in transfers {
        if transfer.operation_type == "trade" {
            let act_id = transfer.metadata.act_id.clone();

            pairs_map.entry(act_id).or_insert_with(...);

            if transfer.direction == "in" {
                pair.in_transfers.push(transfer);
            } else {
                pair.out_transfers.push(transfer);
            }
        }
    }

    pairs_map.into_values().collect()
}
```

**Result:**
```
TradePair #1 (act_id: "act_001"):
├─ OUT: 1M TokenA ($100)
└─ IN: 0.5 SOL ($100)

TradePair #2 (act_id: "act_002"):
├─ OUT: 0.5 SOL ($100)
└─ IN: 2M TokenB ($100)
```

**Problem Starts Here:** Two separate trade pairs from one swap!

---

#### Step 2: Process Each Trade Pair Independently
**File:** `zerion_client/src/lib.rs:989-1010`

```rust
for trade_pair in trade_pairs {
    // Each trade_pair processed completely separately
    // No awareness that act_001 and act_002 are related

    let pair_events = self.process_implicit_swap(
        tx,
        &trade_pair,
        wallet_address,
        chain_id,
    );

    events.extend(pair_events);
}
```

---

#### Step 3: Classify Transfers (Stable vs Volatile)
**File:** `zerion_client/src/lib.rs:1106-1220` (for each trade pair)

**Trade Pair #1 (act_001):**
```rust
// OUT transfers
TokenA (1M) → is_stable_currency? NO → volatile OUT
Total: 1 volatile OUT

// IN transfers
SOL (0.5) → is_stable_currency? YES → stable IN
Total: 0 volatile IN, 1 stable IN
```

**Trade Pair #2 (act_002):**
```rust
// OUT transfers
SOL (0.5) → is_stable_currency? YES → stable OUT
Total: 0 volatile OUT, 1 stable OUT

// IN transfers
TokenB (2M) → is_stable_currency? NO → volatile IN
Total: 1 volatile IN
```

---

#### Step 4: Calculate Implicit Pricing
**File:** `zerion_client/src/lib.rs:1249-1301`

**For Trade Pair #1:**
```rust
// Has: 1 volatile OUT, 1 stable IN
// Pattern: Selling volatile for stable

let stable_in_total: f64 = trade_pair.in_transfers.iter()
    .filter_map(|t| {
        if is_stable_currency(&t.asset.address) {
            t.value  // SOL transfer: $100
        }
    })
    .sum();
// stable_in_total = $100

let volatile_transfer = volatile_transfers[0]; // TokenA
let quantity = parse_quantity(&volatile_transfer.quantity.numeric); // 1M

let implicit_price = stable_in_total / quantity; // $100 / 1M = $0.0001
```

**For Trade Pair #2:**
```rust
// Has: 1 volatile IN, 1 stable OUT
// Pattern: Buying volatile with stable

let stable_out_total: f64 = trade_pair.out_transfers.iter()
    .filter_map(|t| {
        if is_stable_currency(&t.asset.address) {
            t.value  // SOL transfer: $100
        }
    })
    .sum();
// stable_out_total = $100

let volatile_transfer = volatile_transfers[0]; // TokenB
let quantity = parse_quantity(&volatile_transfer.quantity.numeric); // 2M

let implicit_price = stable_out_total / quantity; // $100 / 2M = $0.00005
```

---

#### Step 5: Create Financial Events
**File:** `zerion_client/src/lib.rs:1508-1581`

**Event from Trade Pair #1 (TokenA):**
```rust
NewFinancialEvent {
    event_type: NewEventType::Sell,  // direction OUT
    token_address: "TokenA_address",
    token_symbol: "TKNA",
    quantity: Decimal::from(1_000_000),
    usd_price_per_token: Decimal::from_f64(0.0001),
    usd_value: Decimal::from_f64(100.0),  // ← stable_in_total
    transaction_hash: "tx_abc123",
    timestamp: "2025-10-26T10:00:00Z",
    // ...
}
```

**Event from Trade Pair #2 (TokenB):**
```rust
NewFinancialEvent {
    event_type: NewEventType::Buy,  // direction IN
    token_address: "TokenB_address",
    token_symbol: "TKNB",
    quantity: Decimal::from(2_000_000),
    usd_price_per_token: Decimal::from_f64(0.00005),
    usd_value: Decimal::from_f64(100.0),  // ← stable_out_total ❌
    transaction_hash: "tx_abc123",
    timestamp: "2025-10-26T10:00:00Z",
    // ...
}
```

**Critical Issue:** `usd_value` for TokenB BUY is set to $100 (the stable_out_total)

---

#### Step 6: P&L Calculation
**File:** `pnl_core/src/new_pnl_engine.rs:676-700`

```rust
// Filter for BUY events
let buy_events: Vec<_> = events.iter()
    .filter(|e| e.event_type == NewEventType::Buy)
    .collect();

// buy_events contains:
// - BUY 2M TokenB, usd_value = $100

let total_invested_usd: Decimal = buy_events.iter()
    .map(|e| e.usd_value)
    .sum();
// total_invested_usd = $100 for TokenB
```

---

### The Problem Revealed

**What Actually Happened on Blockchain:**
- User spent $100 total
- Swapped TokenA → TokenB directly
- This is a REINVESTMENT, not new capital

**What Parser Recorded:**
```
SELL TokenA:
- Quantity: 1M
- USD Value: $100
- Interpretation: "Got $100 back from selling TokenA"

BUY TokenB:
- Quantity: 2M
- USD Value: $100 ✓ (correct individual value)
- Interpretation: "Invested $100 in TokenB"

total_invested for TokenB = $100 ❌
```

**Why It's Wrong:**
- The $100 used to buy TokenB came FROM selling TokenA
- It's not new capital investment
- If TokenA was originally bought for $100, then:
  - Total capital invested = $100 (original TokenA purchase)
  - Current position = 2M TokenB
  - But system shows: total_invested = $100 for TokenB (missing the original TokenA buy context)

**If TokenA was also bought in analysis timeframe:**
```
Events:
1. BUY 1M TokenA @ $100 (total_invested += $100)
2. SELL 1M TokenA @ $100 (matched against buy, realized P&L = $0)
3. BUY 2M TokenB @ $100 (total_invested += $100) ❌

total_invested = $200 instead of $100!
```

This is the **double-counting via reinvestment** that user described.

---

## Edge Case #2: Stable/Volatile Value Mismatch - Detailed Trace

### Scenario
User buys 1M TokenX, but Zerion data shows:
- Stable side (SOL transfers OUT): $800 total
- Volatile side (TokenX transfer IN): $600 value

### What Zerion Returns

```json
{
  "changes": [
    {
      "asset": { "fungible_info": { "symbol": "SOL" } },
      "quantity": { "numeric": "3" },
      "direction": "out",
      "operation_type": "trade",
      "value": 600.0,
      "metadata": { "act_id": "act_123" }
    },
    {
      "asset": { "fungible_info": { "symbol": "SOL" } },
      "quantity": { "numeric": "0.5" },
      "direction": "out",
      "operation_type": "trade",
      "value": 100.0,
      "metadata": { "act_id": "act_123" }
    },
    {
      "asset": { "fungible_info": { "symbol": "SOL" } },
      "quantity": { "numeric": "0.5" },
      "direction": "out",
      "operation_type": "trade",
      "value": 100.0,
      "metadata": { "act_id": "act_123" }
    },
    {
      "asset": { "fungible_info": { "symbol": "TokenX" } },
      "quantity": { "numeric": "1000000" },
      "direction": "in",
      "operation_type": "trade",
      "value": 600.0,
      "price": 0.0006,
      "metadata": { "act_id": "act_123" }
    }
  ]
}
```

**Reality:**
- 3 SOL ($600) went to actual swap
- 0.5 SOL ($100) went to DEX fee account
- 0.5 SOL ($100) went to routing account
- User got 1M TokenX worth $600

---

### Parser Code Flow

#### Step 1: Group by act_id
```
TradePair (act_id: "act_123"):
├─ OUT: 3 SOL ($600)
├─ OUT: 0.5 SOL ($100)
├─ OUT: 0.5 SOL ($100)
└─ IN: 1M TokenX ($600)
```

---

#### Step 2: Classify Transfers

```rust
// OUT transfers
SOL 3.0 → is_stable? YES → stable OUT
SOL 0.5 → is_stable? YES → stable OUT
SOL 0.5 → is_stable? YES → stable OUT
Total: 3 stable OUT

// IN transfers
TokenX 1M → is_stable? NO → volatile IN
Total: 1 volatile IN
```

---

#### Step 3: Calculate Stable Side Total

**File:** `zerion_client/src/lib.rs:1249-1264`

```rust
let stable_out_total: f64 = trade_pair.out_transfers.iter()
    .filter_map(|transfer| {
        if Self::is_stable_currency(&transfer.asset.address) {
            transfer.value
        } else {
            None
        }
    })
    .sum();

// Sums ALL stable transfers:
// $600 + $100 + $100 = $800 ❌
```

**Problem:** No distinction between:
- Transfers that went to the swap pool ($600)
- Transfers that went to fee accounts ($200)

---

#### Step 4: Calculate Implicit Price

```rust
let volatile_transfer = volatile_transfers[0]; // TokenX
let quantity = 1_000_000.0;

let implicit_price = stable_out_total / quantity;
// = $800 / 1M = $0.0008 ❌

// But volatile_transfer.price exists and = $0.0006 ✓
// Parser ignores it!
```

---

#### Step 5: Create Event

```rust
NewFinancialEvent {
    event_type: NewEventType::Buy,
    token_symbol: "TokenX",
    quantity: Decimal::from(1_000_000),
    usd_price_per_token: Decimal::from_f64(0.0008),  // ❌ Wrong!
    usd_value: Decimal::from_f64(800.0),  // ❌ Inflated by $200!
    // ...
}
```

**Should Be:**
```rust
usd_price_per_token: Decimal::from_f64(0.0006),  // From transfer.price
usd_value: Decimal::from_f64(600.0),  // From transfer.value
```

---

#### Step 6: Impact on total_invested

```rust
total_invested += $800  // ❌ Inflated by 33%!
// Should be: $600
```

---

### Code Location for Fix

**File:** `zerion_client/src/lib.rs:1542-1548`

**Current Code:**
```rust
usd_price_per_token: Decimal::from_f64_retain(implicit_price).unwrap_or(Decimal::ZERO),
usd_value: Decimal::from_f64_retain(stable_side_value_usd).unwrap_or(Decimal::ZERO),
```

**Issue:** Always uses `stable_side_value_usd`, even when `volatile_transfer.value` exists and differs significantly

**Needed Logic:**
```rust
// Check if volatile transfer has its own value
let volatile_value = volatile_transfer.value;

// Compare stable vs volatile
let discrepancy = (stable_side_value_usd - volatile_value).abs();
let discrepancy_pct = discrepancy / stable_side_value_usd.max(volatile_value);

if discrepancy_pct > 0.05 {  // 5% threshold
    warn!(
        "⚠️  Significant value mismatch: stable=${}, volatile=${}. \
         Difference: ${} ({}%). Using volatile value as it excludes fees.",
        stable_side_value_usd,
        volatile_value,
        discrepancy,
        discrepancy_pct * 100.0
    );

    // Use volatile value (actual token value)
    usd_value = Decimal::from_f64_retain(volatile_value)?;
    usd_price_per_token = usd_value / quantity;
} else {
    // Use stable side (normal case)
    usd_value = Decimal::from_f64_retain(stable_side_value_usd)?;
}
```

---

## Edge Case #3: Fee Transfers - Detailed Example

### Scenario
Small SOL fee transfers included in stable_side_total

### Zerion Data
```json
{
  "changes": [
    {
      "asset": { "fungible_info": { "symbol": "SOL" } },
      "quantity": { "numeric": "0.5" },
      "direction": "out",
      "value": 150.0,
      "metadata": { "act_id": "act_100" }
    },
    {
      "asset": { "fungible_info": { "symbol": "SOL" } },
      "quantity": { "numeric": "0.000005" },
      "direction": "out",
      "value": 0.0015,
      "metadata": { "act_id": "act_100" }
    },
    {
      "asset": { "fungible_info": { "symbol": "TokenY" } },
      "quantity": { "numeric": "1000000" },
      "direction": "in",
      "value": 150.0,
      "metadata": { "act_id": "act_100" }
    }
  ]
}
```

### Current Parser Behavior

```rust
stable_out_total = $150.0 + $0.0015 = $150.0015

usd_value for TokenY = $150.0015
```

**Impact:** Adds $0.0015 per transaction. Over 1000 transactions = $1.50 extra.

---

### Fee Detection Logic Needed

```rust
fn is_likely_fee_transfer(transfer: &ZerionTransfer) -> bool {
    // Check 1: Very small USD value
    if let Some(value) = transfer.value {
        if value < 1.0 {  // Less than $1

            // Check 2: Very small quantity (common fee amounts)
            let qty = parse_quantity(&transfer.quantity.numeric);

            // Common Solana fee amounts
            if qty == 0.000005 || qty == 0.00001 || qty == 0.0001 {
                return true;
            }

            // Or very small relative to main transfers
            if value < 0.01 {  // Less than 1 cent
                return true;
            }
        }
    }
    false
}

// In stable aggregation:
let stable_out_total: f64 = trade_pair.out_transfers.iter()
    .filter(|t| Self::is_stable_currency(&t.asset.address))
    .filter(|t| !is_likely_fee_transfer(t))  // ← Filter fees
    .filter_map(|t| t.value)
    .sum();
```

---

## Summary: Code Locations Causing Issues

### 1. Meme-to-Meme Swaps (Double-Counting)
**Location:** `zerion_client/src/lib.rs:1016-1050` (trade pairing)

**Issue:** Groups by act_id without detecting that multiple act_ids are part of one swap

**Detection Pattern Needed:**
```rust
// If within same transaction:
// - SELL volatile_token_A (with stable IN)
// - BUY volatile_token_B (with stable OUT)
// - stable values match (~same amount)
// - timestamps very close (<1 second apart)
// → This is reinvestment, not new investment
```

---

### 2. Stable/Volatile Mismatches
**Location:** `zerion_client/src/lib.rs:1542-1548` (event creation)

**Issue:** Always uses `stable_side_value_usd`, ignores `volatile_transfer.value`

**Fix:** Compare both, use volatile value if discrepancy > 5%

---

### 3. Fee Transfers
**Location:** `zerion_client/src/lib.rs:1249-1264` (stable aggregation)

**Issue:** Sums ALL stable transfers, including tiny fees

**Fix:** Filter transfers with `value < $0.01` or quantity in known fee ranges

---

### 4. Precision Loss
**Location:** Multiple places with f64 conversions

**Issue:**
```rust
let calculated_value = price * amount.to_f64().unwrap_or(0.0);  // f64 math
usd_value: Decimal::from_f64_retain(calculated_value)  // Lost precision
```

**Fix:** Keep everything in Decimal domain

---

## Next Steps for User

To validate these findings and implement fixes, please provide:

1. **Transaction hash** of a meme-to-meme swap showing double-counting
2. **Transaction hash** of a swap with stable/volatile mismatch
3. **Token address** with incorrect total_invested and expected vs actual values

With concrete examples, I can:
- Verify which edge case(s) apply
- Implement targeted detection and handling
- Test fixes preserve core algorithm (FIFO, buy priority, etc.)
