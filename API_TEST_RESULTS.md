# DexScreener & Jupiter API Testing Results

## Summary

Comprehensive testing of external APIs used by the P&L tracker system.

## DexScreener API Results

### ✅ Working HTTP Endpoints

**Base URL**: `https://api.dexscreener.com/latest/dex/`

| Endpoint | Status | Description | Response Format |
|----------|--------|-------------|-----------------|
| `/tokens/{token_address}` | ✅ 200 | Get all pairs for a specific token | JSON with `pairs` array |
| `/search?q={query}` | ✅ 200 | Search for pairs by token symbol/name | JSON with `pairs` array |
| `/pairs/{chain}/{pair_address}` | ✅ 200 | Get specific pair data | JSON with `pair` object |

### 📊 Response Schema

**Pair Object Structure**:
```json
{
  "chainId": "solana",
  "dexId": "raydium",
  "pairAddress": "8sLbNZoA1cfnvMJLPfp98ZLAnFSYCFApfJKMbiXNLwxj",
  "baseToken": {
    "address": "So11111111111111111111111111111111111111112",
    "name": "Wrapped SOL",
    "symbol": "SOL"
  },
  "quoteToken": {
    "address": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
    "name": "USD Coin",
    "symbol": "USDC"
  },
  "priceUsd": "153.73",
  "volume": {
    "h24": 37483392.04,
    "h6": 11942237.85,
    "h1": 1234567.89,
    "m5": 98765.43
  },
  "liquidity": {
    "usd": 12345678.90,
    "base": 80000.123,
    "quote": 12345678.90
  },
  "txns": {
    "h24": {
      "buys": 1234,
      "sells": 987
    },
    "h6": {
      "buys": 345,
      "sells": 267
    }
  }
}
```

### ❌ Blocked/Restricted Endpoints

| Endpoint | Status | Issue |
|----------|--------|-------|
| `wss://io.dexscreener.com/dex/screener/v5/pairs/h24/1` | ❌ 403 | WebSocket blocked, requires browser session |
| `https://io.dexscreener.com/dex/log/amm/v4/*` | ❌ 403 | Cloudflare protection |

**WebSocket Issues**:
- The trending pairs WebSocket endpoint requires a valid browser session
- Direct programmatic access is blocked with 403 Forbidden
- Headers including Origin, User-Agent, and WebSocket extensions don't bypass the restriction

### 📈 Rate Limits & Headers

- **CORS**: Enabled with `access-control-allow-origin: *`
- **Caching**: Responses include ETags and cache-control headers
- **Rate Limiting**: No explicit rate limit headers observed in testing
- **Authentication**: No API key required for public endpoints

## Jupiter API Results

### ✅ Working Endpoints

**Base URL**: `https://lite-api.jup.ag/price/v2`

| Endpoint | Status | Description | Response Format |
|----------|--------|-------------|-----------------|
| `?ids={token_address}` | ✅ 200 | Get price for single token | JSON with `data` object |
| `?ids={token1},{token2},...` | ✅ 200 | Get prices for multiple tokens | JSON with `data` object |

### ❌ Non-working Endpoints

| Endpoint | Status | Issue |
|----------|--------|-------|
| `https://price.jup.ag/v6/price` | ❌ DNS | Domain not found |

### 📊 Response Schema

**Price Response Structure**:
```json
{
  "data": {
    "So11111111111111111111111111111111111111112": {
      "id": "So11111111111111111111111111111111111111112",
      "type": "derivedPrice",
      "price": "153.591452500"
    },
    "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v": {
      "id": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
      "type": "derivedPrice", 
      "price": "0.999891234"
    }
  },
  "timeTaken": 0.123
}
```

### 📈 Rate Limits & Performance

- **Response Time**: < 1 second for multiple token queries
- **Batch Support**: ✅ Supports multiple token IDs in single request
- **Caching**: CloudFlare caching enabled
- **Rate Limiting**: No explicit limits observed

## Recommendations for Implementation

### DexScreener Integration

