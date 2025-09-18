# P&L Calculation Algorithm Documentation

## Overview

This document describes the comprehensive P&L (Profit & Loss) calculation algorithm used for analyzing cryptocurrency trading performance from transaction data. The algorithm is designed as a two-component system: Data Preparation & Parsing and P&L Engine, designed for high-performance parallel processing of multiple wallets.

## Version History
- **v2.0** (Current): Enhanced price validation, exchange currency filtering, phantom buy improvements
- **v1.0** (Original): Basic FIFO matching with universal token treatment

## Sample Transaction Data

Below are sample transactions in the format received from data providers (BirdEye, Zerion, etc.):

### Transaction 1: SOL → BONK
```json
{
  "quote": {
    "symbol": "SOL",
    "address": "So11111111111111111111111111111111111111112",
    "ui_change_amount": -3.54841245,
    "price": 150.92661594596476
  },
  "base": {
    "symbol": "Bonk",
    "address": "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263",
    "ui_change_amount": 31883370.79991,
    "price": 1.6796824680689412e-05
  },
  "tx_hash": "VKQDkkQ3V6zHayKvmXXmMJVuBWqnaQdUDgkAdPmr9nEa1tkiLZaZvhzkM1gim865EnXxVomSNM1TcBxHDyi5AW7",
  "block_unix_time": 1751614209,
  "volume_usd": 535.5498830590299
}
```

### Transaction 2: SOL → BONK
```json
{
  "quote": {
    "symbol": "Bonk",
    "address": "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263",
    "ui_change_amount": 8927067.47374,
    "price": 1.6796824680689412e-05
  },
  "base": {
    "symbol": "SOL",
    "address": "So11111111111111111111111111111111111111112",
    "ui_change_amount": -0.993505263,
    "price": 150.92661594596476
  },
  "tx_hash": "tpo9yxyeoaaVCr8E8nyLiNr7HoTpEvafLTWjRcXEXz7vKwozPjUzGWx5hUVBj6xot7RRSWmfkbFjdfR7PWPgqmu",
  "block_unix_time": 1751614209,
  "volume_usd": 149.9463872690957
}
```

### Transaction 3: SOL → ai16z
```json
{
  "quote": {
    "symbol": "ai16z",
    "address": "HeLp6NuQkmYB4pYWo2zYs22mESHXPQYzXbB8n4V98jwC",
    "ui_change_amount": 980.476464445,
    "price": 0.15288455027765796
  },
  "base": {
    "symbol": "SOL",
    "address": "So11111111111111111111111111111111111111112",
    "ui_change_amount": -0.993194709,
    "price": 150.9268041464183
  },
  "tx_hash": "ftDsDy9qg1FH7F66PJhY2STdzLhUDqti5N9VVEsCBjs4DSfvGgNKcabaY44LN3bWehJLx727D84r6GFDHEcycmM",
  "block_unix_time": 1751614208,
  "volume_usd": 149.89970332450193
}
```

### Transaction 4: SOL → ai16z
```json
{
  "quote": {
    "symbol": "ai16z",
    "address": "HeLp6NuQkmYB4pYWo2zYs22mESHXPQYzXbB8n4V98jwC",
    "ui_change_amount": 2204.775487409,
    "price": 0.15287039634817054
  },
  "base": {
    "symbol": "SOL",
    "address": "So11111111111111111111111111111111111111112",
    "ui_change_amount": -2.233309111,
    "price": 150.91726485995815
  },
  "tx_hash": "43LzpzTH27ihxnqEdGpeFVCZaYV6i4ZvTMnqzDooH2X3H9XHAPfwcd2836Q5apNvsboyURWSHVBDXHinXyLRpafJ",
  "block_unix_time": 1751614207,
  "volume_usd": 337.0449026189447
}
```

## Algorithm Components

### Component 1: Data Preparation & Parsing Module

#### Purpose
Process raw Birdeye transaction data into standardized financial events. This component is designed for parallel processing of multiple wallets simultaneously.

#### Parallelism Design
- **Wallet-level parallelism**: Process multiple wallets concurrently
- **Transaction batching**: Handle large transaction sets efficiently
- **Stateless processing**: Each wallet can be processed independently
- **Thread-safe operations**: Support concurrent execution across multiple threads

#### Core Algorithm Steps

