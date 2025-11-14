# Copy-Trading Metrics Implementation Guide
**Frontend-Only Filtering Approach**

## Overview
Add 6 new filterable metrics and 4 display-only metrics to the wallet analyzer for copy-trading analysis. All filtering happens client-side in the frontend, matching the existing pattern used for P&L, win rate, and other filters.

---

## New Metrics to Implement

### Filterable Metrics (Client-Side)

1. **Expectancy %**
   ```
   Expectancy % = (Winrate × AvgWin%) + ((1-Winrate) × AvgLoss%)
   ```
   - Components:
     - Winrate = wins / (wins + losses)
     - AvgWin% = mean(roi_pct_token | is_win)
     - AvgLoss% = mean(roi_pct_token | is_loss) [negative]

2. **Median Loss %**
   ```
   Median Loss % = Median(roi_pct_token | is_loss)
   ```

3. **Skill PnL Ratio**
   ```
   SkillPnLRatio = (TotalPnL - Top1PnL_pos) / TotalPnL
   ```
   - TotalPnL = Σ(pnl_usd_token)
   - Top1PnL_pos = max(pnl_usd_token | > 0)
   - If TotalPnL ≤ 0 → SkillPnLRatio = 0

4. **Profitable Tokens > X%**
   ```
   CountGoodWins = #(roi_pct_token > X)
   ```
   - Default threshold: 20%
   - User can configure threshold in UI

5. **Median ROI %**
   ```
   Median ROI % = Median(roi_pct_token)
   ```

6. **Winrate %**
   ```
   Winrate % = #(roi_pct_token > 0) / #(roi_pct_token ≠ 0) × 100
   ```
   - Note: This already exists as `win_rate` but will be surfaced in the new metrics section

### Display-Only Metrics (No Filtering)

1. **Avg Win % / Avg Loss %**
   - AvgWin% = Mean(roi_pct_token | is_win)
   - AvgLoss% = Mean(roi_pct_token | is_loss)

2. **Median of Profits (USD)**
   ```
   Median Profit (USD) = Median(pnl_usd_token | pnl_usd_token > 0)
   ```

3. **Per-Token ROI Table**
   - Columns: token | roi_pct | pnl_usd | invested_usd | returned_usd | result | trades
   - Displayed in wallet detail modal

4. **Top 1 P&L (USD)**
   - Largest single token P&L (used for Skill PnL Ratio calculation)

---

## Data Requirements

### Per Token (from TokenPnLResult)
From `/home/mrima/tytos/wallet-analyser/pnl_core/src/new_pnl_engine.rs` lines 94-151:
- `token_address` / `token_symbol`
- `total_invested_usd` (cost basis)
- `total_returned_usd` (proceeds)
- `total_pnl_usd` (realized + unrealized)
- `winning_trades` / `losing_trades`
- `remaining_position` (for unrealized value)

### Calculated Per Token
- **ROI %**: `roi_pct_token = ((total_returned_usd + unrealized_value) / total_invested_usd - 1) × 100`
- **Flags**: `is_win = roi_pct_token > 0`, `is_loss = roi_pct_token < 0`

### Exchange Currency Filtering
From lines 249-330 in `new_pnl_engine.rs`, the function `is_exchange_currency_token()` excludes:
- SOL (native)
- USDC, USDT (stablecoins)
- Wrapped tokens
- Tokens with symbol matching exchange patterns

---

## Implementation Plan

### Phase 1: Backend Core Calculations
**File**: `/home/mrima/tytos/wallet-analyser/pnl_core/src/new_pnl_engine.rs`
**Duration**: 1-2 days

#### 1.1 Add Fields to `PortfolioPnLResult` Struct

**Location**: After line 221 (inside `PortfolioPnLResult` struct definition at lines 154-222)

Add these fields before the closing brace:

```rust
// Copy-trading metrics
#[serde(default)]
pub expectancy_percentage: Decimal,

#[serde(default)]
pub median_loss_percentage: Decimal,

#[serde(default)]
pub skill_pnl_ratio: Decimal,

#[serde(default)]
pub profitable_tokens_count: u32,

#[serde(default)]
pub profitable_tokens_threshold_pct: Decimal,

#[serde(default)]
pub median_roi_percentage: Decimal,

#[serde(default)]
pub avg_win_percentage: Decimal,

#[serde(default)]
pub avg_loss_percentage: Decimal,

#[serde(default)]
pub median_profit_usd: Decimal,

#[serde(default)]
pub top1_pnl_usd: Decimal,
```

#### 1.2 Add Helper Struct and Function

**Location**: After the `PortfolioPnLResult` impl block (around line 245)

```rust
/// Copy-trading metrics calculation result
#[derive(Debug, Clone)]
struct CopyTradingMetrics {
    pub expectancy_percentage: Decimal,
    pub median_loss_percentage: Decimal,
    pub skill_pnl_ratio: Decimal,
    pub profitable_tokens_count: u32,
    pub profitable_tokens_threshold_pct: Decimal,
    pub median_roi_percentage: Decimal,
    pub avg_win_percentage: Decimal,
    pub avg_loss_percentage: Decimal,
    pub median_profit_usd: Decimal,
    pub top1_pnl_usd: Decimal,
}

/// Helper function to calculate median of sorted Decimal vector
fn calculate_median(sorted_values: &[Decimal]) -> Decimal {
    if sorted_values.is_empty() {
        return Decimal::ZERO;
    }

    let len = sorted_values.len();
    if len % 2 == 1 {
        sorted_values[len / 2]
    } else {
        (sorted_values[len / 2 - 1] + sorted_values[len / 2]) / Decimal::TWO
    }
}
```