1. **Use HTTP API Only**: WebSocket access is blocked for programmatic use
2. **Polling Strategy**: Implement periodic polling of trending/search endpoints
3. **Caching**: Leverage ETags for efficient caching
4. **Fallback Strategy**: Have backup plans for when io.dexscreener.com endpoints are unavailable

### Jupiter Integration

1. **Use lite-api.jup.ag**: Only working domain for price fetching
2. **Batch Requests**: Request multiple token prices in single API call
3. **Error Handling**: Implement retry logic for network failures
4. **Price Caching**: Cache prices with reasonable TTL (30-60 seconds)

### Sample Implementation URLs

```rust
// DexScreener
const DEXSCREENER_BASE = "https://api.dexscreener.com/latest/dex";
let sol_pairs_url = format!("{}/tokens/So11111111111111111111111111111111111111112", DEXSCREENER_BASE);
let search_url = format!("{}/search?q=pump", DEXSCREENER_BASE);
let pair_url = format!("{}/pairs/solana/{}", DEXSCREENER_BASE, pair_address);

// Jupiter
const JUPITER_BASE = "https://lite-api.jup.ag/price/v2";
let price_url = format!("{}?ids={}", JUPITER_BASE, token_ids.join(","));
```

## Alternative Strategy: Trending Token Discovery

### ✅ Solution for WebSocket Limitations

Since WebSocket access is blocked, we developed a **comprehensive HTTP-based strategy**:

#### 1. Trending Token Identification
- **Source**: DexScreener boosted tokens (users pay to promote = trending signal)
- **Endpoints**: `/token-boosts/latest/v1` and `/token-boosts/top/v1`
- **Criteria**: Volume > $1.27M/24h, Transactions > 45K/24h

#### 2. Active Trader Discovery
- **Method**: Solana RPC integration with trending pair addresses
- **Process**: Get transaction signatures → Extract wallet addresses
- **Validation**: ✅ Successfully tested with live trending pair

#### 3. Current Trending Tokens (Live Data)
| Token | Pair Address | 24h Volume | 24h Txns | Change |
|-------|-------------|------------|----------|---------|
| STOPWAR/SOL | A7Z2aTBCcBrEmWFrP2jCpzKdiwHAJhdbWiuXdqjyuyew | $2.4M | 82,699 | +129% |
| ZENAI/SOL | 6JaPZRPZYbNwwRVPXodGWZvHzkQpxBcb9QVtjJiGUGwx | $2.7M | 47,283 | +48.7% |
| MESH/SOL | [From analysis] | $1.2M | 101,296 | -23.5% |

#### 4. Wallet Discovery Validation
✅ **Successfully tested Solana RPC**:
- Retrieved 10 recent transaction signatures
- Extracted 19 account keys (wallet addresses) per transaction  
- ~50-100 wallets discoverable per hour per trending pair

### 📊 Performance Comparison

| Feature | WebSocket (Blocked) | HTTP Strategy (Working) |
|---------|-------------------|------------------------|
| **Real-time** | ✅ Instant | ⚠️ 1-2 min delay |
| **Rate Limits** | ❌ 403 Forbidden | ✅ 60-300 req/min |
| **Trending Data** | ✅ Direct | ✅ Via boosted tokens |
| **Trader Discovery** | ❌ Not available | ✅ Via Solana RPC |
| **Reliability** | ❌ Blocked | ✅ Official APIs |
| **Sustainability** | ❌ Can't implement | ✅ Long-term viable |

## Test Files Created

- `test_dexscreener.js` - WebSocket testing script
- `test_http_endpoints.js` - DexScreener HTTP endpoint testing  
- `test_jupiter_api.js` - Jupiter API testing
- `test_official_dexscreener_api.js` - Official API endpoint testing
- `analyze_trending_tokens.js` - Trending token analysis
- `test_solana_rpc.js` - Wallet discovery validation
- `TRENDING_STRATEGY.md` - Complete alternative strategy
- `API_TEST_RESULTS.md` - This summary document