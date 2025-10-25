# Solscan Transaction Analysis

## Transaction #1 (AvQb9vkf) - BUY ✓
```
Swap: 2.149948112 SOL ($414.81) → 12,005,534.95392 Solano
Type: BUY (SOL OUT, Solano IN)
```
**Expected**: 1 BUY event for 12,005,534 tokens
**System creates**: 3 BUY events (12,005,534 + 702,739 + 9,498,861) ❌

---

## Transaction #2 (261axxdq) - SELL ❌ Treated as BUY!
```
Swap: 9,498,861.256355 Solano → 5.652574121 WSOL ($1,089.19)
Fee: 0.056525741 SOL ($10.89) to Axiom Fees Vault
Type: SELL (Solano OUT, WSOL/SOL IN)
```
**Expected**: 1 SELL event for 9,498,861 tokens
**System creates**: 1 BUY event for 9,498,861 tokens assigned to tx AvQb9vkf ❌

**This is matched trade #3:**
- Buy: 9,498,861 @ $366.31 (WRONG - should not exist!)
- Sell: 9,498,861 @ $1,068.58 (from tx 261axxdq) ✓
- P&L: +$702.27 (fake profit from phantom buy)

---

## Transaction #3 (36Ws79Xf) - SELL ✓
```
Swap: 12,708,273.984288 Solano → 1.464195783 WSOL ($282.13)
Fee: 0.014641957 SOL ($2.82) to Axiom Fees Vault
Type: SELL (Solano OUT, WSOL IN)
```
**Expected**: 1 SELL event for 12,708,273 tokens
**System appears to handle this correctly** ✓

---

## THE BUG REVEALED

### What's Happening:
1. **AvQb9vkf** (real buy of 12M tokens) creates **3 BUY events** with different quantities
2. The quantity **9,498,861** from tx **261axxdq** (a SELL) is being created as a **BUY event**
3. This phantom BUY is assigned the wrong transaction hash (**AvQb9vkf** instead of **261axxdq**)

### The Result:
- Parser creates BUY events from SELL transactions
- Assigns them the wrong transaction hash
- Creates phantom matched trades with fake profits
- Causes double-counting (same tokens counted as bought AND remaining)

### Root Cause:
The Zerion parser is confusing transaction directions:
- Transaction 261axxdq has Solano direction="out" (selling)
- But parser creates a BUY event (should be SELL)
- Likely issue: direction is being inverted or event type is wrong

### Where to Look:
`zerion_client/src/lib.rs` in `convert_transfer_to_event` or `convert_trade_pair_to_events`:
- Lines 1543-1580: Event type determination based on direction
- Line 1545-1547: "trade" operation with "in"/"out" direction logic
- May be inverting the direction or misinterpreting "out" as "in"