#### 1.3 Add Calculation Method to PnLEngine Impl

**Location**: Inside the `impl PnLEngine` block (add after line 1667, after `calculate_active_days_count`)

```rust
/// Calculate copy-trading metrics from token results
/// Excludes exchange currency tokens (SOL, USDC, etc.)
fn calculate_copy_trading_metrics(
    token_results: &[TokenPnLResult],
    profitable_tokens_threshold: Decimal,
) -> CopyTradingMetrics {
    // Calculate per-token ROI percentages (excluding exchange currencies)
    let mut token_roi_data: Vec<(Decimal, Decimal)> = Vec::new(); // (roi_pct, pnl_usd)

    for token in token_results {
        if Self::is_exchange_currency_token(token) {
            continue;
        }

        // ROI% = ((total_returned + remaining_value) / total_invested - 1) × 100
        let roi_pct = if token.total_invested_usd > Decimal::ZERO {
            let unrealized_value = token.remaining_position.as_ref()
                .map(|p| p.current_value_usd)
                .unwrap_or(Decimal::ZERO);
            let total_value = token.total_returned_usd + unrealized_value;
            ((total_value / token.total_invested_usd) - Decimal::ONE) * Decimal::from(100)
        } else {
            Decimal::ZERO
        };

        token_roi_data.push((roi_pct, token.total_pnl_usd));
    }

    // Separate winners and losers
    let winning_rois: Vec<Decimal> = token_roi_data.iter()
        .filter(|(roi, _)| *roi > Decimal::ZERO)
        .map(|(roi, _)| *roi)
        .collect();

    let losing_rois: Vec<Decimal> = token_roi_data.iter()
        .filter(|(roi, _)| *roi < Decimal::ZERO)
        .map(|(roi, _)| *roi)
        .collect();

    // Calculate Avg Win % and Avg Loss %
    let avg_win_pct = if !winning_rois.is_empty() {
        winning_rois.iter().sum::<Decimal>() / Decimal::from(winning_rois.len())
    } else {
        Decimal::ZERO
    };

    let avg_loss_pct = if !losing_rois.is_empty() {
        losing_rois.iter().sum::<Decimal>() / Decimal::from(losing_rois.len())
    } else {
        Decimal::ZERO
    };

    // Calculate Token-level Winrate
    let total_tokens = token_roi_data.len();
    let winning_tokens = winning_rois.len();
    let token_win_rate = if total_tokens > 0 {
        Decimal::from(winning_tokens) / Decimal::from(total_tokens)
    } else {
        Decimal::ZERO
    };

    // Calculate Expectancy %
    let expectancy_pct = (token_win_rate * avg_win_pct) +
                         ((Decimal::ONE - token_win_rate) * avg_loss_pct);

    // Calculate Median Loss %
    let mut losing_roi_sorted = losing_rois.clone();
    losing_roi_sorted.sort();
    let median_loss_pct = calculate_median(&losing_roi_sorted);

    // Calculate Median ROI %
    let mut all_roi_values: Vec<Decimal> = token_roi_data.iter()
        .map(|(roi, _)| *roi)
        .collect();
    all_roi_values.sort();
    let median_roi_pct = calculate_median(&all_roi_values);

    // Calculate Median Profit USD (only positive P&Ls)
    let mut profitable_pnl_values: Vec<Decimal> = token_roi_data.iter()
        .filter(|(_, pnl)| *pnl > Decimal::ZERO)
        .map(|(_, pnl)| *pnl)
        .collect();
    profitable_pnl_values.sort();
    let median_profit_usd = calculate_median(&profitable_pnl_values);

    // Calculate Skill PnL Ratio and Top 1 P&L
    let mut all_pnl_values: Vec<Decimal> = token_roi_data.iter()
        .map(|(_, pnl)| *pnl)
        .collect();
    all_pnl_values.sort_by(|a, b| b.cmp(a)); // Descending

    let top1_pnl = all_pnl_values.get(0).copied().unwrap_or(Decimal::ZERO);
    let total_pnl: Decimal = all_pnl_values.iter().sum();

    let skill_pnl_ratio = if total_pnl > Decimal::ZERO {
        (total_pnl - top1_pnl.max(Decimal::ZERO)) / total_pnl
    } else {
        Decimal::ZERO
    };

    // Count Profitable Tokens > X%
    let profitable_tokens_count = token_roi_data.iter()
        .filter(|(roi, _)| *roi > profitable_tokens_threshold)
        .count() as u32;

    CopyTradingMetrics {
        expectancy_percentage: expectancy_pct,
        median_loss_percentage: median_loss_pct,
        skill_pnl_ratio,
        profitable_tokens_count,
        profitable_tokens_threshold_pct: profitable_tokens_threshold,
        median_roi_percentage: median_roi_pct,
        avg_win_percentage: avg_win_pct,
        avg_loss_percentage: avg_loss_pct,
        median_profit_usd,
        top1_pnl_usd: top1_pnl,
    }
}
```

