# Batch Mode vs Continuous Mode: Unified Code Path Analysis

## Executive Summary

✅ **BOTH fixes apply to BOTH batch mode AND continuous mode** because they share the same underlying processing functions.

---

## Code Path Analysis

### Batch Mode Entry Points

**API Endpoint:** `POST /api/pnl/batch/run`
**Flow:**
```
API Server
    ↓
job_orchestrator::process_batch_job()
    ↓
job_orchestrator::process_single_wallet()
    ↓
job_orchestrator::process_single_wallet_with_zerion()
    ↓
job_orchestrator::calculate_pnl_with_zerion_transactions()
    ↓
[SHARED PROCESSING FUNCTIONS]
```

### Continuous Mode Entry Points

**Background Service:** Continuous wallet monitoring
**Flow:**
```
job_orchestrator::start_continuous_mode()
    ↓
job_orchestrator::run_continuous_cycle()
    ↓
job_orchestrator::process_claimed_batch()
    ↓
job_orchestrator::process_single_wallet_token_pair()
    ↓
job_orchestrator::process_single_wallet_with_zerion()
    ↓
job_orchestrator::calculate_pnl_with_zerion_transactions()
    ↓
[SHARED PROCESSING FUNCTIONS]
```

---

## Shared Functions (Unified Code Path)

Both modes converge at `calculate_pnl_with_zerion_transactions()` and then use:

### 1. Transaction Parsing
**Function:** `zerion_client::convert_to_financial_events()`
**File:** `zerion_client/src/lib.rs:1563`
**Location of Fix #1:** Lines 1302-1331 (Direction validation)

```rust
// Step 1: Convert Zerion transactions to financial events
let conversion_result = zerion_client.convert_to_financial_events(&zerion_transactions, wallet_address);
let mut financial_events = conversion_result.events;
```

**✅ Direction Validation Fix applies to BOTH modes** (in `convert_trade_pair_to_events`)

### 2. Enrichment Extraction
**Function:** `zerion_client::extract_skipped_transaction_info()`
**File:** `zerion_client/src/lib.rs:1946`

```rust
// Step 1.5: Extract and enrich skipped transactions using BirdEye
let skipped_txs = zerion_client.extract_skipped_transaction_info(&zerion_transactions, wallet_address);
```

**Shared by BOTH modes**

### 3. BirdEye Enrichment
**Function:** `job_orchestrator::enrich_with_birdeye_historical_prices()`
**File:** `job_orchestrator/src/lib.rs`

```rust
match self.enrich_with_birdeye_historical_prices(&skipped_txs, chain).await {
    Ok(enriched_events) => { /* ... */ }
}
```

**Shared by BOTH modes**

### 4. Deduplication
**Location:** `job_orchestrator::calculate_pnl_with_zerion_transactions()`
**File:** `job_orchestrator/src/lib.rs:1371-1408`
**Location of Fix #2:** Lines 1371-1408 (Deduplication)

```rust
// === CRITICAL BUG FIX: Filter duplicate events ===
let existing_event_keys: HashSet<(String, String, String)> = financial_events
    .iter()
    .map(|e| (e.transaction_hash.clone(), e.token_address.clone(), format!("{:?}", e.event_type)))
    .collect();

let unique_enriched_events: Vec<NewFinancialEvent> = enriched_events
    .into_iter()
    .filter(|e| !existing_event_keys.contains(&key))
    .collect();

financial_events.extend(unique_enriched_events);
```

**✅ Deduplication Fix applies to BOTH modes** (in shared function)

### 5. PNL Matching
**Function:** `pnl_core::NewPnLEngine::calculate_portfolio_pnl()`
**File:** `pnl_core/src/new_pnl_engine.rs`

```rust
let portfolio_result = pnl_engine
    .calculate_portfolio_pnl(events_by_token, Some(current_prices))
    .map_err(|e| OrchestratorError::PnL(format!("Portfolio P&L calculation failed: {}", e)))?;
```

**Shared by BOTH modes**

---

## Key Differences Between Modes

