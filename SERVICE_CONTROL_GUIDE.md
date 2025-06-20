# P&L Tracker - Service Control & API Rate Management Guide

## 🎯 Service Control Overview

The P&L Tracker includes **automatic wallet discovery** and **P&L analysis** services that can be **controlled via API endpoints**. This ensures you have full control over API usage and can prevent exhausting your BirdEye API limits.

## 🚦 Service States & Control

### Current Deployment Status
**Default State:** Both services are **STOPPED** by default  
**Control Method:** API endpoints (not automatic startup)  
**Rate Limiting:** Configurable via API settings  

### ✅ Tested Service Control

| Service | Start Endpoint | Stop Endpoint | Status |
|---------|---------------|---------------|---------|
| **Wallet Discovery** | `POST /api/services/discovery/start` | `POST /api/services/discovery/stop` | ✅ Working |
| **P&L Analysis** | `POST /api/services/pnl/start` | `POST /api/services/pnl/stop` | ✅ Working |

## 📊 Service Configuration & Rate Limiting

### Configure Services Before Starting
```bash
# Configure both services with rate limiting
curl -X POST http://134.199.211.155:8080/api/services/config \
  -H "Content-Type: application/json" \
  -d '{
    "enable_wallet_discovery": true,
    "enable_pnl_analysis": true,
    "birdeye_config": {
      "max_trending_tokens": 5,
      "max_traders_per_token": 10,
      "cycle_interval_seconds": 300,
      "min_trader_volume_usd": 1000.0,
      "min_trader_trades": 5,
      "debug_mode": false
    }
  }'
```

**Key Rate Limiting Parameters:**
- `max_trending_tokens`: Limit how many trending tokens to analyze per cycle
- `max_traders_per_token`: Limit traders discovered per token
- `cycle_interval_seconds`: Time between discovery cycles (300s = 5 minutes)
- `min_trader_volume_usd`: Filter traders by minimum volume
- `min_trader_trades`: Filter traders by minimum trade count

## 🚀 Starting Services

### Start Wallet Discovery
```bash
# Start wallet discovery service
curl -X POST http://134.199.211.155:8080/api/services/discovery/start

# Expected response:
# {"data":{"message":"Wallet discovery service started successfully"},"timestamp":"..."}
```

### Start P&L Analysis  
```bash
# Start P&L analysis service
curl -X POST http://134.199.211.155:8080/api/services/pnl/start

# Expected response:
# {"data":{"message":"P&L analysis service started successfully"},"timestamp":"..."}
```

## 🛑 Stopping Services

### Stop Wallet Discovery
```bash
# Stop wallet discovery to prevent new API calls
curl -X POST http://134.199.211.155:8080/api/services/discovery/stop

# Expected response:
# {"data":{"message":"Wallet discovery service stopped successfully"},"timestamp":"..."}
```

### Stop P&L Analysis
```bash
# Stop P&L analysis service
curl -X POST http://134.199.211.155:8080/api/services/pnl/stop

# Expected response:
# {"data":{"message":"P&L analysis service stopped successfully"},"timestamp":"..."}
```

## 📈 Monitoring Service Activity

