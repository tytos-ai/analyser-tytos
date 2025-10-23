# OOM Fix Progress - ROI Percentage Column Optimization

**Date Started**: 2025-10-14
**Status**: Phase 1 Complete, Phase 2 Pending
**Server**: http://134.199.211.155:8080 (4GB RAM Digital Ocean droplet)

---

## Problem Statement

### Original Issue
- **Endpoint**: `/api/results`
- **Symptom**: Server killed by OOM when fetching 56k wallet results
- **Root Cause**: Query loads entire `portfolio_json` JSONB blob for every record
  - Line 266-267 in `postgres_client.rs`: `SELECT portfolio_json`
  - Line 308: Deserializes FULL `PortfolioPnLResult` from JSON
  - **Memory Usage**: 20k wallets × 30KB each = 600MB → **OOM crash on 4GB server**

### User Requirements
1. Load **only lightweight summary data** for table display
2. Summary data must include **ALL fields needed for frontend filters and sorting**:
   - P&L (total, realized, unrealized)
   - Win rate, hold time, trades count
   - Unique tokens count, active days count
   - **ROI percentage (profit %)** - CRITICAL for "Profit %" sort option
3. Full `portfolio_json` should ONLY load when clicking individual wallet detail modal
4. If schema changes needed, provide migration plan to update existing 56k records

---

## Solution Architecture

### Memory Optimization
| Metric | Before | After | Improvement |
|--------|--------|-------|-------------|
| Query size | 20k records | 5k records | 4x smaller batches |
| Per-record memory | ~30KB (JSON) | ~200 bytes | **150x reduction** |
| Total memory | 600MB | 1MB | **600x reduction** |
| Query time | ~10s | ~50ms | **200x faster** |

### Key Discovery
The ROI/profit percentage is **already calculated** by the PnL engine:
```rust
// pnl_core/src/new_pnl_engine.rs:52
pub struct PortfolioPnLResult {
    pub profit_percentage: Decimal,  // Already calculated!
}
```
Formula: `((total_returned + current_holdings_value) / total_invested) × 100`

**We just need to extract it and store it in a column!**

---

## Phase 1: Schema Migration & Store Function ✅ COMPLETED

### 1.1 Created Lightweight Summary Struct ✅
**File**: `persistence_layer/src/lib.rs:63-81`

```rust
/// Lightweight summary of portfolio P&L result (without full portfolio_json)
/// Used for efficient listing and filtering without loading full result data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredPortfolioPnLResultSummary {
    pub wallet_address: String,
    pub chain: String,
    pub total_pnl_usd: f64,
    pub realized_pnl_usd: f64,
    pub unrealized_pnl_usd: f64,
    pub roi_percentage: f64,  // NEW FIELD!
    pub total_trades: i32,
    pub win_rate: f64,
    pub avg_hold_time_minutes: f64,
    pub unique_tokens_count: Option<u32>,
    pub active_days_count: Option<u32>,
    pub analyzed_at: DateTime<Utc>,
    pub is_favorited: bool,
    pub is_archived: bool,
}
```

**Memory per record**: ~200 bytes (vs 30KB with full JSON)

### 1.2 Updated Store Function ✅
**File**: `persistence_layer/src/postgres_client.rs:117-119`

```rust
// Extract profit_percentage (ROI) from the portfolio result
// This is already calculated by the PnL engine
let roi_percentage = portfolio_result.profit_percentage.to_string().parse::<f64>().unwrap_or(0.0);
```

**File**: `persistence_layer/src/postgres_client.rs:137-157`

```rust
INSERT INTO pnl_results
(wallet_address, chain, total_pnl_usd, realized_pnl_usd, unrealized_pnl_usd, total_trades, win_rate,
 tokens_analyzed, avg_hold_time_minutes, unique_tokens_count, active_days_count, roi_percentage, portfolio_json, analyzed_at, analysis_source)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
```

**Status**: New wallet analyses will automatically have `roi_percentage` populated.

### 1.3 Created SQL Migration Script ✅
**File**: `migrations/add_roi_percentage_column.sql`

#### Step 1: Add Column
```sql
ALTER TABLE pnl_results ADD COLUMN IF NOT EXISTS roi_percentage NUMERIC DEFAULT 0;
```

#### Step 2: Create Index
```sql
CREATE INDEX IF NOT EXISTS idx_pnl_results_roi ON pnl_results(roi_percentage);
```

