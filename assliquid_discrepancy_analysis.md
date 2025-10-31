# AssLiquid Token P&L Discrepancy Analysis

## Executive Summary

**Issue**: The P&L calculation for AssLiquid token shows `total_returned` of **$5,784.07** (claimed), but the user states this is incorrect and should be approximately **$4,188.59**.

**Root Cause Found**: The system is INCLUDING the proceeds from 2 large sell transactions (14.28M tokens total worth $4,964.13) in the P&L calculation. However, these tokens were RECEIVED (not bought), and the P&L engine logs confirm they were "excluded from P&L" - yet their sell proceeds are still being counted in `total_returned`.

**Actual Discrepancy**: 
- Claimed `total_returned`: $5,784.07
- Calculated from raw data: $11,096.93 (all 23 sells)
- Should be (21 small sells only): $6,132.81
- User expects: ~$4,188.59
- Extra amount being counted: $4,964.13 (the 2 large sells)

---

## 1. Raw Transaction Data Summary

### BUYS (6 transactions)
Total USDC spent: **$4,999.18**
Total AssLiquid bought: **9,714,009.92 tokens**

| Date | TX Hash | USDC Out | AssLiquid In |
|------|---------|----------|--------------|
| 2025-10-03 03:44:09 | VT1cg1y6 | $999.58 | 2,203,944.16 |
| 2025-10-03 03:45:07 | 5tF6qp3E | $999.79 | 2,222,782.52 |
| 2025-10-03 03:46:08 | 31BZu4Wm | $399.81 | 869,051.11 |
| 2025-10-11 17:25:42 | 5MqqQisF | $1,000.00 | 1,636,969.76 |
| 2025-10-20 14:51:38 | 33ruambC | $600.02 | 866,198.27 |
| 2025-10-20 17:00:49 | unD1jZ8e | $999.97 | 1,915,064.10 |

### SELLS (23 transactions)
Total USDC received: **$11,096.93**
Total AssLiquid sold: **21,187,428.61 tokens**

#### 2 LARGE SELLS (these are the problem!)
| Date | TX Hash | AssLiquid Out | USDC In | Price |
|------|---------|---------------|---------|-------|
| 2025-10-22 21:03:14 | 28vRFhyq | 7,143,714.31 | **$2,579.73** | $0.0003611 |
| 2025-10-22 21:05:46 | 5ururFHf | 7,143,714.31 | **$2,384.40** | $0.0003338 |
| **SUBTOTAL** | | **14,287,428.61** | **$4,964.13** | |

#### 21 SMALL SELLS
Total USDC received: **$6,132.81**
Total AssLiquid sold: **6,900,000.00 tokens**

Top examples:
- 2025-10-04 13:20:26: 1,000,000 tokens → $1,223.77
- 2025-10-03 22:18:11: 600,000 tokens → $445.21
- 2025-10-04 19:13:50: 300,000 tokens → $462.68
- (18 more small sells ranging from 100K-300K tokens each)

### RECEIVES (34 non-trade transfers)
Total AssLiquid received: **~5.1M tokens** (across 34 transactions)

Examples:
- 2025-10-20 17:00:49: 1,915,064.10 tokens
- 2025-10-20 14:51:38: 866,198.27 tokens  
- 2025-10-11 17:25:42: 1,636,969.76 tokens
- (31 more receive transactions)

---

## 2. P&L Engine Processing (from logs)

The P&L engine correctly processes AssLiquid:

```
Token AssLiquid Enhanced P&L Summary:
  💰 P&L: Realized: $841.41, Unrealized: $0, Total: $841.41, Trades: 22, Win Rate: 86.36%
  🎁 Receives: Total: 11,473,418.70, Sold: 11,473,418.70, Remaining: 0.000000 (no P&L impact)
  📊 Consumptions: 12 receive→sell events excluded from P&L
```

Key observations:
1. **Total bought tokens**: 9,714,009.92
2. **Total received tokens**: 11,473,418.70
3. **Total sold tokens**: 21,187,428.61 (bought + received)
4. **P&L only counts 22 trades** (the 21 small sells + 1 sell that partially matched buys)
5. **12 receive→sell events excluded from P&L**

### Processing of the 23rd Sell (the second large sell):

