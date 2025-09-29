# System Interaction Guide - Curl Commands

This guide provides ready-to-use curl commands for interacting with the Wallet Analyzer system. Simply copy and paste these commands into your terminal.

## Prerequisites

1. **Start the API Server**
```bash
cargo run -p api_server
```

2. **Verify Server is Running**
```bash
curl -s http://localhost:8080/health
```

---

## 1. System Configuration

### Check Current Configuration
```bash
curl -s http://localhost:8080/api/config | jq
```

### Configure Services for Discovery + Analysis
```bash
curl -X POST http://localhost:8080/api/services/config \
  -H "Content-Type: application/json" \
  -d '{
    "enable_wallet_discovery": true,
    "enable_pnl_analysis": true
  }'
```

### Configure Discovery Only (For Testing)
```bash
curl -X POST http://localhost:8080/api/services/config \
  -H "Content-Type: application/json" \
  -d '{
    "enable_wallet_discovery": true,
    "enable_pnl_analysis": false
  }'
```

### Configure Analysis Only (Manual Discovery)
```bash
curl -X POST http://localhost:8080/api/services/config \
  -H "Content-Type: application/json" \
  -d '{
    "enable_wallet_discovery": false,
    "enable_pnl_analysis": true
  }'
```

---

## 2. Service Management

### NEW: Universal Service Control (Recommended)

#### Start Wallet Discovery with Default Configuration
```bash
curl -X POST http://localhost:8080/api/services/control \
  -H "Content-Type: application/json" \
  -d '{
    "action": "start",
    "service": "wallet_discovery"
  }'
```

#### Start P&L Analysis Service
```bash
curl -X POST http://localhost:8080/api/services/control \
  -H "Content-Type: application/json" \
  -d '{
    "action": "start",
    "service": "pnl_analysis"
  }'
```

#### Restart Wallet Discovery Service
```bash
curl -X POST http://localhost:8080/api/services/control \
  -H "Content-Type: application/json" \
  -d '{
    "action": "restart",
    "service": "wallet_discovery"
  }'
```

#### Stop Services
```bash
# Stop wallet discovery
curl -X POST http://localhost:8080/api/services/control \
  -H "Content-Type: application/json" \
  -d '{
    "action": "stop",
    "service": "wallet_discovery"
  }'

# Stop P&L analysis
curl -X POST http://localhost:8080/api/services/control \
  -H "Content-Type: application/json" \
  -d '{
    "action": "stop",
    "service": "pnl_analysis"
  }'
```

### Legacy Service Control (Still Supported)

#### Start Wallet Discovery Service
```bash
curl -X POST http://localhost:8080/api/services/discovery/start
```

#### Start P&L Analysis Service
```bash
curl -X POST http://localhost:8080/api/services/pnl/start
```

#### Start Both Services (Discovery + Analysis)
```bash
# Start discovery first
curl -X POST http://localhost:8080/api/services/discovery/start

# Wait a moment, then start analysis
sleep 2
curl -X POST http://localhost:8080/api/services/pnl/start
```

#### Stop Wallet Discovery Service
```bash
curl -X POST http://localhost:8080/api/services/discovery/stop
```

#### Stop P&L Analysis Service
```bash
curl -X POST http://localhost:8080/api/services/pnl/stop
```

#### Stop All Services
```bash
curl -X POST http://localhost:8080/api/services/discovery/stop
curl -X POST http://localhost:8080/api/services/pnl/stop
```

---

## 3. System Monitoring

### Check Service Status
```bash
curl -s http://localhost:8080/api/services/status | jq
```

### Check Service Status (Formatted)
```bash
curl -s http://localhost:8080/api/services/status | jq '{
  wallet_discovery: .data.wallet_discovery.state,
  queue_size: .data.wallet_discovery.queue_size,
  pnl_analysis: .data.pnl_analysis.state,
  wallets_processed: .data.pnl_analysis.wallets_processed
}'
```