#### 1.4 Call Metrics Calculation in `calculate_portfolio_pnl`

**Location**: Inside `calculate_portfolio_pnl()` at line 339-572, after the `active_days_count` calculation (around line 555)

Add this code:

```rust
// Calculate copy-trading metrics (default 20% threshold for "profitable tokens")
let copy_metrics = Self::calculate_copy_trading_metrics(
    &token_results,
    Decimal::from(20) // Default threshold
);
```

Then add these fields when constructing the `PortfolioPnLResult` (around line 560):

```rust
expectancy_percentage: copy_metrics.expectancy_percentage,
median_loss_percentage: copy_metrics.median_loss_percentage,
skill_pnl_ratio: copy_metrics.skill_pnl_ratio,
profitable_tokens_count: copy_metrics.profitable_tokens_count,
profitable_tokens_threshold_pct: copy_metrics.profitable_tokens_threshold_pct,
median_roi_percentage: copy_metrics.median_roi_percentage,
avg_win_percentage: copy_metrics.avg_win_percentage,
avg_loss_percentage: copy_metrics.avg_loss_percentage,
median_profit_usd: copy_metrics.median_profit_usd,
top1_pnl_usd: copy_metrics.top1_pnl_usd,
```

#### 1.5 Unit Tests

Create tests in `pnl_core/src/tests.rs` or a new test module:

```rust
#[cfg(test)]
mod copy_trading_metrics_tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_calculate_median_odd_count() {
        let values = vec![dec!(1), dec!(2), dec!(3), dec!(4), dec!(5)];
        assert_eq!(calculate_median(&values), dec!(3));
    }

    #[test]
    fn test_calculate_median_even_count() {
        let values = vec![dec!(1), dec!(2), dec!(3), dec!(4)];
        assert_eq!(calculate_median(&values), dec!(2.5));
    }

    #[test]
    fn test_calculate_median_empty() {
        let values: Vec<Decimal> = vec![];
        assert_eq!(calculate_median(&values), Decimal::ZERO);
    }

    // Add more tests for edge cases:
    // - All winners, no losers
    // - All losers, no winners
    // - Single token
    // - Skill ratio when top1 > totalPnL
}
```

---

### Phase 2: Database Layer
**Duration**: 1 day

#### 2.1 Create Migration

**File**: `/home/mrima/tytos/wallet-analyser/migrations/add_copy_trading_metrics.sql`

```sql
-- Add copy-trading metrics columns to pnl_results table
ALTER TABLE pnl_results
ADD COLUMN IF NOT EXISTS expectancy_percentage NUMERIC(10, 4) DEFAULT 0.0,
ADD COLUMN IF NOT EXISTS median_loss_percentage NUMERIC(10, 4) DEFAULT 0.0,
ADD COLUMN IF NOT EXISTS skill_pnl_ratio NUMERIC(10, 6) DEFAULT 0.0,
ADD COLUMN IF NOT EXISTS profitable_tokens_count INTEGER DEFAULT 0,
ADD COLUMN IF NOT EXISTS profitable_tokens_threshold_pct NUMERIC(10, 2) DEFAULT 20.0,
ADD COLUMN IF NOT EXISTS median_roi_percentage NUMERIC(10, 4) DEFAULT 0.0,
ADD COLUMN IF NOT EXISTS avg_win_percentage NUMERIC(10, 4) DEFAULT 0.0,
ADD COLUMN IF NOT EXISTS avg_loss_percentage NUMERIC(10, 4) DEFAULT 0.0,
ADD COLUMN IF NOT EXISTS median_profit_usd NUMERIC(20, 2) DEFAULT 0.0,
ADD COLUMN IF NOT EXISTS top1_pnl_usd NUMERIC(20, 2) DEFAULT 0.0;

-- Note: No indexes needed since filtering is client-side
-- Indexes would only slow down INSERT operations
```

#### 2.2 Update Persistence Layer Struct

**File**: `/home/mrima/tytos/wallet-analyser/persistence_layer/src/lib.rs`
**Location**: Lines 65-90, inside `StoredPortfolioPnLResultSummary` struct

Add after line 89 (before the closing brace):

```rust
// Copy-trading metrics
#[serde(default)]
pub expectancy_percentage: f64,
#[serde(default)]
pub median_loss_percentage: f64,
#[serde(default)]
pub skill_pnl_ratio: f64,
#[serde(default)]
pub profitable_tokens_count: u32,
#[serde(default)]
pub profitable_tokens_threshold_pct: f64,
#[serde(default)]
pub median_roi_percentage: f64,
#[serde(default)]
pub avg_win_percentage: f64,
#[serde(default)]
pub avg_loss_percentage: f64,
#[serde(default)]
pub median_profit_usd: f64,
#[serde(default)]
pub top1_pnl_usd: f64,
```

#### 2.3 Update PostgreSQL Client - INSERT

**File**: `/home/mrima/tytos/wallet-analyser/persistence_layer/src/postgres_client.rs`