#### Step 3: Backfill Existing Records
```sql
DO $$
DECLARE
    record_count INTEGER := 0;
    batch_size INTEGER := 100;
    ...
BEGIN
    -- For each record, extract profit_percentage from portfolio_json
    profit_percent := (portfolio_data->>'profit_percentage')::NUMERIC;

    -- Update the record
    UPDATE pnl_results
    SET roi_percentage = profit_percent
    WHERE wallet_address = wallet_rec.wallet_address
      AND chain = wallet_rec.chain;
END $$;
```

**Features**:
- Processes 100 records at a time (avoids memory issues)
- Progress logging every 100 records
- Statistics summary at end
- Safe to run on production (uses streaming fetch)
- **Estimated time**: ~10-15 minutes for 56k records

### 1.4 Created Migration Documentation ✅
**File**: `migrations/README_ROI_MIGRATION.md`

Complete documentation including:
- Overview of problem and solution
- What changed (schema, code, data source)
- How to run migration (step-by-step)
- Verification steps
- Rollback procedure
- Next steps

---

## Phase 2: Query Optimization ⏳ PENDING

### 2.1 Create Lightweight Query Function ❌ NOT STARTED
**File**: `persistence_layer/src/postgres_client.rs` (new function)

**Function to create**:
```rust
pub async fn get_all_pnl_results_summary(
    &self,
    offset: usize,
    limit: usize,
    chain_filter: Option<&str>,
) -> Result<(Vec<StoredPortfolioPnLResultSummary>, usize)> {
    // Get total count
    let count_query = if let Some(chain) = chain_filter {
        sqlx::query("SELECT COUNT(*) as count FROM pnl_results WHERE chain = $1").bind(chain)
    } else {
        sqlx::query("SELECT COUNT(*) as count FROM pnl_results")
    };

    let count_row = count_query.fetch_one(&self.pool).await?;
    let total_count: i64 = count_row.get("count");

    if total_count == 0 {
        return Ok((Vec::new(), 0));
    }

    // Get ONLY summary columns - NO portfolio_json!
    let rows = if let Some(chain) = chain_filter {
        sqlx::query(
            r#"
            SELECT wallet_address, chain, total_pnl_usd, realized_pnl_usd, unrealized_pnl_usd,
                   roi_percentage, total_trades, win_rate, avg_hold_time_minutes,
                   unique_tokens_count, active_days_count, analyzed_at, is_favorited, is_archived
            FROM pnl_results
            WHERE chain = $1
            ORDER BY analyzed_at DESC
            LIMIT $2 OFFSET $3
            "#
        )
        .bind(chain)
        .bind(limit as i64)
        .bind(offset as i64)
    } else {
        sqlx::query(
            r#"
            SELECT wallet_address, chain, total_pnl_usd, realized_pnl_usd, unrealized_pnl_usd,
                   roi_percentage, total_trades, win_rate, avg_hold_time_minutes,
                   unique_tokens_count, active_days_count, analyzed_at, is_favorited, is_archived
            FROM pnl_results
            ORDER BY analyzed_at DESC
            LIMIT $1 OFFSET $2
            "#
        )
        .bind(limit as i64)
        .bind(offset as i64)
    };

    let rows = rows.fetch_all(&self.pool).await?;

    // Parse rows directly from database columns - NO JSON deserialization!
    let mut results = Vec::new();
    for row in rows {
        let result = StoredPortfolioPnLResultSummary {
            wallet_address: row.get("wallet_address"),
            chain: row.get("chain"),
            total_pnl_usd: row.get("total_pnl_usd"),
            realized_pnl_usd: row.get("realized_pnl_usd"),
            unrealized_pnl_usd: row.get("unrealized_pnl_usd"),
            roi_percentage: row.get("roi_percentage"),  // NOW AVAILABLE FROM COLUMN!
            total_trades: row.get("total_trades"),
            win_rate: row.get("win_rate"),
            avg_hold_time_minutes: row.get("avg_hold_time_minutes"),
            unique_tokens_count: row.get::<Option<i32>, _>("unique_tokens_count").map(|v| v as u32),
            active_days_count: row.get::<Option<i32>, _>("active_days_count").map(|v| v as u32),
            analyzed_at: row.get("analyzed_at"),
            is_favorited: row.get("is_favorited"),
            is_archived: row.get("is_archived"),
        };
        results.push(result);
    }

    Ok((results, total_count as usize))
}
```