### Monitor Queue Size Only
```bash
curl -s http://localhost:8080/api/services/status | jq '.data.wallet_discovery.queue_size'
```

### Check System Health
```bash
curl -s http://localhost:8080/health
```

---

## 4. Batch P&L Analysis

### Submit Batch Job with Time-Based Filtering (NEW - Recommended)

#### Analyze Last 7 Days
```bash
curl -X POST http://localhost:8080/api/pnl/batch/run \
  -H "Content-Type: application/json" \
  -d '{
    "wallet_addresses": [
      "ARZfRQgaoVjbHqg7p6M4uFPcJwEqwgUPT5hvfMkV8JNy",
      "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa"
    ],
    "chain": "solana",
    "time_range": "7d"
  }'
```

#### Analyze Last 1 Hour (Recent Activity)
```bash
curl -X POST http://localhost:8080/api/pnl/batch/run \
  -H "Content-Type: application/json" \
  -d '{
    "wallet_addresses": ["ARZfRQgaoVjbHqg7p6M4uFPcJwEqwgUPT5hvfMkV8JNy"],
    "chain": "solana",
    "time_range": "1h"
  }'
```

#### Analyze Last 30 Days
```bash
curl -X POST http://localhost:8080/api/pnl/batch/run \
  -H "Content-Type: application/json" \
  -d '{
    "wallet_addresses": [
      "ARZfRQgaoVjbHqg7p6M4uFPcJwEqwgUPT5hvfMkV8JNy",
      "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa"
    ],
    "chain": "solana",
    "time_range": "30d"
  }'
```

### Submit Batch Job (Basic - Most Recent 1000 Transactions)
```bash
curl -X POST http://localhost:8080/api/pnl/batch/run \
  -H "Content-Type: application/json" \
  -d '{
    "wallet_addresses": [
      "ARZfRQgaoVjbHqg7p6M4uFPcJwEqwgUPT5hvfMkV8JNy",
      "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa"
    ],
    "chain": "solana"
  }'
```

### Submit Batch Job (With Transaction Limit)
```bash
curl -X POST http://localhost:8080/api/pnl/batch/run \
  -H "Content-Type: application/json" \
  -d '{
    "wallet_addresses": [
      "ARZfRQgaoVjbHqg7p6M4uFPcJwEqwgUPT5hvfMkV8JNy",
      "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa"
    ],
    "chain": "solana",
    "max_transactions": 200
  }'
```

**Note:** When `time_range` is provided, ALL transactions within that period are fetched (ignoring `max_transactions`)

### Submit Batch Job (Multiple Wallets)
```bash
curl -X POST http://localhost:8080/api/pnl/batch/run \
  -H "Content-Type: application/json" \
  -d '{
    "wallet_addresses": [
      "ARZfRQgaoVjbHqg7p6M4uFPcJwEqwgUPT5hvfMkV8JNy",
      "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
      "APv9a1L4kLzeZERNAthAHTmjprJHCsio7NteRsp1D77Q",
      "87Hnwjwp28WPBLrc1JcuxWxQJenknT2zdN3atbNgtN7t"
    ],
    "chain": "solana",
    "max_transactions": 500
  }'
```


### Check Batch Job Status (Replace JOB_ID)
```bash
# Get job ID from batch submission response, then:
curl -s http://localhost:8080/api/pnl/batch/status/YOUR_JOB_ID_HERE | jq
```

### Get Batch Job Results (Replace JOB_ID)
```bash
curl -s http://localhost:8080/api/pnl/batch/results/YOUR_JOB_ID_HERE | jq
```

### Get Batch Results as Trader Summaries (Replace JOB_ID)
```bash
curl -s http://localhost:8080/api/pnl/batch/results/YOUR_JOB_ID_HERE/traders | jq
```

### Download Batch Results as CSV (Replace JOB_ID)
```bash
curl -s http://localhost:8080/api/pnl/batch/results/YOUR_JOB_ID_HERE/export.csv > batch_results.csv
```

