# The Real Bug - Cost Basis Inflation Analysis

## The Numbers Don't Lie

### What ACTUALLY Happened (from Zerion raw data):

**BUYS (Solano IN, SOL OUT):**
- Total SOL spent: **$7,128.87**
- Total Solano bought: **22,207,135 tokens**
- **Actual cost per token: $0.000321**

**SELLS (Solano OUT, SOL IN):**
- Total SOL received: **$1,339.82**
- Total Solano sold: **22,207,135 tokens**

**Net Result:**
- Spent: $7,128.87
- Received: $1,339.82
- Loss: $5,789.05
- Percentage: -81.2% loss

---

### What The System REPORTS:

- **Total invested: $13,871.97** ❌ (194% inflated!)
- **Total returned: $1,339.82** ✓ (Correct)
- **Total P&L: -$12,532.15** ❌ (Should be -$5,789.05)

**Remaining Position:**
- Quantity: 22,207,135 tokens
- **Cost basis: $0.000589 per token** ❌ (184% inflated!)
- Value: $13,073.56

---

## The Root Cause

### Cost Basis Comparison:
- **Actual**: $0.000321 per token ($7,128.87 / 22,207,135)
- **System**: $0.000589 per token
- **Inflation**: 1.84x = **84% too high!**

### Where $13,871.97 Comes From:
```
Matched trades (sold): $798.40
Remaining position:    $13,073.56  ← WRONG cost basis
                       ___________
Total invested:        $13,871.96
```

### Where It SHOULD Be:
```
Matched trades (sold): ~$5,789.05  (cost of sold tokens)
Remaining position:    ~$1,339.82  (cost of remaining tokens)
                       ___________
Total invested:        ~$7,128.87  ✓
```

---

## Why The Cost Basis Is Wrong

### The Matched Trades Show a Clue:

Looking at the 3 matched trades from transaction `AvQb9vkf`:
1. Buy: 12,005,534 tokens @ $0.0000337 = $404.99
2. Buy: 702,739 tokens @ $0.0000386 = $27.10
3. Buy: 9,498,861 tokens @ $0.0000386 = $366.31

**Problem**: Transaction `AvQb9vkf` only has **ONE** Solano IN transfer (12,005,534 tokens), not THREE!

### What's Happening:

The parser is creating **MULTIPLE BUY events** from transactions that should create **ONE** BUY event.

**Transaction `AvQb9vkf`**:
- Raw data: 1 IN (12,005,534 Solano), 3 OUT (SOL totaling $409.83)
- Parser creates: **3 BUY events?** (12,005,534 + 702,739 + 9,498,861)
- Sum of parsed buys: 22,207,134 tokens (equals TOTAL from all transactions!)

This suggests the parser is either:
1. Splitting single transfers into multiple events incorrectly
2. Creating events from the SOL OUT transfers as if they were separate buys
3. Mixing up transactions somehow

---

## The Impact

When cost basis is inflated by 1.84x:
- Unrealized P&L is calculated using wrong cost
- Total invested is 84% higher than reality
- P&L appears much worse than it actually is (-81% → looks like -90%)

---

## What Needs to be Investigated

1. **Why does AvQb9vkf create 3 BUY events instead of 1?**
   - Are the 3 SOL OUT transfers being converted to BUY events?
   - Is the trade pairing splitting transactions incorrectly?

2. **Where do the quantities 702,739 and 9,498,861 come from?**
   - These don't appear in transaction AvQb9vkf
   - Do they come from OTHER transactions?
   - Is there transaction ID confusion?

3. **Why is the remaining position cost basis $0.000589 instead of $0.000321?**
   - This is the core issue causing the inflation
   - Need to trace which BUY events contribute to remaining position

---

## Next Steps

1. **Enable DEBUG logging** and run the parser on this wallet
2. **Examine EVERY BUY event** created from each transaction
3. **Trace the remaining position** calculation to see which buys are unmatched
4. **Identify phantom BUY events** that shouldn't exist

The fix I implemented (summing ALL stable transfers) should help, but we need to verify it actually creates the correct number of events with correct quantities.
