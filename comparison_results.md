# P&L Implementation Comparison Results

## Summary
Both implementations processed the same wallet address `MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa` with 100 transactions, but show significant differences in results.

## Python Reference Implementation Results
- **Transaction Count**: 100
- **Financial Events Generated**: 111 
- **Event Breakdown**: 72 BUY events, 39 SELL events
- **Realized P&L**: +1.101708993040896851831543476 SOL
- **Unrealized P&L**: 0 SOL (no current prices calculated)
- **Total Invested**: 177.9318770118133614735488270 SOL
- **Total Withdrawn**: 190.2865650223278210348957264 SOL
- **Active Positions**: 13 different tokens

## Rust API Server Results
- **Transaction Count**: 100 (same input)
- **Financial Events Generated**: 111 (same as Python)
- **Event Breakdown**: Not explicitly stated, but can be inferred from token breakdown
- **Realized P&L**: -0.0103277220959840096378649949 USD
- **Unrealized P&L**: 32.581824473185472743007677752 USD
- **Total P&L**: 32.571496751089488733369812757 USD
- **Total Trades**: 95 (vs 111 events, suggesting some aggregation)
- **Win Rate**: 0% (0 winning trades, 6 losing trades)
- **ROI**: -41.01%

## Key Differences Identified

### 1. **Currency Units**
- **Python**: Results in SOL
- **Rust**: Results in USD
- **Impact**: Cannot directly compare numerical values

### 2. **Transaction vs Event Count**
- **Python**: 111 events from 100 transactions
- **Rust**: 95 trades from 100 transactions
- **Analysis**: Rust may be aggregating some events or filtering differently

### 3. **P&L Calculation Approach**
- **Python**: Simple FIFO with fallback SOL price of $155
- **Rust**: Uses current market prices for unrealized P&L calculation
- **Impact**: Rust provides more accurate current valuations

### 4. **Unrealized P&L Handling**
- **Python**: Set to 0 (no current price fetching)
- **Rust**: Fetches current prices for accurate unrealized P&L
- **Impact**: Rust provides complete P&L picture

### 5. **Trade Counting Logic**
- **Python**: Counts individual BUY/SELL events
- **Rust**: May be counting complete trade cycles or using different logic

## Potential Issues to Investigate

### 1. **Event Generation Discrepancy**
- Both show 111 events, but Rust shows 95 trades
- Need to investigate if Rust is aggregating or filtering events

### 2. **Currency Conversion**
- Python uses fixed $155 SOL price
- Rust uses dynamic pricing
- Need to verify if same price sources are used

### 3. **Realized P&L Sign Difference**
- Python shows +1.10 SOL profit
- Rust shows -0.01 USD loss
- This could be due to:
  - Different SOL/USD conversion rates
  - Different event classification
  - Different cost basis calculations

### 4. **Position Tracking**
- Python shows 13 active positions
- Rust shows detailed current holdings with live prices
- Both should have similar position counts if logic is consistent

## Recommendations for Further Investigation

1. **Standardize Currency Units**: Convert Python results to USD using same exchange rate as Rust
2. **Event-by-Event Comparison**: Compare the first 10 events from both implementations
3. **Price Source Verification**: Ensure both use same price sources and timestamps
4. **FIFO Logic Verification**: Confirm both use identical FIFO cost basis calculation
5. **Transaction Filtering**: Check if Rust applies any filtering that Python doesn't

## Conclusion

The implementations show the same event count (111) but different trade counts and P&L values. The most significant difference is the currency unit (SOL vs USD) and the inclusion of current market prices in the Rust implementation. A deeper investigation is needed to determine if the core P&L calculation logic is equivalent when normalized for currency and pricing differences.