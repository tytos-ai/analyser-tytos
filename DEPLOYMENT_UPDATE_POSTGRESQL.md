# PostgreSQL Deployment Update Instructions

This document provides the updated deployment commands for the P&L Tracker system with PostgreSQL integration. These commands replace the corresponding sections in `DEPLOYMENT_GUIDE_COMPLETE.md`.

## 🔄 Updated Commands for PostgreSQL Integration

### Step 5: Build the Application (Updated)

#### 5.1 Build Release Version
```bash
# Build the project in release mode (optimized) - No changes needed
ssh root@134.199.211.155 "source ~/.cargo/env && cd /opt/pnl_tracker && cargo build --release"
```

#### 5.2 Build API Server Specifically
```bash
# Build the API server binary - No changes needed
ssh root@134.199.211.155 "source ~/.cargo/env && cd /opt/pnl_tracker && cargo build --release -p api_server"
```

### Step 6: Production Configuration (Updated)

#### 6.1 Create Production Config File with PostgreSQL
```bash
# Create production configuration with PostgreSQL support
ssh root@134.199.211.155 "cat > /opt/pnl_tracker/config.prod.toml << 'EOF'
[system]
debug_mode = false
redis_mode = true
process_loop_ms = 30000
output_csv_file = \"pnl_results.csv\"
pnl_parallel_batch_size = 20

# Data source configuration for transaction fetching  
# Options: \"BirdEye\"
data_source = \"BirdEye\"

[redis]
# IMPORTANT: For production server, use password-protected Redis URL
url = \"redis://:dexscreener_732d9e7d7d74573e0040d736e94e3a29@localhost:6379\"
connection_timeout_seconds = 10
default_lock_ttl_seconds = 600

# DexScreener configuration for boosted token discovery
[dexscreener]
api_base_url = \"https://api.dexscreener.com\"
request_timeout_seconds = 30
rate_limit_delay_ms = 1000  # 60 requests per minute = 1 request per second
max_retries = 3
enabled = true
min_boost_amount = 100.0    # Minimum boost amount to consider
max_boosted_tokens = 50     # Increased from 20 to process more boosted tokens

[birdeye]
api_key = \"5ff313b239ac42e297b830b10ea1871d\"
api_base_url = \"https://public-api.birdeye.so\"
request_timeout_seconds = 30
price_cache_ttl_seconds = 60
rate_limit_per_second = 100
max_traders_per_token = 100  # Increased from 20 for maximum coverage
max_transactions_per_trader = 100  # API limit - DO NOT CHANGE
default_max_transactions = 900  # Default maximum transactions to fetch/analyze per wallet
max_token_rank = 1000
new_listing_enabled = true
new_listing_min_liquidity = 1000.0
new_listing_max_age_hours = 24
new_listing_max_tokens = 100  # Increased from 25 to process more new listings
max_trending_tokens = 10000  # Maximum trending tokens to process (0 = unlimited)


# Price fetching configuration (Jupiter + BirdEye)
[price_fetching]
primary_source = \"jupiter\"
fallback_enabled = true
fallback_source = \"birdeye\"
jupiter_api_url = \"https://lite-api.jup.ag\"
birdeye_api_url = \"https://public-api.birdeye.so\"
request_timeout_seconds = 30
price_cache_ttl_seconds = 300
enable_caching = true

[pnl]
timeframe_mode = \"none\"
timeframe_general = \"7d\"
wallet_min_capital = 0.0
aggregator_min_hold_minutes = 0.0
amount_trades = 0
win_rate = 0.0
aggregator_batch_size = 20
max_signatures = 1000  # NOTE: This value is now ignored - automatically set to match birdeye.default_max_transactions

# Advanced trader filtering for copy trading (production settings)
[trader_filter]
min_realized_pnl_usd = 100.0       # Production threshold
min_total_trades = 5               # Production threshold
min_winning_trades = 2             # Production threshold  
min_win_rate = 40.0                # Production threshold
min_roi_percentage = 10.0           # Production threshold
min_capital_deployed_sol = 1.0      # Production threshold
max_avg_hold_time_minutes = 1440   # 24 hours max
min_avg_hold_time_minutes = 5      # 5 minutes min
exclude_holders_only = true        # Skip wallets with only buy transactions
exclude_zero_pnl = true           # Skip wallets with no realized gains
min_transaction_frequency = 0.1   # Production frequency requirement

[api]
host = \"0.0.0.0\"
port = 8080
enable_cors = true
request_timeout_seconds = 30
EOF"
```

**Key Production Settings:**
- `debug_mode = false`: Optimized logging for production
- `host = "0.0.0.0"`: Accept connections from any IP
- `port = 8080`: Standard HTTP alternative port
- `default_max_transactions = 900`: Updated transaction limit for thorough analysis
- Stricter trader filters for better quality results
- **PostgreSQL**: Uses environment variable `DATABASE_URL` (no config file setting needed)