**Step 1**: Extract metrics from PortfolioPnLResult (around line 105 in `store_pnl_result_with_source`)

```rust
// Copy-trading metrics
let expectancy_percentage = portfolio_result.expectancy_percentage
    .to_string().parse::<f64>().unwrap_or(0.0);
let median_loss_percentage = portfolio_result.median_loss_percentage
    .to_string().parse::<f64>().unwrap_or(0.0);
let skill_pnl_ratio = portfolio_result.skill_pnl_ratio
    .to_string().parse::<f64>().unwrap_or(0.0);
let profitable_tokens_count = portfolio_result.profitable_tokens_count as i32;
let profitable_tokens_threshold_pct = portfolio_result.profitable_tokens_threshold_pct
    .to_string().parse::<f64>().unwrap_or(20.0);
let median_roi_percentage = portfolio_result.median_roi_percentage
    .to_string().parse::<f64>().unwrap_or(0.0);
let avg_win_percentage = portfolio_result.avg_win_percentage
    .to_string().parse::<f64>().unwrap_or(0.0);
let avg_loss_percentage = portfolio_result.avg_loss_percentage
    .to_string().parse::<f64>().unwrap_or(0.0);
let median_profit_usd = portfolio_result.median_profit_usd
    .to_string().parse::<f64>().unwrap_or(0.0);
let top1_pnl_usd = portfolio_result.top1_pnl_usd
    .to_string().parse::<f64>().unwrap_or(0.0);
```

**Step 2**: Update INSERT statement (around line 143)

Find the existing INSERT and add the new columns:

```sql
INSERT INTO pnl_results (
    -- ... existing columns ...
    expectancy_percentage, median_loss_percentage, skill_pnl_ratio,
    profitable_tokens_count, profitable_tokens_threshold_pct, median_roi_percentage,
    avg_win_percentage, avg_loss_percentage, median_profit_usd, top1_pnl_usd
) VALUES (
    -- ... existing $1, $2, etc. ...
    $21, $22, $23, $24, $25, $26, $27, $28, $29, $30
)
```

**Step 3**: Add bind parameters (around line 170)

```rust
.bind(expectancy_percentage)
.bind(median_loss_percentage)
.bind(skill_pnl_ratio)
.bind(profitable_tokens_count)
.bind(profitable_tokens_threshold_pct)
.bind(median_roi_percentage)
.bind(avg_win_percentage)
.bind(avg_loss_percentage)
.bind(median_profit_usd)
.bind(top1_pnl_usd)
```

#### 2.4 Update PostgreSQL Client - SELECT

**File**: `/home/mrima/tytos/wallet-analyser/persistence_layer/src/postgres_client.rs`

**Step 1**: Update SELECT statement in `get_all_pnl_results_summary` (around line 395)

```sql
SELECT
    -- ... existing columns ...
    expectancy_percentage::float8,
    median_loss_percentage::float8,
    skill_pnl_ratio::float8,
    profitable_tokens_count,
    profitable_tokens_threshold_pct::float8,
    median_roi_percentage::float8,
    avg_win_percentage::float8,
    avg_loss_percentage::float8,
    median_profit_usd::float8,
    top1_pnl_usd::float8
FROM pnl_results
WHERE is_archived = false
ORDER BY analyzed_at DESC
```

**Step 2**: Parse fields when building StoredPortfolioPnLResultSummary (around line 449)

```rust
expectancy_percentage: row.get("expectancy_percentage"),
median_loss_percentage: row.get("median_loss_percentage"),
skill_pnl_ratio: row.get("skill_pnl_ratio"),
profitable_tokens_count: row.get::<Option<i32>, _>("profitable_tokens_count")
    .map(|v| v as u32).unwrap_or(0),
profitable_tokens_threshold_pct: row.get("profitable_tokens_threshold_pct"),
median_roi_percentage: row.get("median_roi_percentage"),
avg_win_percentage: row.get("avg_win_percentage"),
avg_loss_percentage: row.get("avg_loss_percentage"),
median_profit_usd: row.get("median_profit_usd"),
top1_pnl_usd: row.get("top1_pnl_usd"),
```

---

### Phase 3: API Layer
**Duration**: 0.5 days

#### 3.1 Update API Types

**File**: `/home/mrima/tytos/wallet-analyser/api_server/src/types.rs`
**Location**: Inside `StoredPnLResultSummary` struct (around line 405)

Add before the closing brace:

```rust
// Copy-trading metrics
#[serde(default)]
pub expectancy_percentage: f64,
#[serde(default)]
pub median_loss_percentage: f64,
#[serde(default)]
pub skill_pnl_ratio: f64,
#[serde(default)]
pub profitable_tokens_count: u32,
#[serde(default)]
pub profitable_tokens_threshold_pct: f64,
#[serde(default)]
pub median_roi_percentage: f64,
#[serde(default)]
pub avg_win_percentage: f64,
#[serde(default)]
pub avg_loss_percentage: f64,
#[serde(default)]
pub median_profit_usd: f64,
#[serde(default)]
pub top1_pnl_usd: f64,
```

#### 3.2 Update API Handlers

**File**: `/home/mrima/tytos/wallet-analyser/api_server/src/handlers.rs`
**Location**: In `get_all_results` handler (around line 939)