**Key Points**:
- ✅ **NO** `portfolio_json` in SELECT
- ✅ **NO** JSON deserialization
- ✅ Reads directly from indexed columns
- ✅ Returns lightweight `StoredPortfolioPnLResultSummary`
- ✅ Includes `roi_percentage` for frontend sorting
- ✅ Safe limit: 5000 records (vs 20k before)

**Memory**: 5000 records × 200 bytes = 1MB (vs 600MB before)

### 2.2 Update Handlers ❌ NOT STARTED
**File**: `api_server/src/handlers.rs`

#### Handler 1: `get_all_results` (line 890-987)
**Changes needed**:
```rust
pub async fn get_all_results(
    State(state): State<AppState>,
    Query(query): Query<AllResultsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(50).min(5000); // Changed from 20000 to 5000

    // OLD: Uses get_all_pnl_results (loads portfolio_json)
    // NEW: Use get_all_pnl_results_summary (NO portfolio_json)
    let (summary_results, total_count) = state.persistence_client
        .get_all_pnl_results_summary(offset, limit, query.chain.as_deref())  // NEW FUNCTION!
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to fetch results: {}", e)))?;

    // Convert to response format - NO deserialization needed!
    let results: Vec<StoredPnLResultSummary> = summary_results
        .into_iter()
        .map(|summary| StoredPnLResultSummary {
            wallet_address: summary.wallet_address,
            chain: summary.chain,
            token_address: "portfolio".to_string(),
            token_symbol: "PORTFOLIO".to_string(),
            total_pnl_usd: Decimal::from_f64_retain(summary.total_pnl_usd).unwrap_or(Decimal::ZERO),
            realized_pnl_usd: Decimal::from_f64_retain(summary.realized_pnl_usd).unwrap_or(Decimal::ZERO),
            unrealized_pnl_usd: Decimal::from_f64_retain(summary.unrealized_pnl_usd).unwrap_or(Decimal::ZERO),
            roi_percentage: Decimal::from_f64_retain(summary.roi_percentage).unwrap_or(Decimal::ZERO), // NOW AVAILABLE! (was hardcoded to ZERO at line 932)
            total_trades: summary.total_trades as u32,
            win_rate: Decimal::from_f64_retain(summary.win_rate).unwrap_or(Decimal::ZERO),
            avg_hold_time_minutes: Decimal::from_f64_retain(summary.avg_hold_time_minutes).unwrap_or(Decimal::ZERO),
            unique_tokens_count: summary.unique_tokens_count,
            active_days_count: summary.active_days_count,
            analyzed_at: summary.analyzed_at,
            is_favorited: summary.is_favorited,
            is_archived: summary.is_archived,
        })
        .collect();

    // ... rest of handler unchanged
}
```

**Critical fix**: Remove hardcoded `roi_percentage: Decimal::ZERO` at line 932!

#### Handler 2: `get_discovered_wallets` (find similar pattern)
Apply same changes as `get_all_results`.

### 2.3 Update PersistenceClient Wrapper ❌ NOT STARTED
**File**: `persistence_layer/src/lib.rs` (around line 426-436)

Add public method to delegate to PostgresClient:
```rust
pub async fn get_all_pnl_results_summary(
    &self,
    offset: usize,
    limit: usize,
    chain_filter: Option<&str>,
) -> Result<(Vec<StoredPortfolioPnLResultSummary>, usize)> {
    self.postgres_client
        .get_all_pnl_results_summary(offset, limit, chain_filter)
        .await
        .map_err(|e| PersistenceError::PoolCreation(e.to_string()))
}
```

### 2.4 Update Frontend Page Size ❌ NOT STARTED
**File**: `frontend/app/(dashboard)/results/page.tsx`

**Current values** (lines 130, 135, 141, 148):
- `pageSize: 20000` (DANGEROUS!)

**Change to**:
- `pageSize: 5000` (safe for 4GB server)

**Location**: Multiple `useQuery` hooks in the component

---

## Migration Execution Plan

### Prerequisites
- ✅ Migration SQL script created
- ✅ Migration README created
- ✅ Backend code updated to store ROI
- ❌ **MUST RUN**: SQL migration on server (adds column and backfills data)

### Step 1: SSH to Server
```bash
ssh root@134.199.211.155
```