---

## 5. Results Retrieval

### Get All P&L Results
```bash
curl -s http://localhost:8080/api/results | jq
```

### Get Top 10 Most Profitable Wallets
```bash
curl -s http://localhost:8080/api/results | jq '.data.results[:10] | sort_by(.total_pnl_usd | tonumber) | reverse'
```

### Get P&L Results with Pagination
```bash
# First 50 results
curl -s "http://localhost:8080/api/results?limit=50&offset=0" | jq

# Next 50 results
curl -s "http://localhost:8080/api/results?limit=50&offset=50" | jq
```

### Filter Results by Minimum P&L
```bash
curl -s http://localhost:8080/api/results | jq '.data.results[] | select(.total_pnl_usd | tonumber > 1000)'
```

### Get Results Summary
```bash
curl -s http://localhost:8080/api/results | jq '.data.summary'
```

### Export All Results to CSV
```bash
curl -s http://localhost:8080/api/results/export.csv > all_results.csv
```

---

## 6. DexScreener Integration Endpoints

The system integrates with DexScreener for trending token discovery and top traders analysis using web scraping:

**Note**: Wallet transaction data and balance fetching are handled by Zerion API.

### DexScreener Token Pages
```
Endpoint: https://dexscreener.com/{chain}/{pair_address}
Method: Web Scraping with Browser Automation
Purpose: Scrapes trending token pages for top traders
Rate Limiting: Built-in delays and anti-detection features
```

### DexScreener Top Traders Discovery
```
Process: Automated browser navigation to "Top Traders" section
Method: JavaScript execution and DOM parsing
Data Extracted:
- Wallet addresses from block explorer links
- Transaction volumes and patterns
- Trading activity indicators
Purpose: Discovers high-volume traders for P&L analysis
Rate Limiting: Intelligent request spacing and browser rotation
```

### DexScreener Trending Tokens Discovery
```
Process: Scrapes trending token lists from DexScreener
Method: Web scraping with headless Chrome
Data Extracted:
- Token addresses and pair information
- Trading volume and price movements
- Market activity indicators
Purpose: Identifies trending tokens for trader discovery
Rate Limiting: Configurable scraping intervals
```

## 6.1. Zerion Integration Endpoints

The system uses Zerion API for wallet transaction data and balance fetching:

### Zerion Wallet Transactions
```
Endpoint: https://api.zerion.io/v1/wallets/{wallet_address}/transactions/
Method: GET
Headers:
- authorization: Basic {base64_encoded_api_key}
- accept: application/json
Parameters:
- currency: usd
- page[size]: 100 (configurable)
Purpose: Gets complete transaction history for P&L analysis
```

### Zerion Wallet Positions (Balance Fetching)
```
Endpoint: https://api.zerion.io/v1/wallets/{wallet_address}/positions/
Method: GET
Headers:
- authorization: Basic {base64_encoded_api_key}
- accept: application/json
Parameters:
- filter[positions]: only_simple
- currency: usd
- filter[trash]: only_non_trash
- filter[chain_ids]: solana
- sort: value
Purpose: Gets current wallet token balances with USD values for unrealized P&L
```

---

## 7. Quick Test Scenarios

### Scenario 1: Quick Discovery Test (NEW Universal Control)
```bash
# 1. Start discovery with custom runtime configuration
curl -X POST http://localhost:8080/api/services/control \
  -H "Content-Type: application/json" \
  -d '{
    "action": "start",
    "service": "wallet_discovery",
    "config_override": {
      "max_transactions_to_fetch": 100,
      "min_capital_sol": 1.0,
      "min_trades": 3
    }
  }'

# 2. Monitor for 30 seconds
for i in {1..6}; do
  echo "Check $i:"
  curl -s http://localhost:8080/api/services/status | jq '.data.wallet_discovery.queue_size'
  sleep 5
done

# 3. Stop discovery
curl -X POST http://localhost:8080/api/services/control \
  -H "Content-Type: application/json" \
  -d '{
    "action": "stop",
    "service": "wallet_discovery"
  }'
```