**Step 1: Financial Event Creation**
For every single transaction:
- Examine both `quote` and `base` sides
- Check the `ui_change_amount` sign for each side
- **Negative amount → SELL event** (token disposed of)
- **Positive amount → BUY event** (token acquired)
- **Always create exactly 2 events per transaction** (one buy, one sell)
- Validate and use embedded price data for USD value calculation

**Step 2: Price Validation & Event Standardization**

**Price Data Validation**:
The engine validates all price data to detect and handle corrupted values:
- **25% Deviation Threshold**: When `price` and `nearest_price` differ by >25%, uses the more reliable value
- **Fallback Strategy**: Automatically falls back to `nearest_price` when main price appears corrupted
- **Zero/Negative Handling**: Rejects negative prices, allows zero with careful handling

Each financial event contains:
- Wallet address
- Token address and symbol
- Event type (BUY or SELL)
- Quantity (absolute value of ui_change_amount)
- USD price at transaction time (validated)
- USD value (quantity × validated_price)
- Timestamp (block_unix_time)
- Transaction hash

**Critical Note on Absolute Values**: 
SELL events have negative `ui_change_amount` values (representing token balance decrease), while BUY events have positive values (representing token balance increase). We use absolute values for quantities to ensure mathematical consistency:

- **Without absolute value**: SELL of 3.548 SOL @ $150.93 = -3.548 × $150.93 = -$535.55 (nonsensical negative value)
- **With absolute value**: SELL of 3.548 SOL @ $150.93 = |−3.548| × $150.93 = $535.55 (correct economic value)

This ensures that:
1. All quantities represent meaningful positive numbers
2. USD values reflect actual economic amounts exchanged
3. P&L calculations use consistent mathematical operations
4. The direction (buy vs sell) is captured in the event type, not the quantity sign

**Step 3: Data Handoff**
Make standardized financial events immediately available to the P&L Engine through the chosen data interface:
- **Direct memory passing**: For single-process implementations
- **Message queues**: For distributed processing
- **Database storage**: For persistent processing pipelines
- **In-memory caches**: For high-performance scenarios

The specific implementation is flexible and depends on system architecture requirements.

### Component 2: P&L Engine Module

#### Purpose
Calculate comprehensive P&L metrics by consuming financial events and performing token-by-token analysis. This component is designed for concurrent processing of multiple wallet P&L calculations.

#### Parallelism Design
- **Concurrent wallet processing**: Calculate P&L for multiple wallets simultaneously
- **Token-level parallelism**: Within each wallet, process different tokens concurrently
- **Independent calculations**: Each wallet's P&L calculation is completely independent
- **Scalable architecture**: Support horizontal scaling across multiple processors/machines

#### Core Algorithm Steps

**Step 1: Event Retrieval**
- Retrieve all financial events for a wallet from the data source
- Group events by token address
- Sort events chronologically by timestamp (with transaction hash as secondary sort for stability)

**Step 2: Token-by-Token FIFO Matching**
For each token separately:
- Separate BUY events and SELL events
- Sort both lists chronologically
- Match each SELL against the oldest available BUY using FIFO principle
- Calculate realized P&L for each matched pair: `(sell_price - buy_price) × quantity`

**Step 3: Phantom Buy Creation (Enhanced)**
For any SELL event with no corresponding BUY:
- **Create a phantom BUY event**:
  - Timestamp: Exactly 1 second before the sell event
  - Price: Same as sell price (ensuring zero P&L)
  - Transaction Hash: Format: `phantom_buy_{original_tx_hash}`
- **Investment Exclusion**: Phantom buys are filtered from `total_invested_usd` calculations
- This creates **zero P&L** for unmatched sells
- Rationale: Unknown original cost basis, so assume break-even

**Step 4: Trade Metrics Calculation**

**Trade Definition**: 1 Trade = 1 FIFO matched pair (1 BUY + 1 SELL)

**Win Rate Calculation**:
- Count profitable trades (P&L > 0)
- Count losing trades (P&L < 0)
- Win Rate = (Winning trades / Total trades) × 100%

**Hold Time Calculation**:
- For each matched pair: Hold time = Sell timestamp - Buy timestamp
- Calculate average, minimum, and maximum hold times per token

**Step 5: Unrealized P&L Calculation**
For remaining token balances after FIFO matching:
- Fetch current market price for each token
- Calculate unrealized P&L: `(current_price - weighted_avg_cost_basis) × remaining_quantity`

**Step 6: Portfolio Aggregation**
- Sum realized P&L across all tokens
- Sum unrealized P&L across all tokens
- Calculate overall win rate from all trades
- Total P&L = Realized P&L + Unrealized P&L