### Check Service Status
```bash
# Get current status of both services
curl -s http://134.199.211.155:8080/api/services/status

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

## 🔧 API Rate Management Strategies

### Conservative Configuration (Low API Usage)
```json
{
  "enable_wallet_discovery": true,
  "enable_pnl_analysis": false,
  "birdeye_config": {
    "max_trending_tokens": 2,
    "max_traders_per_token": 5,
    "cycle_interval_seconds": 600,
    "min_trader_volume_usd": 5000.0,
    "min_trader_trades": 10,
    "debug_mode": false
  }
}
```
**API Usage:** ~10 calls every 10 minutes

### Aggressive Configuration (High API Usage)
```json
{
  "enable_wallet_discovery": true,
  "enable_pnl_analysis": true,
  "birdeye_config": {
    "max_trending_tokens": 10,
    "max_traders_per_token": 20,
    "cycle_interval_seconds": 60,
    "min_trader_volume_usd": 100.0,
    "min_trader_trades": 2,
    "debug_mode": true
  }
}
```
**API Usage:** ~200+ calls per minute (⚠️ High rate)

### Testing Configuration (Minimal API Usage)
```json
{
  "enable_wallet_discovery": true,
  "enable_pnl_analysis": false,
  "birdeye_config": {
    "max_trending_tokens": 1,
    "max_traders_per_token": 2,
    "cycle_interval_seconds": 1800,
    "min_trader_volume_usd": 10000.0,
    "min_trader_trades": 20,
    "debug_mode": true
  }
}
```
**API Usage:** ~2-3 calls every 30 minutes

## 📊 Service Workflow

### Automatic Discovery Workflow
1. **Trending Tokens:** Service fetches trending tokens from BirdEye
2. **Top Traders:** For each token, discovers top traders
3. **Queue Wallets:** Discovered wallets are queued for P&L analysis
4. **Rate Limiting:** Respects configured cycle intervals and limits
5. **Filtering:** Only high-quality traders are queued based on filters

### P&L Analysis Workflow  
1. **Queue Monitoring:** Monitors Redis queue for discovered wallets
2. **Transaction Fetching:** Fetches wallet transactions via BirdEye
3. **P&L Calculation:** Calculates P&L using embedded prices
4. **Result Storage:** Stores results in Redis for API retrieval
5. **Continuous Processing:** Processes queue continuously when running

## 🚨 API Rate Limit Protection

### Built-in Protections
- ✅ **Configurable Intervals:** Control frequency of discovery cycles
- ✅ **Trader Limits:** Limit number of traders per token
- ✅ **Token Limits:** Limit number of trending tokens analyzed
- ✅ **Volume Filters:** Only analyze high-volume traders
- ✅ **Stop Controls:** Immediate stop capability via API

### BirdEye API Rate Limits
- **Free Tier:** ~1,000 requests per day
- **Pro Tier:** ~10,000 requests per day
- **Enterprise:** Custom limits

### Recommended Settings by Tier

#### Free Tier (1,000 requests/day)
```json
{
  "max_trending_tokens": 1,
  "max_traders_per_token": 3,
  "cycle_interval_seconds": 3600,
  "min_trader_volume_usd": 10000.0
}
```
**Usage:** ~72 requests/day

#### Pro Tier (10,000 requests/day)  
```json
{
  "max_trending_tokens": 5,
  "max_traders_per_token": 10,
  "cycle_interval_seconds": 300,
  "min_trader_volume_usd": 1000.0
}
```
**Usage:** ~1,440 requests/day

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
curl -X POST http://134.199.211.155:8080/api/services/discovery/stop
curl -X POST http://134.199.211.155:8080/api/services/pnl/stop
```

### Conservative Start (Low API Usage)
```bash
# Configure for minimal API usage
curl -X POST http://134.199.211.155:8080/api/services/config \
  -H "Content-Type: application/json" \
  -d '{
    "enable_wallet_discovery": true,
    "enable_pnl_analysis": false,
    "birdeye_config": {
      "max_trending_tokens": 2,
      "max_traders_per_token": 5,
      "cycle_interval_seconds": 1800,
      "min_trader_volume_usd": 5000.0,
      "min_trader_trades": 10,
      "debug_mode": false
    }
  }'

# Start discovery only
curl -X POST http://134.199.211.155:8080/api/services/discovery/start
```

### Status Check
```bash
# Quick status check
curl -s http://134.199.211.155:8080/api/services/status | jq '.data'
```

## 🎛️ Manual Operation Mode

For maximum control over API usage, you can operate in **manual mode**:

1. **Keep Services Stopped:** Don't start automatic discovery
2. **Manual Batch Analysis:** Use batch P&L endpoints for specific wallets
3. **Controlled Discovery:** Start discovery for short periods, then stop
4. **Monitor Usage:** Check logs and status regularly

### Manual Batch Analysis Example
```bash
# Analyze specific wallets without automatic discovery
curl -X POST http://134.199.211.155:8080/api/pnl/batch/run \
  -H "Content-Type: application/json" \
  -d '{
    "wallet_addresses": ["wallet1", "wallet2"],
    "filters": {
      "min_capital_sol": "1",
      "min_trades": 5,
      "max_signatures": 1000
    }
  }'
```

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
- Consider upgrading BirdEye plan if needed

## 🎯 Summary

✅ **Services DO NOT run automatically** - they must be started via API  
✅ **Full control via endpoints** - start/stop anytime  
✅ **Configurable rate limiting** - prevent API exhaustion  
✅ **Multiple operation modes** - automatic, manual, or hybrid  
✅ **Real-time monitoring** - track usage and activity  
✅ **Emergency stops** - immediate service shutdown capability  

**Your deployment is safe from runaway API usage!** The services only run when explicitly started and can be stopped immediately via API calls.