Update the field mapping when converting from persistence layer to API types:

```rust
expectancy_percentage: summary.expectancy_percentage,
median_loss_percentage: summary.median_loss_percentage,
skill_pnl_ratio: summary.skill_pnl_ratio,
profitable_tokens_count: summary.profitable_tokens_count,
profitable_tokens_threshold_pct: summary.profitable_tokens_threshold_pct,
median_roi_percentage: summary.median_roi_percentage,
avg_win_percentage: summary.avg_win_percentage,
avg_loss_percentage: summary.avg_loss_percentage,
median_profit_usd: summary.median_profit_usd,
top1_pnl_usd: summary.top1_pnl_usd,
```

**Note**: No query parameter filtering is needed since all filtering happens client-side.

---

### Phase 4: Frontend Implementation
**Duration**: 2 days

#### 4.1 Update TypeScript Interface

**File**: `/home/mrima/tytos/frontend/app/(dashboard)/results/page.tsx`
**Location**: Around line 52 (WalletResult interface)

Add to the interface:

```typescript
interface WalletResult {
  // ... existing fields ...

  // Copy-trading metrics
  expectancy_percentage?: number
  median_loss_percentage?: number
  skill_pnl_ratio?: number
  profitable_tokens_count?: number
  profitable_tokens_threshold_pct?: number
  median_roi_percentage?: number
  avg_win_percentage?: number
  avg_loss_percentage?: number
  median_profit_usd?: number
  top1_pnl_usd?: number
}
```

#### 4.2 Add Filter State Variables

**Location**: After line 126 (with other useState declarations)

```typescript
// Copy-trading metric filters
const [minExpectancy, setMinExpectancy] = useState<string>('')
const [maxExpectancy, setMaxExpectancy] = useState<string>('')
const [minMedianLoss, setMinMedianLoss] = useState<string>('')
const [maxMedianLoss, setMaxMedianLoss] = useState<string>('')
const [minSkillRatio, setMinSkillRatio] = useState<string>('')
const [maxSkillRatio, setMaxSkillRatio] = useState<string>('')
const [minProfitableTokens, setMinProfitableTokens] = useState<string>('')
const [profitableTokensThreshold, setProfitableTokensThreshold] = useState<string>('20')
const [minMedianROI, setMinMedianROI] = useState<string>('')
const [maxMedianROI, setMaxMedianROI] = useState<string>('')
```

#### 4.3 Update Filtering Logic

**Location**: Inside `filteredAndSortedResults` useMemo (around lines 260-376)

Add these filters after the existing filter logic:

```typescript
// Expectancy % filter
if (minExpectancy !== '') {
  filtered = filtered.filter(r =>
    (r.expectancy_percentage ?? 0) >= parseFloat(minExpectancy)
  )
}
if (maxExpectancy !== '') {
  filtered = filtered.filter(r =>
    (r.expectancy_percentage ?? 0) <= parseFloat(maxExpectancy)
  )
}

// Median Loss % filter
if (minMedianLoss !== '') {
  filtered = filtered.filter(r =>
    (r.median_loss_percentage ?? 0) >= parseFloat(minMedianLoss)
  )
}
if (maxMedianLoss !== '') {
  filtered = filtered.filter(r =>
    (r.median_loss_percentage ?? 0) <= parseFloat(maxMedianLoss)
  )
}

// Skill PnL Ratio filter
if (minSkillRatio !== '') {
  filtered = filtered.filter(r =>
    (r.skill_pnl_ratio ?? 0) >= parseFloat(minSkillRatio)
  )
}
if (maxSkillRatio !== '') {
  filtered = filtered.filter(r =>
    (r.skill_pnl_ratio ?? 0) <= parseFloat(maxSkillRatio)
  )
}

// Profitable Tokens count filter
if (minProfitableTokens !== '') {
  filtered = filtered.filter(r =>
    (r.profitable_tokens_count ?? 0) >= parseInt(minProfitableTokens)
  )
}

// Median ROI % filter
if (minMedianROI !== '') {
  filtered = filtered.filter(r =>
    (r.median_roi_percentage ?? 0) >= parseFloat(minMedianROI)
  )
}
if (maxMedianROI !== '') {
  filtered = filtered.filter(r =>
    (r.median_roi_percentage ?? 0) <= parseFloat(maxMedianROI)
  )
}
```

**Update dependency array**:
```typescript
}, [
  resultsData?.results,
  searchAddress,
  sortBy,
  sortOrder,
  minPnl,
  maxPnl,
  minWinRate,
  maxWinRate,
  // Add new dependencies
  minExpectancy,
  maxExpectancy,
  minMedianLoss,
  maxMedianLoss,
  minSkillRatio,
  maxSkillRatio,
  minProfitableTokens,
  minMedianROI,
  maxMedianROI,
  selectedChain
])
```

#### 4.4 Add Filter UI Inputs

**Location**: Inside the Advanced Filters section (around line 839)

Add a new collapsible section for copy-trading metrics:

```tsx
{/* Copy-Trading Metrics Filters */}
<div className="space-y-3">
  <h4 className="text-sm font-medium text-blue-ice flex items-center gap-2">
    <Target className="w-4 h-4" />
    Copy-Trading Metrics
  </h4>

  <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
    {/* Expectancy % */}
    <div>
      <label className="text-sm text-gray-400 mb-1 block">Min Expectancy %</label>
      <Input
        type="number"
        step="0.1"
        value={minExpectancy}
        onChange={(e) => setMinExpectancy(e.target.value)}
        placeholder="e.g. 5"
        className="bg-navy-deep border-blue-ice/20 text-white"
      />
    </div>
    <div>
      <label className="text-sm text-gray-400 mb-1 block">Max Expectancy %</label>
      <Input
        type="number"
        step="0.1"
        value={maxExpectancy}
        onChange={(e) => setMaxExpectancy(e.target.value)}
        placeholder="e.g. 50"
        className="bg-navy-deep border-blue-ice/20 text-white"
      />
    </div>

    {/* Median Loss % */}
    <div>
      <label className="text-sm text-gray-400 mb-1 block">Min Median Loss %</label>
      <Input
        type="number"
        step="0.1"
        value={minMedianLoss}
        onChange={(e) => setMinMedianLoss(e.target.value)}
        placeholder="e.g. -20"
        className="bg-navy-deep border-blue-ice/20 text-white"
      />
    </div>
    <div>
      <label className="text-sm text-gray-400 mb-1 block">Max Median Loss %</label>
      <Input
        type="number"
        step="0.1"
        value={maxMedianLoss}
        onChange={(e) => setMaxMedianLoss(e.target.value)}
        placeholder="e.g. -5"
        className="bg-navy-deep border-blue-ice/20 text-white"
      />
    </div>

    {/* Skill PnL Ratio */}
    <div>
      <label className="text-sm text-gray-400 mb-1 block">Min Skill Ratio</label>
      <Input
        type="number"
        step="0.01"
        value={minSkillRatio}
        onChange={(e) => setMinSkillRatio(e.target.value)}
        placeholder="e.g. 0.5"
        className="bg-navy-deep border-blue-ice/20 text-white"
      />
    </div>
    <div>
      <label className="text-sm text-gray-400 mb-1 block">Max Skill Ratio</label>
      <Input
        type="number"
        step="0.01"
        value={maxSkillRatio}
        onChange={(e) => setMaxSkillRatio(e.target.value)}
        placeholder="e.g. 1.0"
        className="bg-navy-deep border-blue-ice/20 text-white"
      />
    </div>

    {/* Profitable Tokens */}
    <div>
      <label className="text-sm text-gray-400 mb-1 block">
        Min Profitable Tokens (>{profitableTokensThreshold}%)
      </label>
      <Input
        type="number"
        value={minProfitableTokens}
        onChange={(e) => setMinProfitableTokens(e.target.value)}
        placeholder="e.g. 5"
        className="bg-navy-deep border-blue-ice/20 text-white"
      />
    </div>
    <div>
      <label className="text-sm text-gray-400 mb-1 block">Threshold %</label>
      <Input
        type="number"
        step="1"
        value={profitableTokensThreshold}
        onChange={(e) => setProfitableTokensThreshold(e.target.value)}
        placeholder="20"
        className="bg-navy-deep border-blue-ice/20 text-white"
      />
    </div>

    {/* Median ROI % */}
    <div>
      <label className="text-sm text-gray-400 mb-1 block">Min Median ROI %</label>
      <Input
        type="number"
        step="0.1"
        value={minMedianROI}
        onChange={(e) => setMinMedianROI(e.target.value)}
        placeholder="e.g. 10"
        className="bg-navy-deep border-blue-ice/20 text-white"
      />
    </div>
    <div>
      <label className="text-sm text-gray-400 mb-1 block">Max Median ROI %</label>
      <Input
        type="number"
        step="0.1"
        value={maxMedianROI}
        onChange={(e) => setMaxMedianROI(e.target.value)}
        placeholder="e.g. 100"
        className="bg-navy-deep border-blue-ice/20 text-white"
      />
    </div>
  </div>
</div>
```

#### 4.5 Add Metrics Display in Wallet Detail Modal

**Location**: Inside the wallet detail modal (around line 1396)

Add a new card section after the existing P&L overview:

```tsx
{/* Copy-Trading Metrics Card */}
<Card className="glass-card border-blue-ice/20">
  <CardHeader>
    <CardTitle className="text-lg font-bold text-white flex items-center gap-2">
      <Target className="w-5 h-5 text-purple-400" />
      Copy Trading Analysis
    </CardTitle>
  </CardHeader>
  <CardContent>
    <div className="grid grid-cols-2 md:grid-cols-3 gap-4">
      {/* Filterable Metrics */}
      <div className="space-y-1">
        <p className="text-sm text-gray-400">Expectancy %</p>
        <p className="text-lg font-semibold text-white">
          {selectedWallet?.expectancy_percentage?.toFixed(2) ?? '0.00'}%
        </p>
      </div>

      <div className="space-y-1">
        <p className="text-sm text-gray-400">Median Loss %</p>
        <p className="text-lg font-semibold text-red-400">
          {selectedWallet?.median_loss_percentage?.toFixed(2) ?? '0.00'}%
        </p>
      </div>

      <div className="space-y-1">
        <p className="text-sm text-gray-400">Skill Ratio</p>
        <p className="text-lg font-semibold text-white">
          {selectedWallet?.skill_pnl_ratio?.toFixed(3) ?? '0.000'}
        </p>
      </div>

      <div className="space-y-1">
        <p className="text-sm text-gray-400">
          Profitable (>{selectedWallet?.profitable_tokens_threshold_pct ?? 20}%)
        </p>
        <p className="text-lg font-semibold text-green-400">
          {selectedWallet?.profitable_tokens_count ?? 0} tokens
        </p>
      </div>

      <div className="space-y-1">
        <p className="text-sm text-gray-400">Median ROI %</p>
        <p className={`text-lg font-semibold ${
          (selectedWallet?.median_roi_percentage ?? 0) >= 0
            ? 'text-green-400'
            : 'text-red-400'
        }`}>
          {selectedWallet?.median_roi_percentage?.toFixed(2) ?? '0.00'}%
        </p>
      </div>

      {/* Display-Only Metrics */}
      <div className="space-y-1">
        <p className="text-sm text-gray-400">Avg Win / Loss %</p>
        <p className="text-sm font-semibold">
          <span className="text-green-400">
            {selectedWallet?.avg_win_percentage?.toFixed(2) ?? '0.00'}%
          </span>
          {' / '}
          <span className="text-red-400">
            {selectedWallet?.avg_loss_percentage?.toFixed(2) ?? '0.00'}%
          </span>
        </p>
      </div>

      <div className="space-y-1">
        <p className="text-sm text-gray-400">Median Profit (USD)</p>
        <p className="text-lg font-semibold text-green-400">
          ${(selectedWallet?.median_profit_usd ?? 0).toFixed(2)}
        </p>
      </div>

      <div className="space-y-1">
        <p className="text-sm text-gray-400">Top 1 P&L (USD)</p>
        <p className="text-lg font-semibold text-white">
          ${(selectedWallet?.top1_pnl_usd ?? 0).toFixed(2)}
        </p>
      </div>
    </div>
  </CardContent>
</Card>
```

#### 4.6 Add Per-Token ROI Table

**Location**: In the Token Breakdown section of wallet detail modal

This requires fetching the full wallet details (with token_results). Add this table after the metrics card:

```tsx
{/* Per-Token ROI Table */}
{walletDetails?.token_results && walletDetails.token_results.length > 0 && (
  <Card className="glass-card border-blue-ice/20">
    <CardHeader>
      <CardTitle className="text-lg font-bold text-white flex items-center gap-2">
        <TrendingUp className="w-5 h-5 text-blue-400" />
        Per-Token Performance
      </CardTitle>
    </CardHeader>
    <CardContent>
      <div className="overflow-x-auto">
        <table className="w-full">
          <thead>
            <tr className="border-b border-blue-ice/20">
              <th className="text-left py-2 px-3 font-medium text-gray-400">Token</th>
              <th className="text-right py-2 px-3 font-medium text-gray-400">Invested</th>
              <th className="text-right py-2 px-3 font-medium text-gray-400">Returned</th>
              <th className="text-right py-2 px-3 font-medium text-gray-400">ROI %</th>
              <th className="text-right py-2 px-3 font-medium text-gray-400">P&L</th>
              <th className="text-right py-2 px-3 font-medium text-gray-400">Trades</th>
              <th className="text-center py-2 px-3 font-medium text-gray-400">Result</th>
            </tr>
          </thead>
          <tbody>
            {walletDetails.token_results.map((token: any, idx: number) => {
              const roi = token.total_invested_usd > 0
                ? ((token.total_returned_usd / token.total_invested_usd - 1) * 100)
                : 0
              const isWin = roi > 0

              return (
                <tr key={idx} className="border-b border-blue-ice/10 hover:bg-blue-ice/5">
                  <td className="py-2 px-3 text-white font-mono text-sm">
                    {token.token_symbol || token.token_address.slice(0, 8)}
                  </td>
                  <td className="text-right py-2 px-3 text-gray-300">
                    ${token.total_invested_usd?.toFixed(2) ?? '0.00'}
                  </td>
                  <td className="text-right py-2 px-3 text-gray-300">
                    ${token.total_returned_usd?.toFixed(2) ?? '0.00'}
                  </td>
                  <td className={`text-right py-2 px-3 font-semibold ${
                    isWin ? 'text-green-400' : 'text-red-400'
                  }`}>
                    {roi.toFixed(2)}%
                  </td>
                  <td className={`text-right py-2 px-3 font-semibold ${
                    token.total_pnl_usd >= 0 ? 'text-green-400' : 'text-red-400'
                  }`}>
                    ${token.total_pnl_usd?.toFixed(2) ?? '0.00'}
                  </td>
                  <td className="text-right py-2 px-3 text-gray-300">
                    {token.total_trades ?? 0}
                  </td>
                  <td className="text-center py-2 px-3">
                    {isWin ? (
                      <CheckCircle className="w-5 h-5 text-green-400 inline" />
                    ) : (
                      <XCircle className="w-5 h-5 text-red-400 inline" />
                    )}
                  </td>
                </tr>
              )
            })}
          </tbody>
        </table>
      </div>
    </CardContent>
  </Card>
)}
```

