# P&L Tracker - Service Control & API Rate Management Guide

## 🎯 Service Control Overview

The P&L Tracker includes **automatic wallet discovery** and **P&L analysis** services that can be **controlled via API endpoints**. This ensures you have full control over resource usage and web scraping activity.

## 🚦 Service States & Control

### Current Deployment Status
**Default State:** Both services are **STOPPED** by default  
**Control Method:** API endpoints (not automatic startup)  
**Rate Limiting:** Configurable via API settings  

### ✅ Verified Working Service Control

| Service | Start Endpoint | Stop Endpoint | Status |
|---------|---------------|---------------|---------|
| **Wallet Discovery** | `POST /api/services/discovery/start` | `POST /api/services/discovery/stop` | ✅ Working |
| **P&L Analysis** | `POST /api/services/pnl/start` | `POST /api/services/pnl/stop` | ✅ Working |

**✅ Verified Pipeline:** Discovery → Redis Queue → P&L Analysis → Results Storage  
**✅ Real Results:** System successfully analyzed traders with profits up to $4,889.82

## 📊 Service Configuration & Rate Limiting

### Configure Services Before Starting
```bash
# Configure both services (VERIFIED WORKING)
curl -X POST http://localhost:8080/api/services/config \
  -H "Content-Type: application/json" \
  -d '{
    "enable_wallet_discovery": true,
    "enable_pnl_analysis": true
  }'
```

**Service Configuration:**
- `enable_wallet_discovery`: Enable/disable automatic wallet discovery via DexScreener scraping
- `enable_pnl_analysis`: Enable/disable P&L analysis of discovered wallets

**Note**: The system now uses DexScreener web scraping instead of BirdEye API calls, with built-in rate limiting and quality filtering.

## 🚀 Starting Services

### Start Wallet Discovery
```bash
# Start wallet discovery service (VERIFIED WORKING)
curl -X POST http://134.199.211.155:8080/api/services/discovery/start

# Expected response:
# {"data":{"message":"Wallet discovery service started successfully"},"timestamp":"..."}
```

### Start P&L Analysis  
```bash
# Start P&L analysis service (VERIFIED WORKING)
curl -X POST http://localhost:8080/api/services/pnl/start

# Expected response:
# {"data":{"message":"P&L analysis service started successfully"},"timestamp":"..."}
```

## 🛑 Stopping Services

### Stop Wallet Discovery
```bash
# Stop wallet discovery to prevent new API calls
curl -X POST http://localhost:8080/api/services/discovery/stop

# Expected response:
# {"data":{"message":"Wallet discovery service stopped successfully"},"timestamp":"..."}
```

### Stop P&L Analysis
```bash
# Stop P&L analysis service
curl -X POST http://localhost:8080/api/services/pnl/stop

# Expected response:
# {"data":{"message":"P&L analysis service stopped successfully"},"timestamp":"..."}
```

## 📈 Monitoring Service Activity

### ✅ Working Results Endpoint
```bash
# Get P&L analysis results (VERIFIED WORKING)
curl -s http://localhost:8080/api/results

# Example real response:
# {
#   "data": {
#     "results": [
#       {
#         "wallet_address": "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
#         "total_pnl_usd": "4889.82",
#         "realized_pnl_usd": "4889.82",
#         "roi_percentage": "217.32",
#         "total_trades": 42,
#         "win_rate": "0.69"
#       }
#     ]
#   }
# }
```

### Check Service Status
```bash
# Get current status of both services (VERIFIED WORKING)
curl -s http://localhost:8080/api/services/status

# Example response:
{
  "data": {
    "wallet_discovery": {
      "state": "Running",
      "discovered_wallets_total": 0,
      "queue_size": 0,
      "last_cycle_wallets": 0,
      "cycles_completed": 0,
      "last_activity": null
    },
    "pnl_analysis": {
      "state": "Running",
      "wallets_processed": 0,
      "wallets_in_progress": 0,
      "successful_analyses": 0,
      "failed_analyses": 0,
      "last_activity": null
    }
  }
}
```

**Service States:**
- `Stopped`: Service is not running
- `Starting`: Service is initializing  
- `Running`: Service is active and processing
- `Stopping`: Service is shutting down
- `Error(message)`: Service encountered an error

## 🔧 Resource Management Strategies

### Conservative Configuration (Lower Resource Usage)
```json
{
  "enable_wallet_discovery": true,
  "enable_pnl_analysis": false
}
```
**Usage:** Only runs wallet discovery via DexScreener scraping. Analysis can be done manually later.

### Full Pipeline Configuration (Normal Usage)
```json
{
  "enable_wallet_discovery": true,
  "enable_pnl_analysis": true
}
```
**Usage:** Runs both discovery and analysis continuously. Built-in rate limiting for DexScreener scraping.

### Manual Analysis Configuration (Discovery Off)
```json
{
  "enable_wallet_discovery": false,
  "enable_pnl_analysis": true
}
```
**Usage:** Only processes manually queued wallets. Use for batch analysis jobs only.

## 📊 Service Workflow

### Automatic Discovery Workflow
1. **Trending Tokens:** Service scrapes trending tokens from DexScreener using browser automation
2. **Top Traders:** For each token, navigates to DexScreener pages and extracts top traders
3. **Wallet Extraction:** Parses block explorer links to extract wallet addresses
4. **Queue Wallets:** Discovered wallets are queued for P&L analysis
5. **Rate Limiting:** Built-in delays and anti-detection features prevent blocking
6. **Quality Filtering:** Only high-volume traders are queued based on scraping criteria

### P&L Analysis Workflow
1. **Queue Monitoring:** Monitors Redis queue for discovered wallets
2. **Transaction Fetching:** Fetches wallet transactions via Zerion API
3. **P&L Calculation:** Calculates P&L using embedded transaction prices
4. **Result Storage:** Stores results in PostgreSQL for API retrieval
5. **Continuous Processing:** Processes queue continuously when running