### Step 7: Systemd Service Configuration (Updated)

#### 7.1 Create Service File with PostgreSQL Support
```bash
# Create systemd service with PostgreSQL environment variables
ssh root@134.199.211.155 "cat > /etc/systemd/system/pnl-tracker.service << 'EOF'
[Unit]
Description=P&L Tracker API Server with PostgreSQL
After=network.target redis-server.service postgresql.service
Requires=redis-server.service

[Service]
Type=simple
User=root
WorkingDirectory=/opt/pnl_tracker
Environment=RUST_LOG=info
Environment=CONFIG_FILE=config.prod.toml
Environment=DATABASE_URL=postgresql://root:Great222*@localhost/analyser
Environment=REDIS_URL=redis://localhost:6379
ExecStart=/opt/pnl_tracker/target/release/api_server
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal

# Resource limits
LimitNOFILE=65536
LimitNPROC=32768

[Install]
WantedBy=multi-user.target
EOF"
```

**Service Configuration Updates:**
- `After=postgresql.service`: Start after PostgreSQL is ready
- `Environment=DATABASE_URL`: PostgreSQL connection string
- `Environment=REDIS_URL`: Redis connection string (explicit)
- **Note**: PostgreSQL is not in `Requires=` to allow service to start even if PostgreSQL restarts

#### 7.2 Enable and Configure Service (Unchanged)
```bash
# Reload systemd configuration and enable service
ssh root@134.199.211.155 "systemctl daemon-reload && systemctl enable pnl-tracker"
```

## 🔍 Updated Verification Steps

### PostgreSQL Integration Tests

#### Test 1: Verify PostgreSQL Connection
```bash
# Test that API can connect to PostgreSQL
ssh root@134.199.211.155 "systemctl start pnl-tracker && sleep 5 && curl -s http://localhost:8080/health | jq"
```

**Expected Response:**
```json
{
  "data": {
    "status": "healthy",
    "version": "0.1.0",
    "uptime_seconds": 5
  },
  "timestamp": "2025-07-19T23:40:00.000Z"
}
```

#### Test 2: Submit Batch Job to Test PostgreSQL Storage
```bash
# Submit a test batch job to verify PostgreSQL storage
curl -s -X POST http://134.199.211.155:8080/api/pnl/batch/run \
-H "Content-Type: application/json" \
-d '{
  "wallet_addresses": ["5ngDQ3vMT7GHeFvXP5GLX79t6fqG2t15kyQTtst9zFdw"]
}'
```

#### Test 3: Verify Data Storage in PostgreSQL
```bash
# Check that data is stored in PostgreSQL
ssh root@134.199.211.155 "PGPASSWORD='Great222*' psql -h localhost -U root -d analyser -c 'SELECT COUNT(*) FROM batch_jobs;'"
```

**Expected Output:**
```
 count 
-------
     1
(1 row)
```

#### Test 4: Check Individual P&L Results Storage
```bash
# Wait for processing and check individual results
sleep 10
ssh root@134.199.211.155 "PGPASSWORD='Great222*' psql -h localhost -U root -d analyser -c 'SELECT COUNT(*) FROM pnl_results;'"
```

## 📊 Key Changes Summary

### What's New in PostgreSQL Integration:
1. **Hybrid Architecture**: Redis for operational data, PostgreSQL for persistent storage
2. **Environment Variables**: `DATABASE_URL` for PostgreSQL connection
3. **Schema**: 4-table PostgreSQL schema (pnl_results, batch_jobs, batch_results, pnl_summary_stats)
4. **Performance**: 900 transaction limit for thorough analysis
5. **Data Integrity**: Proper timestamp handling and NOT NULL constraints

### What's Removed:
1. **PostgreSQL Installation**: Already installed on server
2. **Config File Database Section**: Uses environment variable instead
3. **Migration Code**: Removed from codebase

### What Stays the Same:
1. **Build Process**: No changes to Rust compilation
2. **API Endpoints**: All existing endpoints work unchanged
3. **Redis Usage**: Still used for caching and queue management
4. **BirdEye Integration**: Unchanged functionality

## 🚨 Important Notes

1. **PostgreSQL Schema**: Already synchronized between local and server
2. **Timestamp Handling**: All datatype issues resolved
3. **Transaction Limit**: Updated to 900 for production throughput
4. **Memory Usage**: Expected ~5-10MB (increased slightly due to PostgreSQL)
5. **Dependencies**: No additional package installation required

## ✅ Deployment Success Indicators

After deployment, verify these are working:
- [ ] Health endpoint responds correctly
- [ ] Batch jobs complete and store in PostgreSQL
- [ ] Individual P&L results stored in `pnl_results` table
- [ ] Service restarts automatically on failure
- [ ] Memory usage stable (~5-10MB)
- [ ] Transaction processing with 900 limit works

---

*These instructions update the original deployment guide with PostgreSQL integration while leveraging the existing server infrastructure.*