**Add necessary imports** (at top of file around line 45):

```typescript
import { Target, TrendingUp, CheckCircle, XCircle } from 'lucide-react'
```

---

### Phase 5: Testing & Deployment
**Duration**: 1 day

#### 5.1 Run Migration

```bash
cd /home/mrima/tytos/wallet-analyser
psql $DATABASE_URL < migrations/add_copy_trading_metrics.sql
```

#### 5.2 Backend Testing

```bash
# Run unit tests
cargo test -p pnl_core calculate_copy_trading_metrics
cargo test -p pnl_core calculate_median

# Run full test suite
cargo test --workspace
```

#### 5.3 Integration Testing

1. Submit a batch job with known wallets
2. Verify metrics are calculated and stored in database
3. Check API response includes new fields
4. Verify frontend displays metrics correctly

#### 5.4 Frontend Testing

1. Test each filter independently
2. Test filter combinations
3. Test sorting by new metrics
4. Test wallet detail modal display
5. Test per-token ROI table

#### 5.5 Backfill Existing Records (Optional)

Create a script to re-analyze existing wallets:

```bash
# Get all wallet addresses
psql $DATABASE_URL -c "SELECT DISTINCT wallet_address FROM pnl_results WHERE expectancy_percentage = 0"

# Re-submit each for analysis via API
# POST /api/pnl/batch/run with existing wallet addresses
```

---

## Implementation Timeline

### Day 1: Backend Core
- Update `PortfolioPnLResult` struct
- Implement `calculate_median()` helper
- Implement `calculate_copy_trading_metrics()`
- Update `calculate_portfolio_pnl()`
- Write unit tests

### Day 2: Database + API
- Create and run migration
- Update `StoredPortfolioPnLResultSummary`
- Update `postgres_client.rs` INSERT/SELECT
- Update API types and handlers
- Verify data flow end-to-end

### Days 3-4: Frontend
- Update TypeScript interfaces
- Add filter state and UI inputs
- Update filtering logic in useMemo
- Add metrics display card
- Add per-token ROI table
- Test all UI interactions

### Day 5: Testing & Deployment
- Run full test suite
- Integration testing
- Performance testing
- Backfill existing records
- Deploy to production

**Total Duration**: 5 days

---

## Key Design Decisions

### 1. Frontend-Only Filtering
All filtering happens in the `filteredAndSortedResults` useMemo hook, matching the existing pattern. This keeps the backend simple and reduces database query complexity.

### 2. Per-Token ROI Calculation
Formula includes unrealized positions:
```
ROI% = ((total_returned_usd + unrealized_value) / total_invested_usd - 1) × 100
```

### 3. Exchange Currency Exclusion
Uses existing `is_exchange_currency_token()` function (lines 249-330) to skip:
- SOL (native Solana)
- USDC, USDT (stablecoins)
- Wrapped tokens

### 4. Default Threshold
20% for "profitable tokens" count (stored as `profitable_tokens_threshold_pct`)

### 5. Backward Compatibility
- All fields use `#[serde(default)]`
- Database columns have DEFAULT values
- Existing records show 0 until re-analyzed

### 6. Denormalized Storage
- All metrics stored in `pnl_results` table
- No joins needed for queries
- Calculations happen once during P&L analysis
- No database indexes needed (client-side filtering)

---

## Testing Strategy

### Unit Tests (pnl_core)
- Test `calculate_median()` with various inputs (empty, odd, even, single)
- Test metric calculations with edge cases:
  - All winners, no losers
  - All losers, no winners
  - Single token
  - Zero total P&L
  - Negative total P&L
- Test exchange currency filtering

### Integration Tests
- Submit batch job and verify metrics in database
- Verify API returns all new fields
- Test frontend state management

### Performance Tests
- Benchmark median calculation on large datasets (1000+ tokens)
- Test client-side filtering with 1000+ wallets
- Monitor render performance with filters active

### End-to-End Tests
1. Submit batch analysis → verify metrics calculated
2. Apply filters → verify correct results displayed
3. View wallet detail → verify all metrics shown
4. Export CSV → verify new columns included

---

## Migration & Rollback

### Migration Steps
1. Run SQL migration (adds columns with defaults)
2. Deploy backend code (pnl_core, persistence_layer)
3. Deploy API code (api_server types)
4. Deploy frontend code
5. Test with new batch jobs
6. (Optional) Backfill existing records

### Rollback Plan
- Migration is additive only (safe to rollback code)
- Columns have DEFAULT values (no NULL issues)
- Frontend handles missing fields with `??` operator
- Can deploy incrementally (backend first, then frontend)

---

## Notes

- **Exchange Currency Filtering**: SOL, USDC, USDT automatically excluded from metrics
- **Client-Side Performance**: Filtering 1000+ results is fast with useMemo
- **No API Changes**: No new query parameters needed
- **CSV Export**: Will automatically include new fields if exporter reads from StoredPnLResultSummary
- **Display-Only Metrics**: No filter inputs, only displayed in wallet detail modal
- **Per-Token Table**: Uses live calculation from full wallet details (requires separate API call)