### Step 2: Backup Database (Optional but Recommended)
```bash
pg_dump -U your_db_user wallet_analyzer_db > backup_before_roi_migration_$(date +%Y%m%d).sql
```

### Step 3: Run Migration
```bash
psql -U your_db_user -d wallet_analyzer_db -f /home/mrima/tytos/wallet-analyser/migrations/add_roi_percentage_column.sql
```

**Expected Output**:
```
NOTICE:  Starting backfill for 56000 records...
NOTICE:  Processed 100 / 56000 records...
NOTICE:  Processed 200 / 56000 records...
...
NOTICE:  Backfill completed! Updated 56000 records with ROI percentage.
NOTICE:  === ROI Distribution ===
NOTICE:  Records with ROI > 100%: 1234
NOTICE:  Records with ROI 0-100%: 45678
NOTICE:  Records with ROI < 0%: 9088
NOTICE:  Average ROI: 45.67
```

**Duration**: ~10-15 minutes for 56k records

### Step 4: Verify Migration
```sql
-- Check ROI column exists and has data
SELECT COUNT(*) as total,
       COUNT(CASE WHEN roi_percentage != 0 THEN 1 END) as with_roi
FROM pnl_results;

-- Check sample records
SELECT wallet_address, chain, roi_percentage, total_pnl_usd
FROM pnl_results
WHERE roi_percentage != 0
LIMIT 10;

-- Check index exists
SELECT indexname FROM pg_indexes WHERE tablename = 'pnl_results' AND indexname = 'idx_pnl_results_roi';
```

### Step 5: Build and Deploy New Code
```bash
cd /home/mrima/tytos/wallet-analyser/api_server
cargo build --release
systemctl restart wallet-analyzer-api

# Check logs
journalctl -u wallet-analyzer-api -f
```

### Step 6: Test New Records
```bash
# Analyze a test wallet
curl -X POST "http://134.199.211.155:8080/api/analyze" \
  -H "Content-Type: application/json" \
  -d '{"wallet_address":"test_wallet","chain":"solana"}'

# Verify ROI was stored
psql -U your_db_user -d wallet_analyzer_db -c "SELECT wallet_address, roi_percentage FROM pnl_results WHERE wallet_address='test_wallet';"
```

---

## Testing Plan (After Phase 2 Complete)

### Test 1: Memory Usage
**Before**: Query 20k records → 600MB → OOM crash
**After**: Query 5k records → 1MB → Success

```bash
# Monitor memory during query
watch -n 1 free -h

# Trigger query from frontend
# Should see NO spike in memory usage
```

### Test 2: Query Performance
**Before**: 10-25 seconds (or timeout/OOM)
**After**: < 100ms

```bash
# Time the query
time curl "http://134.199.211.155:8080/api/results?limit=5000"
```

### Test 3: Frontend Sorting
**Location**: `/results` page
**Test all sort options**:
- ✅ P&L (ascending/descending)
- ✅ Win Rate
- ✅ Trades
- ✅ Analyzed At
- ✅ Hold Time
- ✅ **Profit %** ← CRITICAL TEST (currently broken due to hardcoded ZERO)

**Expected**: All sorts work correctly without loading full portfolio_json

### Test 4: Frontend Filters
**Location**: `/results` page advanced filters
**Test all filters**:
- ✅ Chain filter
- ✅ Min P&L
- ✅ Max P&L
- ✅ Min Win Rate
- ✅ Max Win Rate
- ✅ Min Hold Time
- ✅ Max Hold Time
- ✅ Min Unique Tokens
- ✅ Min Active Days

**Expected**: All filters work with summary data only

