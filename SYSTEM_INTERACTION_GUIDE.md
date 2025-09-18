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
    "enable_pnl_analysis": true,
    "birdeye_config": {
      "max_trending_tokens": 20,
      "max_traders_per_token": 10,
      "cycle_interval_seconds": 10,
      "min_trader_volume_usd": 1000.0,
      "min_trader_trades": 5,
      "debug_mode": false
    }
  }'
```

### Configure for Fast Testing (5-second cycles)
```bash
curl -X POST http://localhost:8080/api/services/config \
  -H "Content-Type: application/json" \
  -d '{
    "enable_wallet_discovery": true,
    "enable_pnl_analysis": true,
    "birdeye_config": {
      "max_trending_tokens": 10,
      "max_traders_per_token": 10,
      "cycle_interval_seconds": 5,
      "min_trader_volume_usd": 1000.0,
      "min_trader_trades": 5,
      "debug_mode": false
    }
  }'
```

### Configure for Production (60-second cycles)
```bash
curl -X POST http://localhost:8080/api/services/config \
  -H "Content-Type: application/json" \
  -d '{
    "enable_wallet_discovery": true,
    "enable_pnl_analysis": true,
    "birdeye_config": {
      "max_trending_tokens": 50,
      "max_traders_per_token": 20,
      "cycle_interval_seconds": 60,
      "min_trader_volume_usd": 5000.0,
      "min_trader_trades": 10,
      "debug_mode": false
    }
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

### Submit Batch Job (Basic)
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

## 6. BirdEye Integration Endpoints

The system integrates with BirdEye API using these endpoints:

### BirdEye Trending Tokens
```
Endpoint: https://public-api.birdeye.so/defi/trending_tokens/solana
Method: GET
Purpose: Discovers trending tokens in real-time
Rate Limit: 100 requests/minute
```

### BirdEye Top Traders
```
Endpoint: https://public-api.birdeye.so/trader/txs/solana
Method: GET
Parameters:
- address: token_address
- limit: 100 (max traders per token)
- sort_by: volume_usd_desc
Purpose: Gets top traders for each trending token
Rate Limit: 300 requests/minute
```

### BirdEye Trader Transactions
```
Endpoint: https://public-api.birdeye.so/trader/txs/solana/{wallet_address}
Method: GET
Parameters:
- limit: 100 (max transactions)
- tx_type: swap
- sort_by: block_time_desc
Purpose: Gets complete trading history for P&L analysis
Rate Limit: 300 requests/minute
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
  -d '{"enable_wallet_discovery": true, "enable_pnl_analysis": false, "birdeye_config": {"max_trending_tokens": 5, "max_traders_per_token": 5, "cycle_interval_seconds": 5, "min_trader_volume_usd": 1000.0, "min_trader_trades": 3, "debug_mode": false}}'

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
  -d '{"enable_wallet_discovery": true, "enable_pnl_analysis": true, "birdeye_config": {"max_trending_tokens": 10, "max_traders_per_token": 10, "cycle_interval_seconds": 10, "min_trader_volume_usd": 1000.0, "min_trader_trades": 5, "debug_mode": false}}'

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

### Test BirdEye Connectivity (Check System Logs)
```bash
# Start discovery briefly to test BirdEye API
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
| `max_transactions` | u32 | No | Limit transactions per wallet | `200` |

### System Configuration

The system uses configuration from `config.toml` for:
- Service intervals and timeouts
- API keys for BirdEye and Zerion
- Redis and PostgreSQL connections
- Trader filtering (for discovery service only)

Discovery service trader filtering (not used in batch analysis):
- `min_capital_deployed_sol`: Minimum capital in SOL
- `min_total_trades`: Minimum number of trades
- `min_win_rate`: Minimum win rate percentage

---

## Notes

- **Rate Limiting**: BirdEye has rate limits. The system automatically handles retries.
- **Parallel Processing**: System processes wallets in parallel (configurable in `config.toml`).
- **Data Sources**: System uses hybrid Zerion+BirdEye architecture for transaction data.
- **Storage**: All P&L results are stored in PostgreSQL and can be retrieved via API.
- **Chains**: Currently supports Solana blockchain.
- **Logs**: Check server logs for detailed processing information.

## Common Issues

1. **Server not responding**: Check if `cargo run -p api_server` is running
2. **No wallets discovered**: Check BirdEye API connectivity and rate limits
3. **Slow processing**: Reduce `max_trending_tokens` and `max_traders_per_token` for testing
4. **Rate limit errors**: Increase `cycle_interval_seconds` to slow down API calls