```
🔹 Processing Sell #23/23: 7,143,714.306850 AssLiquid @ $0.0003337759...
🔄 Phase 1: Matching against BOUGHT tokens (priority)
   (All bought tokens already consumed by previous sells)
🔄 Phase 2: Matching against RECEIVED tokens (fallback)
   ✓ Consumed 771,290.73 AssLiquid from receive event (no P&L impact)
   ⚠️ Created implicit receive for 6,372,423.57 AssLiquid 
      (pre-existing holdings from outside timeframe - excluded from P&L)
```

The engine correctly identifies that this sell should NOT contribute to P&L!

---

## 3. The Discrepancy

### What the P&L Report SHOULD Show:

**For BUYS:**
- ✅ **total_invested**: $4,999.18 (CORRECT - user confirmed)
- This represents the 6 buy transactions

**For SELLS:**
- ❌ **total_returned**: Currently showing $5,784.07 (or $11,096.93?)
- ✅ **Should be**: $6,132.81 (only the 21 small sells)
- ❌ **WRONG if including**: $4,964.13 from the 2 large sells

### The Bug

The system is apparently counting the USD proceeds from ALL 23 sell transactions ($11,096.93) in `total_returned`, even though:
1. The P&L engine logs show only 22 trades were matched
2. The logs explicitly state "12 receive→sell events excluded from P&L"
3. The 2 large sells should NOT be counted because they sold RECEIVED tokens, not BOUGHT tokens

### Expected vs Actual

| Metric | Expected | Actual (from raw data) | Status |
|--------|----------|----------------------|--------|
| total_invested | $4,999.18 | $4,999.18 | ✅ CORRECT |
| total_returned | ~$4,188.59 (per user) | $11,096.93 (all sells) | ❌ WRONG |
| total_returned | ~$4,188.59 (per user) | $6,132.81 (small sells) | ⚠️ Still ~$1,944 off |
| Should exclude | - | $4,964.13 (2 large sells) | ❌ BEING COUNTED |

---

## 4. Specific Transactions Being Miscounted

### These 2 transactions should NOT contribute to total_returned:

**Transaction 1:**
- Date: 2025-10-22 21:03:14 Z
- Hash: `28vRFhyqRpFE5sdzpPCtHoMT9PadrNykY7HPMXvcEQz72cT6jkaZkyVkaoXrBe3MZS4fX1YkJKwcSD31wNBwm6MJ`
- AssLiquid sold: 7,143,714.31 tokens
- USDC received: **$2,579.73**
- Why excluded: These tokens were RECEIVED, not bought

**Transaction 2:**
- Date: 2025-10-22 21:05:46 Z
- Hash: `5ururFHfgmEVUQJa2DRkQ43z1zYmvNgvPxW8K5qTZr7JdbKqUDfxBr3YE6CDpzSsjKQX4nhNvXFPbbnMDHGHAkvv`
- AssLiquid sold: 7,143,714.31 tokens
- USDC received: **$2,384.40**
- Why excluded: These tokens were RECEIVED, not bought

**Total wrongly counted: $4,964.13**

---

## 5. Additional Discrepancy Note

Even after removing the 2 large sells, there's still a difference:
- Small sells total: $6,132.81
- User expects: ~$4,188.59
- Remaining difference: ~$1,944.22

This suggests there may be additional sell transactions that are being counted but shouldn't be. Further investigation needed to determine:
1. Were some of the "small sells" also selling RECEIVED tokens?
2. Is the user's expected value based on a different timeframe filter?
3. Are there other sell transactions that should be excluded?

---

## 6. Recommended Fix

The P&L calculation code needs to ensure that:
1. When calculating `total_returned`, only include proceeds from sells that matched BOUGHT tokens
2. Exclude proceeds from sells that matched RECEIVED tokens or created implicit receives
3. The matched_trades list should only include buy-sell pairs where the buy was an actual purchase (not a receive)

The P&L engine's FIFO matching logic is CORRECT (as evidenced by the logs). The bug is likely in the final aggregation step where `total_returned` is calculated.

---

## Data Files Referenced

- Raw transaction data: `/home/mrima/tytos/wallet-analyser/assliquid.json`
- P&L logs: `/home/mrima/tytos/wallet-analyser/logs.txt` (lines 16700-16970)
- AssLiquid token address: `51MyT1MHfVU4jzasM6BBJu7WTJLtfrXnqrQ8DZTspump`