### Test 5: Detail Modal
**Location**: Click on any wallet row
**Expected**:
- ✅ Modal opens
- ✅ Full portfolio details load (this WILL load portfolio_json - that's correct!)
- ✅ Token-level breakdown shows
- ✅ Trade history shows

---

## Files Modified

### Phase 1 (Completed) ✅
1. `persistence_layer/src/lib.rs` - Added `StoredPortfolioPnLResultSummary` struct (line 63-81)
2. `persistence_layer/src/postgres_client.rs` - Updated `store_pnl_result_with_source()` (lines 117-157)
3. `migrations/add_roi_percentage_column.sql` - Schema migration and backfill script
4. `migrations/README_ROI_MIGRATION.md` - Migration documentation
5. `PROGRESS_OOM_FIX.md` - This file

### Phase 2 (Pending) ❌
1. `persistence_layer/src/postgres_client.rs` - Add `get_all_pnl_results_summary()` function
2. `persistence_layer/src/lib.rs` - Add wrapper method in `PersistenceClient`
3. `api_server/src/handlers.rs` - Update `get_all_results()` and `get_discovered_wallets()`
4. `frontend/app/(dashboard)/results/page.tsx` - Update pageSize to 5000

---

## Success Criteria

### Phase 1 ✅
- [x] `roi_percentage` column can be added to database
- [x] Existing 56k records can be backfilled with ROI data
- [x] New wallet analyses automatically get `roi_percentage` populated
- [x] Migration documentation complete

### Phase 2 (When Complete)
- [ ] `/api/results` returns in < 100ms (was 25+ minutes or OOM)
- [ ] No OOM crashes with 56k records
- [ ] Memory usage < 10MB per query (was 600MB)
- [ ] All frontend sort options work (especially "Profit %")
- [ ] All frontend filters work without loading portfolio_json
- [ ] Detail modal still loads full portfolio data correctly

---

## Rollback Procedure

### If Migration Fails
```sql
-- Drop index
DROP INDEX IF EXISTS idx_pnl_results_roi;

-- Drop column
ALTER TABLE pnl_results DROP COLUMN IF EXISTS roi_percentage;

-- Verify
SELECT column_name FROM information_schema.columns
WHERE table_name='pnl_results' AND column_name='roi_percentage';
-- Should return no rows
```

### If Code Breaks
```bash
# Revert to previous commit
cd /home/mrima/tytos/wallet-analyser
git log --oneline | head -5  # Find commit before changes
git revert <commit_hash>

# Rebuild and restart
cd api_server
cargo build --release
systemctl restart wallet-analyzer-api
```

---

## Context for Next Session

### What Was Done
1. ✅ Discovered that `profit_percentage` is already calculated by PnL engine
2. ✅ Created lightweight `StoredPortfolioPnLResultSummary` struct
3. ✅ Updated `store_pnl_result()` to extract and store `roi_percentage`
4. ✅ Created SQL migration script with backfill logic
5. ✅ Created comprehensive documentation

### What's Next
1. ❌ Run SQL migration on server (MUST DO FIRST!)
2. ❌ Create `get_all_pnl_results_summary()` function in `postgres_client.rs`
3. ❌ Add wrapper method in `PersistenceClient`
4. ❌ Update `get_all_results()` handler to use new function
5. ❌ Update `get_discovered_wallets()` handler (if applicable)
6. ❌ Update frontend `pageSize` to 5000
7. ❌ Build, deploy, and test

### Critical Information
- **Server**: 4GB RAM Digital Ocean droplet at http://134.199.211.155:8080
- **Database**: PostgreSQL with ~56k wallet records
- **Problem Field**: `roi_percentage` (was hardcoded to ZERO at handlers.rs:932)
- **Data Source**: `profit_percentage` field in `PortfolioPnLResult` (pnl_core)
- **Memory Target**: Reduce from 600MB to 1MB per query (600x improvement)
- **Safe Limit**: 5000 records per query (down from 20000)

### Important Code Locations
- **Old query**: `postgres_client.rs:266-267` (SELECT portfolio_json - BAD!)
- **Old deserialization**: `postgres_client.rs:308` (Deserialize full JSON - BAD!)
- **Hardcoded ZERO**: `handlers.rs:932` (roi_percentage: Decimal::ZERO - MUST FIX!)
- **Frontend sort**: `results/page.tsx:294` (profit_percentage sort - currently broken)
- **Frontend pageSize**: `results/page.tsx:130,135,141,148` (20000 - MUST REDUCE!)

---

## Questions for Next Session

1. **Should we run the SQL migration now?** (Required before Phase 2)
2. **Are there other handlers that use `get_all_pnl_results`?** (Need to update all)
3. **Do we need to update any other endpoints?** (e.g., filtered queries)
4. **Should we keep the old `get_all_pnl_results` for backwards compatibility?** (For detail modal)
5. **What's the frontend pagination strategy?** (Infinite scroll vs traditional pagination)

---

## End of Progress Document
Last Updated: 2025-10-14
Next Action: Run SQL migration, then implement Phase 2
