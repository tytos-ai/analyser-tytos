# Deep Code Analysis - No Assumptions

## FACTS from token_6AiuSc3pYM5rpbKPq8JfSQoMoUsfDpzvP5PmMSEKpump_after_fix.json

### Matched Trades Analysis
All 3 matched trades have BUY events with:
- **SAME transaction hash**: AvQb9vkfQzHL8J4hHWzGtGc1VZmWBWfLHHoFnFFA6JD7CqmfENNn48FKwSmxyD5yzq2TGFWQL4TLA3d4s7w85nL
- **SAME timestamp**: 2025-10-23T11:42:43Z

**Trade 1**:
- Buy: 12,005,534.953920 @ $0.0000341371 = $409.83
- Sell: 12,005,534.953920 @ $0.0000213434 = $256.24 (tx: 36Ws79Xfs...)

**Trade 2**:
- Buy: 702,739.030368 @ $0.0000385638 = $27.10
- Sell: 702,739.030368 @ $0.0000213434 = $15.00 (tx: 36Ws79Xfs...)

**Trade 3**:
- Buy: 9,498,861.256355 @ $0.0000385638 = $366.31
- Sell: 9,498,861.256355 @ $0.0001124958 = $1068.58 (tx: 261axxdq...)

**Key Observations**:
1. ONE transaction (AvQb9vkf at 2025-10-23T11:42:43Z) generated 3 different BUY events with different quantities
2. Quantities: 12,005,534 + 702,739 + 9,498,861 = 22,207,135 tokens total
3. Two different prices used: $0.0000341371 and $0.0000385638
4. Remaining position also has 22,207,135 tokens → **DOUBLE COUNTING CONFIRMED**

### Total Accounting
- **20 buy events** created (from how many actual Zerion transactions?)
- **Total bought**: 44,414,270 tokens
- **Matched/sold**: 22,207,135 tokens
- **Remaining**: 22,207,135 tokens
- **Issue**: Should be ~10 Zerion buy transactions, not 20 events

## CODE TO ANALYZE

### 1. Parser Code (zerion_client/src/lib.rs)
Need to trace:
- `convert_to_financial_events()` - Main entry point
- `pair_trade_transfers()` - Groups transfers by act_id
- `convert_trade_pair_to_events()` - Converts pairs to events
- `convert_transfer_with_implicit_price()` - Creates events with calculated price
- `convert_transfer_to_event()` - Creates events from individual transfers

**Question**: Why does ONE Zerion transaction create MULTIPLE events with DIFFERENT quantities?

### 2. PNL Matching Code (pnl_core/src/new_pnl_engine.rs)
Need to trace:
- How buy events are collected
- How FIFO matching works
- How remaining position is calculated
- Why 22M tokens appear in BOTH matched AND remaining

**Question**: Is the matching logic creating duplicate events, or is it receiving duplicates from the parser?

## ANALYSIS PLAN

### Step 1: Find Raw Zerion Data
- Locate actual Zerion API response for this wallet
- Count how many transactions Zerion returns
- Identify which transactions are for Solano token
- Understand the structure of each transaction

### Step 2: Trace Parser Execution
- Add detailed logging to see EXACTLY what events are created
- For transaction AvQb9vkf, trace:
  - How many transfers it contains
  - How they're grouped into trade pairs
  - Why 3 events are created instead of 1

### Step 3: Trace PNL Matching
- Understand how events are fed to the matching engine
- Verify if 20 events are received or 10 events get duplicated
- Check if remaining position calculation has a bug

### Step 4: Identify Root Cause
Only after completing steps 1-3, identify the exact line(s) of code causing the bug.

## HYPOTHESIS (TO BE VERIFIED)
Based on the fact that ONE transaction creates 3 events with different quantities and prices:
- Hypothesis A: The parser sees multiple transfers within transaction AvQb9vkf and creates separate events for each
- Hypothesis B: The parser processes the same transaction multiple times
- Hypothesis C: The transaction has complex swap logic that's being incorrectly parsed

**DO NOT ASSUME - MUST VERIFY WITH CODE ANALYSIS**