### Scenario 1b: Legacy Discovery Test (Still Supported)
```bash
# 1. Configure for fast cycles
curl -X POST http://localhost:8080/api/services/config \
  -H "Content-Type: application/json" \
  -d '{"enable_wallet_discovery": true, "enable_pnl_analysis": false}'

# 2. Start discovery
curl -X POST http://localhost:8080/api/services/discovery/start

# 3. Monitor for 30 seconds
for i in {1..6}; do
  echo "Check $i:"
  curl -s http://localhost:8080/api/services/status | jq '.data.wallet_discovery.queue_size'
  sleep 5
done

# 4. Stop discovery
curl -X POST http://localhost:8080/api/services/discovery/stop
```

### Scenario 2: Full Pipeline Test
```bash
# 1. Configure both services
curl -X POST http://localhost:8080/api/services/config \
  -H "Content-Type: application/json" \
  -d '{"enable_wallet_discovery": true, "enable_pnl_analysis": true}'

# 2. Start discovery
curl -X POST http://localhost:8080/api/services/discovery/start

# 3. Wait for queue to populate
sleep 15

# 4. Start P&L analysis
curl -X POST http://localhost:8080/api/services/pnl/start

# 5. Monitor progress
for i in {1..10}; do
  echo "Status check $i:"
  curl -s http://localhost:8080/api/services/status | jq '{queue_size: .data.wallet_discovery.queue_size, pnl_state: .data.pnl_analysis.state}'
  sleep 10
done

# 6. Check results
curl -s http://localhost:8080/api/results | jq '.data.summary'
```

### Scenario 3: Batch Analysis Test
```bash
# 1. Submit batch job
BATCH_RESPONSE=$(curl -X POST http://localhost:8080/api/pnl/batch/run \
  -H "Content-Type: application/json" \
  -d '{
    "wallet_addresses": ["ARZfRQgaoVjbHqg7p6M4uFPcJwEqwgUPT5hvfMkV8JNy"],
    "chain": "solana",
    "max_transactions": 200
  }')

# 2. Extract job ID
JOB_ID=$(echo $BATCH_RESPONSE | jq -r '.data.job_id')
echo "Job ID: $JOB_ID"

# 3. Monitor job status
for i in {1..20}; do
  STATUS=$(curl -s "http://localhost:8080/api/pnl/batch/status/$JOB_ID" | jq -r '.data.status')
  echo "Check $i: $STATUS"
  if [ "$STATUS" = "Completed" ]; then
    break
  fi
  sleep 5
done

# 4. Get results
curl -s "http://localhost:8080/api/pnl/batch/results/$JOB_ID" | jq
```

---

## 8. Troubleshooting Commands

### Check if Server is Responsive
```bash
curl -w "Time: %{time_total}s\nStatus: %{http_code}\n" -s http://localhost:8080/health
```

### Check Service Configuration
```bash
curl -s http://localhost:8080/api/services/status | jq '.data'
```

### Reset System (Stop All Services)
```bash
curl -X POST http://localhost:8080/api/services/discovery/stop
curl -X POST http://localhost:8080/api/services/pnl/stop
echo "All services stopped"
```

### Test DexScreener Connectivity (Check System Logs)
```bash
# Start discovery briefly to test DexScreener scraping
curl -X POST http://localhost:8080/api/services/discovery/start
sleep 10
curl -X POST http://localhost:8080/api/services/discovery/stop
```

---

## 9. Performance Monitoring

### Monitor Queue Processing Rate
```bash
# Take initial measurement
INITIAL=$(curl -s http://localhost:8080/api/services/status | jq '.data.wallet_discovery.queue_size')
echo "Initial queue size: $INITIAL"

# Wait and measure again
sleep 60

FINAL=$(curl -s http://localhost:8080/api/services/status | jq '.data.wallet_discovery.queue_size')
echo "Final queue size: $FINAL"
echo "Processed: $((INITIAL - FINAL)) wallets in 60 seconds"
```

