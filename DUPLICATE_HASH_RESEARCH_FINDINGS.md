# Duplicate Hash Research Findings & Analysis

## Executive Summary

**Root Cause Identified:** Negative hold times in P&L calculations are caused by duplicate transaction hash entries in BirdEye API data that get parsed as separate financial events, breaking FIFO matching chronology.

**Key Finding:** Duplicate hashes represent legitimate multi-step swaps within single blockchain transactions, NOT API duplicates.

## Original Problem

- **Symptom:** Negative hold times (-26.8 days average for SOL trades)
- **Location:** P&L results for wallet `5ngDQ3vMT7GHeFvXP5GLX79t6fqG2t15kyQTtst9zFdw`
- **Impact:** FIFO matching algorithm produces incorrect chronological ordering

## Research Methodology

### Phase 1: Data Collection (Completed)
- **Original wallet:** 10 BirdEye entries → 8 unique tx_hashes → 1 duplicate hash (3 entries)
- **Sample wallets:** 300+ transactions from 3 active wallets
- **Offset analysis:** Fetched data with offsets 0, 100, 200 to detect patterns

### Phase 2: Pattern Analysis (Completed)

#### Transaction Type Classification:
1. **Type A - Simple Transactions (99%+ cases):**
   - 1 BirdEye entry per blockchain transaction
   - Clear quote/base structure
   - Single DEX source
   - Consistent pricing

2. **Type B - Complex Multi-DEX Routing (<1% cases):**
   - Multiple BirdEye entries per blockchain transaction
   - Same tx_hash across entries
   - Different DEX sources (raydium_clamm, meteora_dlmm)
   - Different instruction indices
   - Legitimate sub-operations within single atomic transaction

## Critical Findings

### Original Problematic Transaction Analysis
**Transaction Hash:** `2dETabf2vq1fL1JEaAPyjbxJiKdxgijo5V7hPBSVd5b7HFXNMouSLyqijXvye1H64hnHhk5AG9q5vZeG1WGw4cCZ`

**Raw Structure (3 entries):**
1. **Entry 1 (Raydium CLAMM):** 3.996 SOL → 33,317.67 MASHA
2. **Entry 2 (Meteora DLMM):** 0.666 SOL → 5,580.64 MASHA  
3. **Entry 3 (Meteora DLMM):** 2.738 SOL → 23,099.21 MASHA

**Net Result:** 7.40 SOL → 61,997.52 MASHA (single atomic buy transaction)

**Parser Problem:** Creates 6 separate financial events instead of 2 (1 buy pair)

### Offset Analysis Results

#### Wallet 1 (`8vMeKNeECuwE2t5YufLZ3YhJBN1YjtdsckpuLbMEkCWt`)
- **Offset 0:** 100 transactions, 100 unique hashes, 0 duplicates
- **Offset 100:** 100 transactions, 100 unique hashes, 0 duplicates  
- **Offset 200:** 100 transactions, 98 unique hashes, **1 duplicate hash** (3 entries)
- **Cross-offset overlap:** 0 (no same transaction across different offsets)

#### Wallet 2 (`6guFU4jXxNzfmsjgR6WmKEe2NRe6P4EBKkJmx3rpQ9SA`)
- **All offsets:** 300 transactions, 300 unique hashes, 0 duplicates

#### Duplicate Hash Detail Analysis
**Hash:** `56sHWKWK8F2QVV8a6ZE52JkBPVywVSvq6bX4gWMSjMvM1ENHkMwhz67NjACa9NYCFWsYtyP6t89JADGYT7tW9pP1`

**Structure:**
- **Same:** tx_hash, timestamp (1750844699), source (raydium), address
- **Different:** ins_index values (4, 5, 6), amounts, volumes
- **Direction:** All SOL → conquer token (consistent)

**Verdict:** Legitimate multi-step swap, NOT API duplicate

## Current Parser Logic Problem

```rust
// Current logic in job_orchestrator/src/lib.rs ~line 886
// Creates TWO FinancialEvents - one for each side of the transaction
// This treats each BirdEye entry as separate transaction
```

