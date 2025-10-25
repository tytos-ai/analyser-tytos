# Complete System Flow: Transaction Parsing → Enrichment → PNL Analysis

## Table of Contents
1. [Data Structures](#data-structures)
2. [Phase 1: Transaction Parsing](#phase-1-transaction-parsing)
3. [Phase 2: Trade Pairing](#phase-2-trade-pairing)
4. [Phase 3: Event Conversion](#phase-3-event-conversion)
5. [Phase 4: Enrichment](#phase-4-enrichment)
6. [Phase 5: Deduplication](#phase-5-deduplication)
7. [Phase 6: PNL Matching](#phase-6-pnl-matching)
8. [Edge Cases & Conditions](#edge-cases--conditions)

---

## Data Structures

### Input: ZerionTransaction
```rust
pub struct ZerionTransaction {
    pub id: String,                    // Zerion's internal ID
    pub transaction_type: String,      // Always "transactions"
    pub attributes: ZerionTransactionAttributes,
    pub relationships: Option<ZerionRelationships>,
}

pub struct ZerionTransactionAttributes {
    pub operation_type: String,        // "trade", "send", "receive", etc.
    pub hash: String,                  // Blockchain transaction hash
    pub mined_at: DateTime<Utc>,      // Transaction timestamp
    pub transfers: Vec<ZerionTransfer>, // All token movements in this tx
    pub fee: Option<ZerionFee>,        // Transaction fee
}

pub struct ZerionTransfer {
    pub direction: String,             // "in", "out", "self"
    pub quantity: ZerionQuantity,      // Token amount
    pub value: Option<f64>,           // USD value (may be NULL)
    pub price: Option<f64>,           // USD price per token (may be NULL)
    pub fungible_info: Option<ZerionFungibleInfo>, // Token metadata
    pub act_id: String,               // Action ID for grouping related transfers
}
```

### Output: NewFinancialEvent
```rust
pub struct NewFinancialEvent {
    pub wallet_address: String,
    pub token_address: String,
    pub token_symbol: String,
    pub chain_id: String,
    pub event_type: NewEventType,     // Buy, Sell, Receive
    pub quantity: Decimal,
    pub usd_price_per_token: Decimal,
    pub usd_value: Decimal,
    pub timestamp: DateTime<Utc>,
    pub transaction_hash: String,
}
```

---

## Phase 1: Transaction Parsing

**Entry Point:** `zerion_client::convert_to_financial_events()`
**File:** `zerion_client/src/lib.rs:1563`

### Step 1.1: Filter Transactions by Operation Type

```rust
for tx in transactions.iter() {
    match tx.attributes.operation_type.as_str() {
        "trade" => { /* Process trades */ }
        "send" => { /* Process sends */ }
        "receive" => { /* Process receives */ }
        _ => { /* Skip other types (approve, etc.) */ }
    }
}
```

**Conditions:**
- ✅ **"trade"**: Token swaps, DEX trades → Process with trade pairing
- ✅ **"send"**: Outgoing transfers → Process as SELL
- ✅ **"receive"**: Incoming transfers → Process as RECEIVE
- ❌ **"approve"**: Token approvals → Skip
- ❌ **Other types**: Skip

---

## Phase 2: Trade Pairing

**Function:** `pair_trade_transfers()`
**File:** `zerion_client/src/lib.rs:1016`

### Purpose
Group transfers within a transaction by `act_id` to understand which tokens were exchanged.

### Logic
```rust
fn pair_trade_transfers<'a>(transfers: &'a [ZerionTransfer]) -> Vec<TradePair<'a>> {
    let mut pairs_map: HashMap<String, TradePair<'a>> = HashMap::new();

    for transfer in transfers {
        let act_id = transfer.act_id.clone();
        let pair = pairs_map.entry(act_id).or_insert(TradePair {
            in_transfers: Vec::new(),
            out_transfers: Vec::new(),
            act_id,
        });

        match transfer.direction.as_str() {
            "in" | "self" => pair.in_transfers.push(transfer),
            "out" => pair.out_transfers.push(transfer),
            _ => {}  // Ignore unknown directions
        }
    }

    pairs_map.into_values().collect()
}
```

### Example
**Transaction with 4 transfers:**
```
Transfer 1: SOL OUT (act_id: 0)
Transfer 2: SOL OUT (act_id: 0) - fee
Transfer 3: SOL OUT (act_id: 0) - fee
Transfer 4: Solano IN (act_id: 0)
```

**Result: 1 trade pair**
```
TradePair {
    act_id: "0",
    in_transfers: [Solano],
    out_transfers: [SOL, SOL fee, SOL fee]
}
```

### Edge Cases
1. **Multiple act_ids**: Creates multiple pairs
2. **One-sided trades**: Pair has only IN or only OUT
   - Handled later by `incomplete trade` check
3. **Empty transfers**: No pairs created

---

## Phase 3: Event Conversion

**Function:** `convert_trade_pair_to_events()`
**File:** `zerion_client/src/lib.rs:1055`

### Decision Tree

#### 3.1: Multi-Hop Swap Detection
```rust
// Count unique token addresses
let unique_assets: HashSet<String> = /* extract all token addresses */;
let has_stable = unique_assets.iter().any(|addr| is_stable_currency(addr));

if unique_assets.len() >= 3 && has_stable {
    // Multi-hop swap: Token A → SOL → Token B
    // Use NET transfer analysis
}
```

**Condition:** 3+ unique tokens AND contains stable currency
**Example:** Selling Moon for Kick via SOL
**Action:** Calculate net transfers, filter intermediaries

##### 3.1.1: Net Transfer Calculation
```rust
let mut net_transfers: HashMap<String, (Decimal, Option<f64>, bool)> = HashMap::new();

// Add IN transfers (positive)
for transfer in in_transfers {
    entry.0 += amount;
}

// Subtract OUT transfers (negative)
for transfer in out_transfers {
    entry.0 -= amount;
}

// Filter by threshold
let net_threshold = Decimal::new(1, 3); // 0.001
if net_qty.abs() > net_threshold {
    // This is a real transfer, not intermediary
    create_event();
}
```

**Purpose:** Eliminate routing tokens (SOL fees in multi-hop)

#### 3.2: Direction Validation (Bug Fix)
```rust
// Check if all volatile transfers have the same direction
let volatile_directions: HashSet<&String> = volatile_transfers.iter()
    .map(|t| &t.direction)
    .collect();

if volatile_directions.len() > 1 {
    // MIXED DIRECTIONS - different blockchain transactions grouped!
    warn!("Mixed directions detected");
    // Fall back to standard conversion
    return events;
}
```

**Condition:** Multiple directions (e.g., one "in" and one "out")
**Cause:** Bug - transfers from different blockchain txs grouped by same act_id
**Action:** Skip implicit pricing, use standard conversion

#### 3.3: Incomplete Trade Check
```rust
if trade_pair.in_transfers.is_empty() || trade_pair.out_transfers.is_empty() {
    warn!("Incomplete trade detected");
    // Still process remaining transfers
}
```

**Condition:** Missing IN or OUT side
**Action:** Log warning, skip this pair

#### 3.4: Implicit Pricing Decision

##### Find Stable Currency Side
```rust
let stable_out_total: f64 = trade_pair.out_transfers.iter()
    .filter_map(|t| {
        if is_stable_currency(&t.token_address) {
            Some(t.value)
        } else {
            None
        }
    })
    .sum();

let stable_in_total: f64 = /* same for IN transfers */;

if stable_out_total > 0.0 {
    stable_side_total_value = Some(stable_out_total);
    stable_is_out = true;
    volatile_transfers = trade_pair.in_transfers; // Volatile is opposite side
}
```

**Stable currencies:** SOL, USDC, USDT, WSOL (see STABLE_CURRENCIES const)

##### Volatile Transfer Count Check
```rust
if volatile_transfers.len() == 1 {
    // SAFE: Use implicit pricing
    // Calculate: price = stable_value / volatile_quantity
} else if volatile_transfers.len() > 1 {
    // UNSAFE: Multiple volatile transfers would each get full stable value
    warn!("Multiple volatile transfers - skipping implicit pricing");
    // Use standard conversion with Zerion's prices
}
```

**Why this check?**
- If 3 volatile transfers and $800 stable value
- Implicit pricing would give each: $800 (multiplying to $2,400!)
- Standard conversion uses individual transfer prices

#### 3.5: Implicit Price Calculation
```rust
fn convert_transfer_with_implicit_price() {
    let amount = parse_decimal(&transfer.quantity.numeric)?;
    let implicit_price = stable_side_value_usd / amount.to_f64();

    let event_type = match transfer.direction.as_str() {
        "in" | "self" => NewEventType::Buy,
        "out" => NewEventType::Sell,
        _ => return None,
    };

    Some(NewFinancialEvent {
        quantity: amount,
        usd_price_per_token: Decimal::from_f64(implicit_price),
        usd_value: Decimal::from_f64(stable_side_value_usd),
        event_type,
        transaction_hash: tx.attributes.hash.clone(),
        // ... other fields
    })
}
```

**Key Points:**
- Volatile transfer direction determines BUY/SELL
- All stable side transfers are converted with their own prices
- Transaction hash is same for all events from one transaction

#### 3.6: Standard Conversion (Fallback)
```rust
fn convert_transfer_to_event(tx, transfer, wallet_address, chain_id) {
    // Check if transfer has price/value
    if transfer.price.is_none() && transfer.value.is_none() {
        // NULL price - will be marked for enrichment
        return None;
    }

    let price = transfer.price?;
    let value = transfer.value?;

    let event_type = match tx.operation_type {
        "trade" => match transfer.direction {
            "in" | "self" => NewEventType::Buy,
            "out" => NewEventType::Sell,
            _ => return None,
        },
        "send" => match transfer.direction {
            "out" => NewEventType::Sell,
            _ => return None,
        },
        "receive" => match transfer.direction {
            "in" => NewEventType::Receive,
            _ => return None,
        },
        _ => return None,
    };

    Some(NewFinancialEvent { /* ... */ })
}
```

### Summary: Event Creation Paths

```
Transaction
    ↓
Trade Pairing (by act_id)
    ↓
├─ Multi-hop (3+ tokens) → Net Transfer Analysis → Events
├─ Mixed Directions → Standard Conversion → Events
├─ Incomplete Trade → Skip pair
├─ Implicit Pricing (1 volatile) → Implicit + Stable Events
├─ Multiple Volatiles → Standard Conversion → Events
└─ No Stable Currency → Standard Conversion → Events
```

**Event Count Examples:**
- Simple swap (Solano for SOL): 2 events (1 Solano BUY + 1 SOL SELL)
- Swap with fees (Solano for SOL + 2 fee payments): 4 events (1 Solano BUY + 3 SOL SELL)

---

## Phase 4: Enrichment

**Function:** `extract_skipped_transaction_info()`
**File:** `zerion_client/src/lib.rs:1946`

### Purpose
Identify transfers that have NULL price/value in raw Zerion data for enrichment via BirdEye.

### Logic
```rust
pub fn extract_skipped_transaction_info(&self, transactions: &[ZerionTransaction]) -> Vec<SkippedTransactionInfo> {
    let mut skipped_info = Vec::new();

    for tx in transactions {
        // Only process trade, send, receive
        if !matches!(tx.operation_type.as_str(), "trade" | "send" | "receive") {
            continue;
        }

        for transfer in &tx.attributes.transfers {
            let fungible_info = transfer.fungible_info.as_ref()?;

            // CRITICAL CHECK: Both price AND value must be None
            if transfer.price.is_none() && transfer.value.is_none() {
                let event_type = /* determine from operation_type + direction */;

                skipped_info.push(SkippedTransactionInfo {
                    tx_hash: tx.attributes.hash.clone(),
                    token_mint: /* extract address */,
                    token_amount: /* parse quantity */,
                    event_type,
                    timestamp: tx.attributes.mined_at,
                    // ...
                });
            }
        }
    }

    skipped_info
}
```

### THE BUG (Now Fixed)
**Problem:** A transfer can have:
- `price = NULL` in raw Zerion data (because it's a new/unknown token)
- BUT still get a price via implicit pricing in Phase 3

**Result:**
1. Phase 3: Creates event with implicit price
2. Phase 4: Marks same transfer as "skipped" (sees price=NULL)
3. Enrichment: Creates ANOTHER event for same transfer
4. Result: DUPLICATE EVENTS!

**Fix:** Deduplication in Phase 5

---

## Phase 5: Deduplication

**Function:** Job orchestrator enrichment merge
**File:** `job_orchestrator/src/lib.rs:1365`

### Logic
```rust
// After getting enriched_events from BirdEye:

// Build set of existing event keys
let existing_event_keys: HashSet<(String, String, String)> = financial_events
    .iter()
    .map(|e| (
        e.transaction_hash.clone(),
        e.token_address.clone(),
        format!("{:?}", e.event_type)  // "Buy", "Sell", "Receive"
    ))
    .collect();

// Filter out duplicate enriched events
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

// Only add unique events
financial_events.extend(unique_enriched_events);
```

### Deduplication Key
- **transaction_hash**: Blockchain tx signature
- **token_address**: Token mint address
- **event_type**: Buy, Sell, or Receive

**Why this works:**
- Same transaction can't have 2 BUY events for same token
- If implicit pricing already created the event, enrichment duplicate is filtered

### Edge Cases
1. **All enriched events are duplicates**: Log message, extend nothing
2. **Some are duplicates**: Add only unique ones, log count
3. **No duplicates**: Add all enriched events

---

## Phase 6: PNL Matching

**Function:** `calculate_token_pnl()`
**File:** `pnl_core/src/new_pnl_engine.rs:570`

### Step 6.1: Separate Events by Type
```rust
let buy_events: Vec<_> = events.iter()
    .filter(|e| e.event_type == NewEventType::Buy)
    .cloned()
    .collect();

let sell_events: Vec<_> = events.iter()
    .filter(|e| e.event_type == NewEventType::Sell)
    .cloned()
    .collect();

let receive_events: Vec<_> = events.iter()
    .filter(|e| e.event_type == NewEventType::Receive)
    .cloned()
    .collect();
```

### Step 6.2: Sort by Timestamp (FIFO)
```rust
// Events are already sorted by timestamp in query
// But we ensure it here
buy_events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
sell_events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
```

### Step 6.3: FIFO Matching Algorithm

```rust
// Create pools
let mut buy_pool: VecDeque<BuyLot> = buy_events.iter()
    .map(|e| BuyLot {
        event: e.clone(),
        remaining_quantity: e.quantity,
        original_quantity: e.quantity,
    })
    .collect();

let mut receive_pool: VecDeque<ReceiveLot> = /* same for receives */;

// Process each sell in chronological order
for sell_event in sell_events {
    let mut quantity_to_match = sell_event.quantity;

    // Phase 1: Match against BOUGHT tokens first (priority)
    while quantity_to_match > Decimal::ZERO && !buy_pool.is_empty() {
        let buy_lot = buy_pool.front_mut().unwrap();

        if buy_lot.remaining_quantity <= quantity_to_match {
            // Consume entire buy lot
            let matched_qty = buy_lot.remaining_quantity;
            quantity_to_match -= matched_qty;

            let realized_pnl = calculate_pnl(
                matched_qty,
                buy_lot.event.usd_price_per_token,
                sell_event.usd_price_per_token
            );

            matched_trades.push(MatchedTrade {
                buy_event: buy_lot.event.clone(),
                sell_event: sell_event.clone(),
                matched_quantity: matched_qty,
                realized_pnl_usd: realized_pnl,
                hold_time_seconds: /* calculate */,
            });

            buy_pool.pop_front(); // Remove fully consumed lot
        } else {
            // Partially consume buy lot
            let matched_qty = quantity_to_match;
            buy_lot.remaining_quantity -= matched_qty;
            quantity_to_match = Decimal::ZERO;

            // Create matched trade
            // buy_lot stays in pool with reduced quantity
        }
    }

    // Phase 2: If still unmatched, try receive pool
    while quantity_to_match > Decimal::ZERO && !receive_pool.is_empty() {
        // Similar logic, but no P&L calculation
        // (received tokens have $0 cost basis)
    }

    // Phase 3: If still unmatched, it's an error
    if quantity_to_match > Decimal::ZERO {
        warn!("Unmatched sell quantity: {}", quantity_to_match);
    }
}
```

### Step 6.4: Calculate Remaining Position
```rust
// Sum up all remaining buy lots
let remaining_bought_quantity: Decimal = buy_pool.iter()
    .map(|lot| lot.remaining_quantity)
    .sum();

let remaining_received_quantity: Decimal = receive_pool.iter()
    .map(|lot| lot.remaining_quantity)
    .sum();

// Calculate average cost basis (bought tokens only)
let total_cost: Decimal = buy_pool.iter()
    .map(|lot| lot.remaining_quantity * lot.event.usd_price_per_token)
    .sum();

let avg_cost_basis = if remaining_bought_quantity > Decimal::ZERO {
    total_cost / remaining_bought_quantity
} else {
    Decimal::ZERO
};
```

### Step 6.5: Calculate Unrealized P&L
```rust
let current_price = /* fetch from Jupiter/BirdEye */;

let unrealized_pnl = (current_price - avg_cost_basis) * remaining_bought_quantity;

// Received tokens don't contribute to unrealized P&L
// (they have $0 cost basis, any sale is pure profit - captured in realized)
```

---

## Edge Cases & Conditions

### 1. Transaction Type Edge Cases

| Operation Type | Direction | Result | Notes |
|----------------|-----------|--------|-------|
| trade | in | Buy | Token received in swap |
| trade | out | Sell | Token sent in swap |
| trade | self | Buy | Self-transfer treated as buy |
| send | out | Sell | Outgoing transfer |
| send | in | Skip | Receiving a send → use "receive" type |
| receive | in | Receive | Incoming transfer |
| receive | out | Skip | Invalid state |
| approve | any | Skip | Not a transfer |

### 2. Price/Value Edge Cases

| Price | Value | Action |
|-------|-------|--------|
| NULL | NULL | Mark for enrichment |
| NULL | Present | Use value, calculate price |
| Present | NULL | Use price, calculate value |
| Present | Present | Use both (prefer price) |

### 3. Multi-Hop Swap Edge Cases

**Scenario:** Selling TokenA for TokenB via SOL

**Transfers:**
```
OUT: TokenA (1000 qty, $500 value)
OUT: SOL (0.5 qty, $50 value) - fee
OUT: SOL (0.5 qty, $50 value) - fee
IN: TokenB (2000 qty, $600 value)
```

**Net Analysis:**
```
TokenA: 0 - 1000 = -1000 (net OUT → SELL)
SOL: 0 - 0.5 - 0.5 = -1.0 (net OUT, but < threshold → filter)
TokenB: 2000 - 0 = +2000 (net IN → BUY)
```

**Result:** 2 events (TokenA SELL, TokenB BUY), SOL filtered as intermediary

### 4. Incomplete Trade Edge Cases

**Missing IN side:**
```
TradePair {
    in_transfers: [],
    out_transfers: [SOL OUT]
}
```
**Action:** Log warning, skip pair, continue processing

**Missing OUT side:**
```
TradePair {
    in_transfers: [Token IN],
    out_transfers: []
}
```
**Action:** Log warning, skip pair, continue processing

### 5. Mixed Direction Edge Case (Bug Scenario)

**Scenario:** Two different blockchain transactions grouped by same act_id

**Transaction 1 (AvQb9vkf - BUY):**
```
IN: Solano (12M tokens)
OUT: SOL ($409)
```

**Transaction 2 (261axxdq - SELL):**
```
OUT: Solano (9M tokens)
IN: SOL ($1068)
```

**If grouped into same pair (BUG):**
```
TradePair {
    act_id: "0",
    in_transfers: [Solano 12M, SOL $1068],
    out_transfers: [Solano 9M, SOL $409]
}
```

**Volatile transfers:** [Solano IN direction="in", Solano OUT direction="out"]
**Mixed directions detected** → Fall back to standard conversion
**Result:** Each transfer processed independently with correct direction

### 6. Enrichment Edge Cases

**Already has implicit price:**
- Phase 3: Creates event with price $0.0003
- Phase 4: Marks as skipped (raw data has price=NULL)
- BirdEye: Returns price $0.0003
- Phase 5: Deduplication filters it out ✅

**New token with no price anywhere:**
- Phase 3: Skipped (no implicit pricing possible)
- Phase 4: Marked for enrichment
- BirdEye: Returns NULL (token too new)
- Phase 5: No event created for this transfer ❌

**Token with BirdEye price but no Zerion price:**
- Phase 3: Skipped
- Phase 4: Marked for enrichment
- BirdEye: Returns valid price
- Phase 5: Unique event, added ✅

### 7. FIFO Matching Edge Cases

**More sells than buys:**
```
Buys: 1000 tokens @ $1 = $1000
Sells: 1500 tokens @ $2 = $3000
```
**Result:**
- Matched: 1000 tokens (realized P&L = $1000 profit)
- Unmatched: 500 sell tokens → **ERROR logged**
- Remaining: 0 tokens

**More buys than sells:**
```
Buys: 2000 tokens @ $1 = $2000
Sells: 1500 tokens @ $2 = $3000
```
**Result:**
- Matched: 1500 tokens (realized P&L = $1500 profit)
- Remaining: 500 tokens @ $1 avg cost

**Partial lot consumption:**
```
Buy #1: 1000 tokens @ $1
Sell #1: 600 tokens @ $2
```
**Result:**
- Matched: 600 tokens from Buy #1
- Remaining in Buy #1: 400 tokens @ $1 (stays in pool)

### 8. Decimal Precision Edge Cases

**Excessive precision:**
```
Quantity: "12005534.9539203984752093857203984572"
```
**Action:** Truncate to 28 decimal places, log warning

**Scientific notation:**
```
Quantity: "1.23e-8"
```
**Action:** Parse via f64, convert to Decimal, log precision warning

---

## Complete Flow Diagram

```
Zerion API Response
    ↓
[PHASE 1: PARSING]
    ↓
Filter by operation_type
    ├─ trade → Trade Pairing
    ├─ send → Direct Conversion
    └─ receive → Direct Conversion
    ↓
[PHASE 2: TRADE PAIRING]
    ↓
Group transfers by act_id
    ↓
Create TradePairs (IN/OUT)
    ↓
[PHASE 3: EVENT CONVERSION]
    ↓
For each TradePair:
    ├─ Multi-hop? (3+ tokens)
    │   └─ Net Transfer Analysis
    ├─ Mixed Directions?
    │   └─ Standard Conversion (fallback)
    ├─ Incomplete?
    │   └─ Skip
    ├─ Stable + 1 Volatile?
    │   └─ Implicit Pricing
    ├─ Stable + Multiple Volatiles?
    │   └─ Standard Conversion
    └─ No Stable?
        └─ Standard Conversion
    ↓
financial_events (List of NewFinancialEvent)
    ↓
[PHASE 4: ENRICHMENT EXTRACTION]
    ↓
For each transaction:
    For each transfer:
        If price=NULL AND value=NULL:
            Add to skipped_txs
    ↓
[PHASE 4.5: BIRDEYE ENRICHMENT]
    ↓
For each skipped_tx:
    Fetch historical price from BirdEye
    Create NewFinancialEvent
    ↓
enriched_events (List of NewFinancialEvent)
    ↓
[PHASE 5: DEDUPLICATION]
    ↓
Build key set from financial_events:
    (tx_hash, token_address, event_type)
    ↓
Filter enriched_events:
    Keep only events NOT in key set
    ↓
Extend financial_events with unique enriched_events
    ↓
[PHASE 6: PNL MATCHING]
    ↓
Group events by token_address
    ↓
For each token:
    ├─ Separate into: buys, sells, receives
    ├─ Sort by timestamp (FIFO)
    ├─ Create buy_pool and receive_pool
    │   ↓
    └─ For each sell (chronological):
        ├─ Match against buy_pool (Phase 1)
        │   └─ Calculate realized P&L
        ├─ If unmatched, match against receive_pool (Phase 2)
        │   └─ $0 cost basis, no P&L
        └─ If still unmatched → ERROR
    ↓
Calculate Remaining Position:
    ├─ Sum remaining buy_pool quantities
    ├─ Calculate avg cost basis
    ├─ Fetch current price
    └─ Calculate unrealized P&L
    ↓
TokenPnLResult {
    matched_trades: Vec<MatchedTrade>,
    remaining_position: RemainingPosition,
    total_realized_pnl_usd,
    total_unrealized_pnl_usd,
    total_pnl_usd,
}
```

---

## Critical Validation Checkpoints

### ✅ Checkpoint 1: After Parsing
```
VALIDATE: events.len() == expected_transfer_count
VALIDATE: All events have valid token_address
VALIDATE: All events have valid timestamp
VALIDATE: All events have non-zero quantity
```

### ✅ Checkpoint 2: After Enrichment
```
VALIDATE: no duplicate (tx_hash, token_address, event_type)
VALIDATE: enriched events have valid prices
LOG: duplicates_filtered count
```

### ✅ Checkpoint 3: Before PNL Matching
```
VALIDATE: events grouped correctly by token
VALIDATE: buy/sell/receive counts match expectations
LOG: Event summary per token
```

### ✅ Checkpoint 4: After PNL Matching
```
VALIDATE: All sells are matched (or logged as errors)
VALIDATE: remaining_quantity = total_bought - total_sold
VALIDATE: realized + unrealized = total P&L
LOG: Extreme buy/sell imbalances (>10x ratio)
```

---

## Configuration & Constants

### Stable Currencies
```rust
const STABLE_CURRENCIES: &[&str] = &[
    "So11111111111111111111111111111111111111112", // SOL
    "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", // USDC
    "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB", // USDT
    "So11111111111111111111111111111111111111111", // WSOL
];
```

### Net Transfer Threshold
```rust
let net_threshold_quantity = Decimal::new(1, 3); // 0.001
```
**Purpose:** Filter out dust/rounding errors in multi-hop swaps

### Decimal Precision
- **Max precision:** 28 decimal places
- **Truncation:** Automatic with warning log
- **Parsing priority:** Exact → Regular → Truncated → f64 fallback

---

This is the complete breakdown of the entire system. Every condition, every edge case, every decision point is documented.