## Key Algorithm Principles

1. **Smart Token Classification**: While all tokens go through the same P&L calculation, the engine intelligently classifies tokens:
   - **Exchange Currency Detection**: Identifies tokens used as trading pairs (USDC, USDT, WETH, etc.)
   - **Pattern Recognition**: Detects exchange currencies via trading patterns (<0.1 min avg hold time, ~$0 P&L)
   - **Portfolio Filtering**: Prevents double-counting of exchange currencies in portfolio totals
2. **Transaction-Level Processing**: Every transaction creates exactly 2 events (buy + sell)
3. **FIFO Matching**: Within each token, sells are matched against buys chronologically with temporal constraints
4. **Zero-Cost Assumption**: Unmatched sells create phantom buys at same price for break-even P&L
5. **Real-Time Pricing**: Current market prices (optionally with real wallet balances) for unrealized P&L
6. **Token-by-Token Analysis**: P&L calculated per token, then aggregated with intelligent filtering

## Architecture & Performance Considerations

### Parallelism Strategy
1. **Wallet-level parallelism**: Both components can process multiple wallets concurrently
2. **Component independence**: Data Preparation and P&L Engine can run simultaneously for different wallets
3. **Token-level concurrency**: Within P&L Engine, different tokens can be processed in parallel
4. **Stateless design**: All operations are stateless, enabling maximum parallelization

### Data Flow
1. **Raw transactions** → Data Preparation Module (parallel processing)
2. **Price validation** → Detects and handles corrupted price data
3. **Standardized financial events** → Data handoff layer (flexible implementation)
4. **Financial events** → P&L Engine Module (concurrent processing)
5. **Token classification** → Exchange currency detection and filtering
6. **Comprehensive P&L metrics** → Final output with investment accuracy

### Scalability Features
- **Horizontal scaling**: Components can be distributed across multiple machines
- **Memory efficiency**: Process wallets independently to minimize memory footprint
- **Concurrent execution**: Support for threading, async processing, and distributed computing
- **Flexible data interface**: Adaptable to various data storage and messaging solutions

## Output Metrics

### Per Token:
- Realized P&L
- Unrealized P&L
- Win rate
- Total trades
- Average hold time
- Remaining position

### Portfolio Level:
- Total realized P&L
- Total unrealized P&L
- Overall P&L
- Overall win rate
- Total trades across all tokens
- Number of tokens analyzed

## Edge Cases & Error Handling

### Same-Sign Transactions
- **Scenario**: Both quote and base have same sign (both positive/negative)
- **Handling**: Transaction rejected with descriptive error
- **Note**: May need future enhancement for complex DeFi operations (airdrops, liquidity additions)

### Timestamp Collisions
- **Stable Sort**: Uses transaction hash as secondary sort key when timestamps are identical
- **Deterministic**: Ensures reproducible results with same input data

### Zero Price Tokens
- **Allowed**: Zero prices accepted (for new/worthless tokens)
- **Validation**: Negative prices rejected to prevent calculation errors

### Phantom Buy Time Conflicts
- **Current**: Uses fixed 1-second offset before sell event
- **Consideration**: Future versions may use nanoseconds or unique counters for better isolation

## Implementation-Specific Features

### Data Quality
- Custom deserializers handle null/missing fields gracefully
- Decimal precision throughout (no floating-point errors)
- Robust error handling with detailed logging

### Performance Optimizations
- Efficient FIFO matching with remaining quantity tracking
- Parallel processing capabilities maintained
- Optional real balance fetching for unrealized P&L accuracy

### Investment Metrics Accuracy
- Correctly excludes phantom transactions from ROI calculations
- Filters exchange currency operations from investment metrics
- Calculates streak analytics and hold time statistics
- Separates `total_invested_usd` from exchange currency swaps

## Maintenance Notes
- Exchange currency address list requires periodic updates as new stablecoins/wrapped tokens emerge
- Price deviation threshold (25%) may need adjustment for highly volatile markets
- Consider nanosecond timestamps for phantom buys in high-frequency trading scenarios
- Transaction hash handling assumes uniqueness (no longer needs deduplication)

This algorithm ensures comprehensive P&L analysis while maintaining separation of concerns between data processing and calculation engines. The architecture is designed for maximum performance through parallelism and flexible enough to adapt to various implementation approaches and scaling requirements.