### Batch Mode Characteristics
- **Trigger:** User submits API request
- **Input:** User-provided wallet list (file or JSON)
- **Processing:** Parallel processing of all wallets
- **Output:** Immediate API response with results
- **Storage:** Results returned to user, optionally stored
- **Use Case:** On-demand analysis, research, manual queries

### Continuous Mode Characteristics
- **Trigger:** Background service (24/7)
- **Input:** Wallets discovered by `dex_client` via DexScreener
- **Processing:** Work-stealing pattern (multiple instances)
- **Output:** Results stored in Redis for retrieval
- **Storage:** Always stored in Redis with "continuous" source tag
- **Use Case:** Automated monitoring, copy trading, alerts

---

## Configuration Differences

### Batch Mode Config
```toml
[pnl]
pnl_parallel_batch_size = 10  # Process N wallets in parallel
```

### Continuous Mode Config
```toml
[system]
pnl_parallel_batch_size = 10  # Claim N wallet-token pairs per cycle

[continuous]
cycle_delay_seconds = 60      # Delay between cycles
```

### Shared Config
```toml
[zerion]
default_max_transactions = 1000  # Max txs per wallet (both modes)

[birdeye]
parallel_batch_size = 50         # Enrichment parallelism (both modes)
```

---

## Data Flow Comparison

### Batch Mode Flow
```
User API Request
    ↓
Parse wallet list
    ↓
[SHARED: process_single_wallet_with_zerion()]
    ↓
[SHARED: calculate_pnl_with_zerion_transactions()]
    ↓
[SHARED: convert_to_financial_events()]          ← FIX #1 HERE
    ↓
[SHARED: extract_skipped_transaction_info()]
    ↓
[SHARED: enrich_with_birdeye_historical_prices()]
    ↓
[SHARED: Deduplication]                          ← FIX #2 HERE
    ↓
[SHARED: PnL Matching]
    ↓
Return results to user
```

### Continuous Mode Flow
```
DexScreener discovers wallet
    ↓
Push to Redis queue
    ↓
Orchestrator claims batch
    ↓
[SHARED: process_single_wallet_with_zerion()]
    ↓
[SHARED: calculate_pnl_with_zerion_transactions()]
    ↓
[SHARED: convert_to_financial_events()]          ← FIX #1 HERE
    ↓
[SHARED: extract_skipped_transaction_info()]
    ↓
[SHARED: enrich_with_birdeye_historical_prices()]
    ↓
[SHARED: Deduplication]                          ← FIX #2 HERE
    ↓
[SHARED: PnL Matching]
    ↓
Store results in Redis
```

---

## Fix Application Summary

### Fix #1: Direction Validation
**Location:** `zerion_client/src/lib.rs:1302-1331`
**Function:** `convert_trade_pair_to_events()`
**Applies To:** ✅ Batch Mode, ✅ Continuous Mode
**Reason:** Both modes call the same `convert_to_financial_events()` function

**Code:**
```rust
// Check if all volatile transfers have the same direction
let volatile_directions: HashSet<&String> = volatile_transfers.iter()
    .map(|t| &t.direction)
    .collect();

if volatile_directions.len() > 1 {
    // Mixed directions - fall back to standard conversion
    warn!("⚠️  MIXED DIRECTIONS in trade pair");
    return events;
}
```

### Fix #2: Enrichment Deduplication
**Location:** `job_orchestrator/src/lib.rs:1371-1408`
**Function:** `calculate_pnl_with_zerion_transactions()`
**Applies To:** ✅ Batch Mode, ✅ Continuous Mode
**Reason:** Both modes call the same `calculate_pnl_with_zerion_transactions()` function

**Code:**
```rust
let existing_event_keys: HashSet<(String, String, String)> = financial_events
    .iter()
    .map(|e| (e.transaction_hash.clone(), e.token_address.clone(), format!("{:?}", e.event_type)))
    .collect();

let unique_enriched_events: Vec<NewFinancialEvent> = enriched_events
    .into_iter()
    .filter(|e| {
        let key = (e.transaction_hash.clone(), e.token_address.clone(), format!("{:?}", e.event_type));
        !existing_event_keys.contains(&key)
    })
    .collect();
```