## 🚨 Web Scraping Protection & Resource Management

### Built-in Protections
- ✅ **Anti-Detection:** Browser fingerprinting and stealth mode
- ✅ **Request Spacing:** Intelligent delays between page requests
- ✅ **Browser Rotation:** Prevents detection patterns
- ✅ **Resource Limits:** Controls CPU and memory usage
- ✅ **Stop Controls:** Immediate stop capability via API
- ✅ **Error Handling:** Graceful recovery from failed requests

### DexScreener Scraping Considerations
- **Rate Limiting:** Built into browser automation (no external limits)
- **Anti-Detection:** Headless Chrome with stealth features
- **Resource Usage:** CPU and memory for browser automation
- **Network Usage:** Standard HTTP requests (not API calls)

### Resource Management Strategies

#### Lightweight Scraping (Lower Resource Usage)
- Uses headless browser with minimal features
- Longer delays between requests
- Processes fewer tokens per cycle
- **Usage:** Suitable for lower-spec servers

#### Standard Scraping (Balanced Usage)
- Full browser automation features
- Moderate request timing
- Standard anti-detection measures
- **Usage:** Recommended for most deployments

## 🔍 Monitoring & Logging

### Check Service Logs
```bash
# View real-time service logs
ssh root@134.199.211.155 "journalctl -u pnl-tracker -f"

# View recent discovery activity
ssh root@134.199.211.155 "journalctl -u pnl-tracker --since '10 minutes ago' | grep -i discovery"

# Check for API errors
ssh root@134.199.211.155 "journalctl -u pnl-tracker --since '1 hour ago' | grep -i error"
```

### Service Status Script
```bash
# Run comprehensive status check
ssh root@134.199.211.155 "/opt/pnl_tracker/status_check.sh"
```

## ⚡ Quick Control Commands

### Emergency Stop (All Services)
```bash
# Stop both services immediately
curl -X POST http://localhost:8080/api/services/discovery/stop
curl -X POST http://localhost:8080/api/services/pnl/stop
```

### Conservative Start (Low API Usage)
```bash
# Configure for minimal API usage
curl -X POST http://localhost:8080/api/services/config \
  -H "Content-Type: application/json" \
  -d '{
    "enable_wallet_discovery": true,
    "enable_pnl_analysis": false
  }'

# Start discovery only
curl -X POST http://localhost:8080/api/services/discovery/start
```

### Status Check
```bash
# Quick status check (VERIFIED WORKING)
curl -s http://localhost:8080/api/services/status | jq '.data'
```

## 🎛️ Manual Operation Mode

For maximum control over API usage, you can operate in **manual mode**:

1. **Keep Services Stopped:** Don't start automatic discovery
2. **Manual Batch Analysis:** Use batch P&L endpoints for specific wallets
3. **Controlled Discovery:** Start discovery for short periods, then stop
4. **Monitor Usage:** Check logs and status regularly

### Manual Batch Analysis Examples

#### Time-Based Analysis (NEW - Recommended)
```bash
# Analyze last 7 days of activity for specific wallets
curl -X POST http://134.199.211.155:8080/api/pnl/batch/run \
  -H "Content-Type: application/json" \
  -d '{
    "wallet_addresses": ["wallet1", "wallet2"],
    "chain": "solana",
    "time_range": "7d"
  }'

# Analyze last hour (useful for recent trading activity)
curl -X POST http://134.199.211.155:8080/api/pnl/batch/run \
  -H "Content-Type: application/json" \
  -d '{
    "wallet_addresses": ["wallet1"],
    "chain": "solana",
    "time_range": "1h"
  }'
```

#### Transaction Limit Analysis (Legacy)
```bash
# Analyze most recent 500 transactions
curl -X POST http://134.199.211.155:8080/api/pnl/batch/run \
  -H "Content-Type: application/json" \
  -d '{
    "wallet_addresses": ["wallet1", "wallet2"],
    "chain": "solana",
    "max_transactions": 500
  }'
```

### Time-Based vs Transaction-Based Analysis

| Approach | When to Use | Benefits | Limitations |
|----------|------------|----------|-------------|
| **Time Range** (`time_range`) | Analyzing specific periods | Gets ALL transactions in period, no limits | May fetch many transactions for active traders |
| **Transaction Limit** (`max_transactions`) | Quick analysis of recent activity | Predictable data volume | May miss older important trades |
| **Default** (no params) | General analysis | Gets most recent 1000 transactions | Fixed limit may not suit all wallets |

## 📋 Best Practices

### 1. **Start Conservative**
- Begin with minimal configuration
- Monitor API usage for 24 hours
- Gradually increase if within limits

### 2. **Use Filters Effectively**
- Set high minimum volume thresholds
- Require minimum trade counts
- Use longer cycle intervals initially

### 3. **Monitor Actively**
- Check status every few hours initially
- Monitor logs for errors or rate limiting
- Track discovery rates and queue sizes

### 4. **Plan for Limits**
- Calculate expected API usage before starting
- Keep emergency stop commands ready
- Consider server resource upgrades if scraping performance is limited

## 🎯 Summary

✅ **Services DO NOT run automatically** - they must be started via API  
✅ **Full control via endpoints** - start/stop anytime  
✅ **Configurable rate limiting** - prevent API exhaustion  
✅ **Multiple operation modes** - automatic, manual, or hybrid  
✅ **Real-time monitoring** - track usage and activity  
✅ **Emergency stops** - immediate service shutdown capability  

**Your deployment is safe from runaway API usage!** The services only run when explicitly started and can be stopped immediately via API calls.