**Issue:** Each BirdEye entry → 2 financial events → Artificial buy/sell pairs

**Example:** 
- Original: 1 blockchain transaction (3 BirdEye entries)
- Parser creates: 6 financial events  
- Should create: 2 financial events (1 consolidated buy pair)

## Trading Pattern Analysis

### Actual Trading Sequence (Corrected)
1. **June 24, 2025:** 7 BUY transactions (35.65 SOL → 290,988 MASHA)
2. **July 20, 2025:** 1 SELL transaction (288,631 MASHA → 18.17 SOL)
3. **Net position:** 2,357 MASHA remaining, -17.48 SOL loss
4. **Hold time:** 26.83 days (positive, as expected)

### Current Parser Problem
- Creates artificial SOL buy events from MASHA→SOL sale
- FIFO tries to match these against future SOL sells
- Results in negative hold times because chronology is inverted

## Proposed Solution Architecture

### Two-Path Consolidation Algorithm

```rust
fn consolidate_transactions(raw_txs: Vec<GeneralTraderTransaction>) -> Vec<ConsolidatedTransaction> {
    let tx_groups = group_by_hash(raw_txs);
    
    for (tx_hash, entries) in tx_groups {
        if entries.len() == 1 {
            // FAST PATH: Simple transaction (99%+ cases)
            consolidated.push(convert_simple_transaction(entries[0]));
        } else {
            // COMPLEX PATH: Multi-entry consolidation (<1% cases)
            consolidated.push(consolidate_multi_entry_transaction(tx_hash, entries));
        }
    }
}
```

### Multi-Entry Consolidation Logic

1. **Calculate Net Flows:**
   ```rust
   for entry in entries {
       update_token_flow(token_flows, entry.quote);
       update_token_flow(token_flows, entry.base);
   }
   ```

2. **Identify Primary Tokens:**
   - Outflow token: Largest absolute outflow (negative)
   - Inflow token: Largest absolute inflow (positive)

3. **Weighted Average Pricing:**
   ```rust
   weighted_price = total_value / total_amount
   ```

4. **Create Consolidated Structure:**
   ```rust
   ConsolidatedTransaction {
       quote: outflow_token,  // "from"
       base: inflow_token,    // "to"
       metadata: preserve_routing_info(entries)
   }
   ```

## Validation Requirements

### Data Integrity Checks
- ✅ Net flow conservation: Sum of all token flows = 0
- ✅ Price reasonableness: Weighted prices within market bounds
- ✅ Timestamp consistency: All entries have same block_unix_time
- ✅ Quote/base assignment: Clear inflow/outflow direction

### Expected Results
- **Before:** 10 BirdEye entries → 20 financial events → Negative hold times
- **After:** 10 BirdEye entries → 8 consolidated transactions → 16 financial events → Positive hold times

## Implementation Status

### Completed
- [x] Root cause identification
- [x] Data collection (300+ transactions)
- [x] Pattern analysis and classification
- [x] Offset-based duplicate investigation
- [x] Algorithm design

### Pending
- [ ] Complete offset analysis for wallet 3
- [ ] Implement consolidation algorithm
- [ ] Test with original problematic wallet
- [ ] Validate P&L accuracy
- [ ] Performance testing

## Key Insights for Implementation

1. **Efficiency Priority:** 99%+ of transactions are simple and should use fast path
2. **Accuracy Critical:** Multi-entry consolidation must preserve all financial information
3. **Metadata Preservation:** Keep routing information for audit trail
4. **Error Handling:** Graceful fallback for unexpected patterns
5. **FIFO Compatibility:** Existing P&L algorithm remains unchanged

## Next Steps

1. **Complete research:** Finish wallet 3 analysis
2. **Implement consolidation:** Build two-path algorithm
3. **Test rigorously:** Validate against original problematic case
4. **Deploy carefully:** Ensure no regressions in simple cases

---

*Research conducted: July 25, 2025*
*Original issue: Negative hold times in P&L calculations*
*Status: Algorithm designed, implementation pending*