---

## Verification Points

### For Batch Mode
**Test Command:**
```bash
curl -X GET "http://localhost:8080/api/v2/wallets/BAr5csYtpWoNpwhUjixX7ZPHXkUciFZzjBp9uNxZXJPh/analysis?chain=solana"
```

**Expected Logs:**
```
➕ Adding X unique enriched events (filtered Y duplicates from implicit pricing)
```

### For Continuous Mode
**Test Command:**
```bash
# Push a wallet to the continuous queue
redis-cli LPUSH "wallet_token_pairs_queue" '{"wallet_address":"BAr5csYtpWoNpwhUjixX7ZPHXkUciFZzjBp9uNxZXJPh","token_address":"6AiuSc3pYM5rpbKPq8JfSQoMoUsfDpzvP5PmMSEKpump","token_symbol":"Solano","chain":"solana"}'

# Check orchestrator logs
tail -f /path/to/orchestrator.log | grep "unique enriched events"
```

**Expected Logs:**
```
➕ Adding X unique enriched events (filtered Y duplicates from implicit pricing)
```

---

## Performance Characteristics

### Batch Mode
- **Parallelism:** Per-wallet (process multiple wallets simultaneously)
- **Memory:** Peak usage during parallel batch processing
- **Response Time:** Immediate (returns when all wallets processed)
- **Throughput:** Limited by API request rate and parallel batch size

### Continuous Mode
- **Parallelism:** Work-stealing (multiple instances claim batches)
- **Memory:** Steady state (processes batch, releases, repeats)
- **Response Time:** Asynchronous (stored in Redis for later retrieval)
- **Throughput:** Limited by Redis queue depth and instance count

---

## Error Handling Differences

### Batch Mode
- **Failure:** Returns error to user immediately
- **Retry:** User must manually retry
- **Partial Success:** Returns partial results + error list

### Continuous Mode
- **Failure:** Returns failed items to Redis queue
- **Retry:** Automatic retry on next cycle
- **Partial Success:** Successful items stored, failed items re-queued

---

## Storage Patterns

### Batch Mode
```rust
// Optional storage (for history/caching)
redis.store_pnl_result_with_source(
    wallet_address,
    chain,
    &report,
    "batch",
    incomplete_count
)
```

### Continuous Mode
```rust
// Always stored (for retrieval/alerts)
redis.store_pnl_result_with_source(
    wallet_address,
    chain,
    &report,
    "continuous",
    incomplete_count
)
```

**Storage Key Pattern:**
```
pnl:result:{wallet_address}:{chain}
```

**Metadata Includes:**
- `source`: "batch" or "continuous"
- `calculated_at`: UTC timestamp
- `incomplete_trades_count`: Number of incomplete trades
- `token_count`: Number of tokens analyzed

---

## Conclusion

### ✅ Fixes Are Universal

Both fixes apply to both operational modes because:

1. **Unified Processing Core:** Both modes use `calculate_pnl_with_zerion_transactions()`
2. **Shared Parsing:** Both modes use `convert_to_financial_events()` from `zerion_client`
3. **Same Enrichment Path:** Both modes use `enrich_with_birdeye_historical_prices()`
4. **Identical PNL Engine:** Both modes use `NewPnLEngine::calculate_portfolio_pnl()`

### 🎯 Testing Strategy

To verify fixes work in both modes:

1. **Test Batch Mode:** Use API endpoint with problematic wallet
2. **Test Continuous Mode:** Push same wallet to Redis queue
3. **Compare Results:** Should match (within timestamp differences)
4. **Verify Logs:** Look for deduplication messages in both modes

### 📊 Expected Impact (Both Modes)

| Metric | Before Fix | After Fix |
|--------|------------|-----------|
| Buy Events | 20 | ~10-12 |
| Total Invested | $13,950 | ~$7,128 |
| Remaining Position | 22M tokens | ~0 tokens |
| Double-Counting | Yes | No |

**Both operational modes will see identical improvements!**
