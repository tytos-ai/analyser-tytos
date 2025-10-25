# Transaction AvQb9vkf Detailed Analysis

## Solscan Blockchain Data

**Transaction**: AvQb9vkfQzHL8J4hHWzGtGc1VZmWBWfLHHoFnFFA6JD7CqmfENNn48FKwSmxyD5yzq2TGFWQL4TLA3d4s7w85nL
**Type**: BUY (Swap SOL for Solano)
**Result**: Success

---

## Token Transfers (Actual Blockchain)

### Solano IN (The Buy):
```
FROM: Pump.fun Bonding Curve
TO:   BAr5cs...xZXJPh (wallet)
QTY:  12,005,534.95392 Solano
```
**This is the ONLY Solano transfer in this transaction!**

### SOL OUT (Payment + Fees):
1. **Main swap payment**:
   - TO: Pump.fun Bonding Curve
   - QTY: 2.123405542 SOL ($409.69)

2. **Protocol fee**:
   - TO: Pump.fun AMM Protocol Fee 2
   - QTY: 0.020172353 SOL ($3.89)

3. **Axiom fee**:
   - TO: Axiom Fees Vault
   - QTY: 0.021716647 SOL ($4.19)

4. **Additional fee**:
   - TO: axmD4L...EXj8dG
   - QTY: 0.004 SOL ($0.77)

5. **Other transfer**:
   - TO: AhkFAQ...XQ5tEc
   - QTY: 0.006370217 SOL ($1.22)

**Total SOL OUT: ~2.175 SOL (~$419.76)**

---

## What Should Be Created

**From this ONE transaction:**
- **1 BUY event**: 12,005,534 Solano @ implicit price from total SOL spent
- **Multiple SELL events**: For each SOL OUT transfer (if tracking SOL)

**Expected events for Solano token:**
- **BUY**: 12,005,534 Solano, cost = $419.76

---

## What The System ACTUALLY Creates

**From matched trades in analysis:**
1. Buy: 12,005,534 tokens @ $0.0000337 = $404.99 ✓ (correct qty, slightly wrong price)
2. Buy: 702,739 tokens @ $0.0000386 = $27.10 ❌ (phantom buy!)
3. Buy: 9,498,861 tokens @ $0.0000386 = $366.31 ❌ (phantom buy from tx 261axxdq!)

**All 3 buys have transaction_hash = AvQb9vkf**

**Total phantom buys from this tx: 702,739 + 9,498,861 = 10,201,600 tokens**

---

## The Mystery: Where Does 702,739 Come From?

Checking all buy transactions from Zerion:
- AvQb9vkf: 12,005,534 ✓
- 4bFRCbDc: 541,359
- 61pzvkYx: 2,503,750
- 4LiHFP8c: 2,643,985
- 2VDMcCvX: 2,785,793
- 4FtQvwxg: 1,023,973
- pFv56Q2j: 140,827
- 57knQgNB: 299,088
- 659ZJNcE: 131,402
- 3kMvevxm: 131,420

**702,739 is NOT in any Zerion transaction!**

This quantity must be calculated/derived somehow by the parser.

---

## Hypothesis

The parser might be:
1. Taking the Solano IN quantity: 12,005,534
2. Subtracting something to get: 702,739
3. Calculation: 12,005,534 - ??? = 702,739
   - Difference: 11,302,795 tokens

OR:
1. Splitting the 12M tokens across the 3 SOL OUT transfers
2. Proportionally assigning quantities based on SOL amounts

Let me test the proportional theory:
- SOL transfer 1: $404.99 / $419.76 * 12,005,534 = 11,587,815 tokens
- SOL transfer 2: $4.09 / $419.76 * 12,005,534 = 117,009 tokens  
- SOL transfer 3: $0.77 / $419.76 * 12,005,534 = 22,010 tokens

**That doesn't match either!**

The quantities 702,739 and 9,498,861 must be coming from OTHER transactions somehow being mixed into this one.

---

## Critical Questions

1. **Why does AvQb9vkf create 3 BUY events?**
2. **Where does the quantity 702,739 come from?** (Not in any Zerion transaction)
3. **Why is the 9,498,861 quantity from tx 261axxdq (a SELL) being created as a BUY under AvQb9vkf?**
4. **Is the parser somehow combining multiple transactions into one?**

---

## Next Steps

Need to add extensive DEBUG logging to trace:
1. How many events are created from transaction AvQb9vkf
2. What quantities each event has
3. What transaction hash is assigned to each event
4. Whether trade pairing is grouping transfers from multiple transactions incorrectly

The answer lies in `pair_trade_transfers` and `convert_trade_pair_to_events`.