### Check Parallel Processing Stats
```bash
# Look for parallel batch logs in system output
curl -s http://localhost:8080/api/services/status | jq '{
  discovery_active: .data.wallet_discovery.state,
  analysis_active: .data.pnl_analysis.state,
  queue_size: .data.wallet_discovery.queue_size,
  timestamp: .timestamp
}'
```

---

## 10. API Configuration

### Supported Parameters

The batch P&L analysis API supports the following parameters:

| Parameter | Type | Required | Purpose | Example |
|-----------|------|----------|---------|---------|
| `wallet_addresses` | Array<String> | Yes | Wallet addresses to analyze | `["ARZfRQg..."]` |
| `chain` | String | Yes | Blockchain network | `"solana"` |
| `time_range` | String | No | Time period to analyze (fetches ALL txs in period) | `"7d"`, `"1h"`, `"30d"` |
| `max_transactions` | u32 | No | Limit transactions per wallet (ignored if time_range set) | `200` |

### Supported Time Ranges

When using the `time_range` parameter, the following formats are supported:

| Format | Description | Example | Result |
|--------|-------------|---------|---------|
| `Nh` | Hours ago | `"1h"`, `"24h"` | Last 1 hour, last 24 hours |
| `Nd` | Days ago | `"1d"`, `"7d"`, `"30d"` | Last 1 day, week, month |
| `Nw` | Weeks ago | `"1w"`, `"4w"` | Last 1 week, 4 weeks |
| `Nm` | Months ago | `"1m"`, `"6m"` | Last 1 month, 6 months |
| `Ny` | Years ago | `"1y"` | Last 365 days |

**Important Notes:**
- When `time_range` is provided, the system fetches ALL transactions within that period (no 1000 transaction limit)
- Without `time_range`, the system uses `max_transactions` limit (default: 1000)
- Shorter time ranges (1h, 1d) typically perform faster than transaction limits
- Maximum allowed time range is 2 years

### Batch Results Endpoints

The system provides multiple ways to retrieve batch job results:

| Endpoint | Purpose | Format |
|----------|---------|---------|
| `/api/pnl/batch/results/:job_id` | Raw P&L analysis results | JSON with detailed transaction data |
| `/api/pnl/batch/results/:job_id/traders` | Trader-focused summaries for copy trading analysis | JSON with trading metrics and performance |
| `/api/pnl/batch/results/:job_id/export.csv` | Downloadable results | CSV format for spreadsheet analysis |

**Note**: The `/traders` endpoint formats results specifically for copy trading analysis, providing trading performance metrics, risk assessment placeholders, and simplified P&L summaries.

### System Configuration

The system uses configuration from `config.toml` for:
- Service intervals and timeouts
- API keys for BirdEye and Zerion
- Redis and PostgreSQL connections
- Trader filtering (for discovery service only)

**Note**: Trader filtering configurations have been removed. The system now uses DexScreener web scraping for wallet discovery with built-in quality filtering.

---

## Notes

- **Rate Limiting**: DexScreener scraping includes built-in rate limiting and anti-detection features.
- **Parallel Processing**: System processes wallets in parallel (configurable in `config.toml`).
- **Data Sources**: System uses Zerion for wallet transactions & balances, DexScreener web scraping for trending tokens & top traders discovery.
- **Storage**: All P&L results are stored in PostgreSQL and can be retrieved via API.
- **Chains**: Currently supports Solana blockchain.
- **Logs**: Check server logs for detailed processing information.

## Common Issues

1. **Server not responding**: Check if `cargo run -p api_server` is running
2. **No wallets discovered**: Check DexScreener connectivity and browser automation
3. **Slow processing**: Reduce `max_trending_tokens` and `max_traders_per_token` for testing
4. **Rate limit errors**: Increase `cycle_interval_seconds` to slow down API calls