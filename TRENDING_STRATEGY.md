# Alternative Trending Token & Trader Discovery Strategy

## Problem Statement

Since DexScreener's WebSocket endpoint for real-time trending pairs is blocked for programmatic access (403 Forbidden), we need an alternative strategy to:

1. **Identify trending token pairs**
2. **Discover active trader wallets** for those pairs

## Solution: HTTP API-Based Trending Discovery

### Phase 1: Trending Token Identification

#### 🎯 Primary Strategy: Boosted Tokens Analysis

**Rationale**: Boosted tokens indicate market interest since users pay to promote them.

**Implementation**:
```rust
// 1. Fetch boosted tokens
GET https://api.dexscreener.com/token-boosts/latest/v1
GET https://api.dexscreener.com/token-boosts/top/v1

// 2. Get trading data for each boosted token
GET https://api.dexscreener.com/token-pairs/v1/solana/{token_address}
```

**Trending Criteria** (based on analysis):
- **Volume Threshold**: > $1.27M in 24h
- **Transaction Threshold**: > 45,000 txns/24h  
- **Minimum Liquidity**: > $10,000
- **Price Change**: > 50% (optional filter for high volatility)

#### 🔄 Secondary Strategy: Polling & Volume Analysis

**Rationale**: Regular polling can identify tokens with sudden volume/activity spikes.

**Implementation**:
1. **Search Popular Tokens**: Use search endpoint with common terms
2. **Monitor Volume Changes**: Track volume increases over time
3. **Activity Spikes**: Monitor transaction count increases

### Phase 2: Trader Wallet Discovery

#### 🔍 Challenge: No Direct Trader Data in DexScreener API

DexScreener API **does not provide**:
- Individual trader wallet addresses
- Transaction details with wallet addresses  
- Top traders or whale activity data

#### ✅ Solution: Solana RPC Integration

**Strategy**: Use trending pair addresses to fetch transaction data via Solana RPC

```rust
// 1. Get trending pair address from DexScreener
let pair_address = "A7Z2aTBCcBrEmWFrP2jCpzKdiwHAJhdbWiuXdqjyuyew"; // STOPWAR/SOL

// 2. Use Solana RPC to get recent transactions
rpc_client.get_signatures_for_address(&pair_address, Some(GetSignaturesForAddressConfig {
    limit: Some(100),
    before: None,
    until: None,
    commitment: Some(CommitmentConfig::confirmed()),
}))

// 3. Parse transaction details to extract wallet addresses
// 4. Analyze wallet activity patterns
```

### Phase 3: Implementation Architecture

#### 🏗️ System Components

1. **DexScreener Monitor** (`dex_client` crate):
   ```rust
   // Periodic polling (every 60 seconds to respect rate limits)
   async fn poll_trending_tokens() {
       let boosted = fetch_boosted_tokens().await?;
       let trending = analyze_trending_criteria(boosted).await?;
       publish_to_redis_queue(trending).await?;
   }
   ```

2. **Solana Transaction Analyzer** (`solana_client` + `tx_parser`):
   ```rust
   // For each trending pair, get recent transactions
   async fn discover_traders(pair_address: &str) {
       let signatures = get_recent_signatures(pair_address).await?;
       let wallets = extract_wallet_addresses(signatures).await?;
       filter_active_traders(wallets).await?;
   }
   ```

3. **Redis Queue System** (`persistence_layer`):
   ```rust
   // Queue discovered wallets for P&L analysis
   redis_client.lpush("wallet_discovery_queue", wallet_addresses).await?;
   ```

#### 📊 Data Flow

```
DexScreener Boosted Tokens → Trending Analysis → Pair Addresses
                                                       ↓
Redis Queue ← Wallet Filtering ← Transaction Analysis ← Solana RPC
     ↓
P&L Analysis Pipeline
```

### Current API Test Results

#### ✅ Working Endpoints

| Endpoint | Rate Limit | Data Quality |
|----------|------------|--------------|
| `/token-boosts/latest/v1` | 60/min | ✅ Fresh boosted tokens |
| `/token-boosts/top/v1` | 60/min | ✅ High-value boosts |
| `/token-pairs/v1/solana/{token}` | 300/min | ✅ Complete trading data |
| `/latest/dex/search` | 300/min | ✅ Search functionality |

#### 📈 Sample Trending Tokens (Current):

1. **STOPWAR/SOL** - Pair: `A7Z2aTBCcBrEmWFrP2jCpzKdiwHAJhdbWiuXdqjyuyew`
   - Volume: $2.4M/24h, Txns: 82,699, Change: +129%

2. **ZENAI/SOL** - Pair: `6JaPZRPZYbNwwRVPXodGWZvHzkQpxBcb9QVtjJiGUGwx`
   - Volume: $2.7M/24h, Txns: 47,283, Change: +48.7%

3. **MESH/SOL** - Pair: `(address from analysis)`
   - Volume: $1.2M/24h, Txns: 101,296, Change: -23.5%

### Implementation Timeline

#### Week 1: Core DexScreener Integration
- [ ] Implement boosted token fetching in `dex_client`
- [ ] Create trending analysis logic
- [ ] Set up Redis queuing system
- [ ] Add rate limiting and error handling

#### Week 2: Solana RPC Integration  
- [ ] Implement transaction fetching for pairs
- [ ] Create wallet address extraction logic
- [ ] Add trader activity filtering
- [ ] Integrate with existing P&L pipeline

#### Week 3: Optimization & Monitoring
- [ ] Add performance monitoring
- [ ] Implement fallback strategies
- [ ] Create trending pair dashboard endpoints
- [ ] Load testing and optimization

### Rate Limiting Strategy

**DexScreener Limits**:
- Token boosts: 60 requests/minute
- Pair data: 300 requests/minute  
- Search: 300 requests/minute

**Implementation**:
```rust
// Staggered polling to maximize efficiency
async fn staggered_polling() {
    // Every 60 seconds: fetch new boosted tokens (1 req/min)
    // Every 10 seconds: analyze 5 top tokens (30 req/min)  
    // Every 5 seconds: get pair data for 1 token (12 req/min)
}
```

### Expected Performance

**Trending Detection Latency**: 60-120 seconds  
**Wallet Discovery Rate**: ~50-100 wallets/hour per trending pair  
**System Load**: Low (HTTP polling vs WebSocket streaming)  
**Data Freshness**: 1-2 minutes behind real-time  

### Fallback Strategies

1. **If DexScreener API fails**: Use Jupiter API to monitor price changes
2. **If rate limits hit**: Implement exponential backoff
3. **If no trending tokens**: Fall back to predefined popular pairs
4. **If Solana RPC fails**: Queue requests for retry

### Key Advantages

✅ **Reliable**: Uses official documented APIs  
✅ **Sustainable**: Respects rate limits  
✅ **Comprehensive**: Gets both trending tokens AND trader wallets  
✅ **Flexible**: Can adjust trending criteria based on market conditions  
✅ **Scalable**: Can add more data sources easily  

### Key Limitations

❌ **Not Real-time**: 1-2 minute delay vs WebSocket  
❌ **Rate Limited**: Cannot poll as frequently as WebSocket  
❌ **Indirect**: Must derive trending status from boost data  
❌ **Complex**: Requires Solana RPC integration for wallet discovery  

## Conclusion

While the WebSocket approach would be ideal, this HTTP-based strategy provides a **robust, sustainable alternative** that can effectively identify trending tokens and discover active trader wallets for P&L analysis.

The key insight is leveraging **boosted tokens as a trending signal** and combining it with **Solana RPC transaction analysis** for comprehensive trader discovery.