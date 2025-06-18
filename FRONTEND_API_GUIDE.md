
# P&L Tracker API - Frontend Integration Guide

## Quick Start

**Base URL:** `http://localhost:8080`  
**All endpoints return JSON**  
**CORS is enabled for frontend integration**

## System Overview

The P&L Tracker API provides two main modes:

1. **Batch Mode** - Analyze specific wallet addresses on-demand
2. **Continuous Mode** - 24/7 monitoring of trending tokens from DexScreener

## Authentication

Currently no authentication is required. All endpoints are publicly accessible.

---

## System Management Endpoints

### Health Check
```http
GET /health
```

**Response:**
```json
{
  "data": {
    "status": "healthy",
    "version": "0.1.0",
    "uptime_seconds": 1234
  },
  "timestamp": "2025-06-18T05:52:26.226749591Z"
}
```

### System Status
```http
GET /api/status
```

**Response:**
```json
{
  "data": {
    "api_server": {
      "status": "running",
      "uptime_seconds": 3600,
      "requests_processed": 150
    },
    "continuous_mode": {
      "enabled": true,
      "status": "running",
      "wallets_in_queue": 25,
      "processed_today": 145
    },
    "dex_monitoring": {
      "status": "running",
      "connected": true,
      "tokens_discovered": 12,
      "last_discovery": "2025-06-18T05:45:00Z"
    },
    "redis": {
      "status": "connected",
      "latency_ms": 2.5
    }
  },
  "timestamp": "2025-06-18T05:54:35.275334662Z"
}
```

### System Logs
```http
GET /api/logs?limit=100&level=info
```

**Query Parameters:**
- `limit` (optional): Number of log entries (default: 50, max: 1000)
- `level` (optional): Log level filter (error, warn, info, debug)

---

## Configuration Management

### Get Current Configuration
```http
GET /api/config
```

**Response:** Full system configuration including all modules (system, solana, redis, dexscreener, jupiter, pnl, trader_filter, api)

### Update Configuration
```http
POST /api/config
Content-Type: application/json

{
  "system": {
    "redis_mode": true,
    "debug_mode": false
  },
  "pnl": {
    "timeframe_mode": "general",
    "timeframe_general": "7d",
    "wallet_min_capital": 1.0
  }
}
```

**Response:**
```json
{
  "data": {
    "message": "Configuration updated successfully",
    "restart_required": false
  },
  "timestamp": "2025-06-18T05:55:00Z"
}
```

---

## Batch P&L Analysis

### Submit Batch Job
```http
POST /api/pnl/batch/run
Content-Type: application/json

{
  "wallets": [
    "7BgBvyjrZX1YKz4oh9mjb8ZScatkkwb8DzFx6BgGvPtP",
    "9WjY8K4h3c1D2FxNoE6ZcJsNrAqGfT5VvMz8yPpKdLm4"
  ],
  "config_overrides": {
    "timeframe_mode": "general",
    "timeframe_general": "30d",
    "wallet_min_capital": 0.5
  }
}
```

**Response:**
```json
{
  "data": {
    "job_id": "batch_job_1734567890",
    "status": "submitted",
    "wallets_count": 2,
    "estimated_duration_seconds": 120
  },
  "timestamp": "2025-06-18T05:55:30Z"
}
```

### Check Batch Job Status
```http
GET /api/pnl/batch/status/{job_id}
```

**Response:**
```json
{
  "data": {
    "job_id": "batch_job_1734567890",
    "status": "running",
    "progress": {
      "completed": 1,
      "total": 2,
      "percentage": 50.0
    },
    "started_at": "2025-06-18T05:55:30Z",
    "estimated_completion": "2025-06-18T05:57:30Z",
    "current_wallet": "9WjY8K4h3c1D2FxNoE6ZcJsNrAqGfT5VvMz8yPpKdLm4"
  },
  "timestamp": "2025-06-18T05:56:15Z"
}
```

**Status Values:**
- `submitted` - Job queued for processing
- `running` - Currently processing wallets
- `completed` - All wallets processed successfully
- `failed` - Job failed with errors
- `partial` - Some wallets processed, some failed

### Get Batch Job Results
```http
GET /api/pnl/batch/results/{job_id}
```

**Response:**
```json
{
  "data": {
    "job_id": "batch_job_1734567890",
    "status": "completed",
    "results": [
      {
        "wallet_address": "7BgBvyjrZX1YKz4oh9mjb8ZScatkkwb8DzFx6BgGvPtP",
        "status": "success",
        "pnl_summary": {
          "total_realized_pnl_usd": 1247.85,
          "total_unrealized_pnl_usd": 342.12,
          "total_trades": 23,
          "winning_trades": 15,
          "win_rate": 65.22,
          "roi_percentage": 12.45,
          "capital_deployed_sol": 45.2,
          "avg_hold_time_minutes": 1440
        },
        "holdings": [
          {
            "token_address": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
            "symbol": "USDC",
            "amount": 1590.12,
            "current_value_usd": 1590.12
          }
        ]
      }
    ],
    "summary": {
      "total_wallets": 2,
      "successful": 2,
      "failed": 0,
      "total_pnl_usd": 2847.65
    }
  },
  "timestamp": "2025-06-18T05:57:45Z"
}
```

### Export Batch Results as CSV
```http
GET /api/pnl/batch/results/{job_id}/export.csv
```

**Response:** CSV file download with headers:
```
wallet_address,total_realized_pnl_usd,total_unrealized_pnl_usd,total_trades,winning_trades,win_rate,roi_percentage,capital_deployed_sol,avg_hold_time_minutes
```

### Filter Copy Traders from Batch Results
```http
GET /api/pnl/batch/results/{job_id}/traders
```

**Response:**
```json
{
  "data": {
    "filtered_traders": [
      {
        "wallet_address": "7BgBvyjrZX1YKz4oh9mjb8ZScatkkwb8DzFx6BgGvPtP",
        "trader_score": 85.5,
        "meets_criteria": true,
        "criteria_met": [
          "min_realized_pnl_usd",
          "min_win_rate",
          "min_total_trades"
        ],
        "pnl_summary": { /* same as above */ }
      }
    ],
    "filter_stats": {
      "total_analyzed": 2,
      "traders_found": 1,
      "pass_rate": 50.0
    }
  },
  "timestamp": "2025-06-18T05:58:00Z"
}
```

---

## Continuous Mode (24/7 Monitoring)

### Get Discovered Wallets
```http
GET /api/pnl/continuous/discovered-wallets?limit=20&offset=0&min_pnl=100
```

**Query Parameters:**
- `limit` (optional): Number of results (default: 50, max: 500)
- `offset` (optional): Pagination offset (default: 0)
- `min_pnl` (optional): Minimum P&L filter in USD
- `min_win_rate` (optional): Minimum win rate percentage
- `discovered_since` (optional): ISO date string

**Response:**
```json
{
  "data": {
    "wallets": [
      {
        "wallet_address": "5K9W8h2m3c1F4x7R9Q2E5vT6uP8sL3dY4nA1gB7zX9V2",
        "discovered_at": "2025-06-18T05:30:00Z",
        "source_token": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
        "discovery_reason": "trending_token_trader",
        "analysis_status": "completed",
        "pnl_summary": {
          "total_realized_pnl_usd": 2340.50,
          "win_rate": 72.5,
          "total_trades": 18,
          "trader_score": 92.3
        },
        "last_analyzed": "2025-06-18T05:45:00Z"
      }
    ],
    "pagination": {
      "total": 156,
      "limit": 20,
      "offset": 0,
      "has_more": true
    }
  },
  "timestamp": "2025-06-18T05:58:30Z"
}
```

### Get Wallet Details
```http
GET /api/pnl/continuous/discovered-wallets/{wallet_address}/details
```

**Response:**
```json
{
  "data": {
    "wallet_address": "5K9W8h2m3c1F4x7R9Q2E5vT6uP8sL3dY4nA1gB7zX9V2",
    "discovery_info": {
      "discovered_at": "2025-06-18T05:30:00Z",
      "source_token": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
      "discovery_reason": "trending_token_trader"
    },
    "pnl_report": {
      "total_realized_pnl_usd": 2340.50,
      "total_unrealized_pnl_usd": 890.25,
      "total_trades": 18,
      "winning_trades": 13,
      "win_rate": 72.22,
      "roi_percentage": 28.5,
      "capital_deployed_sol": 125.8,
      "avg_hold_time_minutes": 2880
    },
    "trade_history": [
      {
        "token_address": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
        "symbol": "USDC",
        "action": "sell",
        "amount": 1000.0,
        "price_usd": 1.001,
        "pnl_usd": 156.75,
        "timestamp": "2025-06-18T04:20:00Z"
      }
    ],
    "current_holdings": [
      {
        "token_address": "So11111111111111111111111111111111111111112",
        "symbol": "SOL",
        "amount": 12.5,
        "current_value_usd": 2750.0
      }
    ]
  },
  "timestamp": "2025-06-18T05:59:00Z"
}
```

---

## DexScreener Monitoring Control

### Get Dex Monitoring Status
```http
GET /api/dex/status
```

**Response:**
```json
{
  "data": {
    "monitoring_status": "running",
    "connection_status": "connected",
    "tokens_monitored": 15,
    "wallets_discovered_today": 47,
    "last_discovery": "2025-06-18T05:55:00Z",
    "trending_criteria": {
      "min_volume_24h": 1270000,
      "min_txns_24h": 45000,
      "min_liquidity_usd": 10000,
      "min_price_change_24h": 50
    },
    "performance_stats": {
      "avg_response_time_ms": 250,
      "success_rate": 98.5,
      "errors_last_hour": 2
    }
  },
  "timestamp": "2025-06-18T05:59:30Z"
}
```

### Control Dex Service
```http
POST /api/dex/control
Content-Type: application/json

{
  "action": "start"  // or "stop", "restart"
}
```

**Response:**
```json
{
  "data": {
    "action": "start",
    "status": "success",
    "message": "DexScreener monitoring started successfully"
  },
  "timestamp": "2025-06-18T06:00:00Z"
}
```

---

## Configuration Options

### System Modes

**Batch Mode Only** (`redis_mode: false`):
- Only batch analysis endpoints are active
- No continuous monitoring
- Suitable for on-demand analysis

**Continuous Mode** (`redis_mode: true`):
- All endpoints active
- 24/7 monitoring enabled
- Redis required for wallet queuing

### P&L Timeframe Modes

1. **none** - Analyze all historical transactions
2. **general** - Use predefined periods ("1d", "7d", "30d", "1y")  
3. **specific** - Custom date range (ISO format)

### Trader Filter Criteria

Configure minimum thresholds for identifying quality traders:
- `min_realized_pnl_usd`: Minimum profit in USD
- `min_win_rate`: Minimum win rate percentage (0-100)
- `min_total_trades`: Minimum number of trades
- `min_roi_percentage`: Minimum ROI percentage

---

## Error Handling

All endpoints return errors in this format:
```json
{
  "error": "Detailed error message",
  "timestamp": "2025-06-18T06:00:30Z"
}
```

**Common HTTP Status Codes:**
- `200` - Success
- `400` - Bad Request (validation errors)
- `404` - Resource not found
- `500` - Internal server error
- `502` - External API error (Solana, DexScreener, Jupiter)

---

## Rate Limits

No rate limits currently implemented, but recommended frontend practices:
- Poll status endpoints max every 5 seconds
- Batch job submissions: max 1 per minute
- Configuration updates: max 1 per minute

---

## Example Frontend Usage

### Starting the System
```javascript
// Check if system is healthy
const health = await fetch('/health').then(r => r.json());

// Get current configuration
const config = await fetch('/api/config').then(r => r.json());

// Enable continuous mode if needed
if (!config.data.system.redis_mode) {
  await fetch('/api/config', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      system: { redis_mode: true }
    })
  });
}
```

### Running Batch Analysis
```javascript
// Submit wallets for analysis
const batchJob = await fetch('/api/pnl/batch/run', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    wallets: ['7BgBvyjrZX1YKz4oh9mjb8ZScatkkwb8DzFx6BgGvPtP'],
    config_overrides: {
      timeframe_mode: 'general',
      timeframe_general: '7d'
    }
  })
}).then(r => r.json());

// Poll for completion
const jobId = batchJob.data.job_id;
let status;
do {
  await new Promise(resolve => setTimeout(resolve, 5000)); // Wait 5 seconds
  status = await fetch(`/api/pnl/batch/status/${jobId}`).then(r => r.json());
} while (status.data.status === 'running');

// Get results
const results = await fetch(`/api/pnl/batch/results/${jobId}`).then(r => r.json());
```

### Monitoring Continuous Mode
```javascript
// Get discovered wallets
const discovered = await fetch('/api/pnl/continuous/discovered-wallets?limit=10&min_pnl=100')
  .then(r => r.json());

// Get details for specific wallet
const walletDetails = await fetch(`/api/pnl/continuous/discovered-wallets/${walletAddress}/details`)
  .then(r => r.json());

// Check DexScreener monitoring status
const dexStatus = await fetch('/api/dex/status').then(r => r.json());
```

This API provides everything needed for a comprehensive P&L tracking frontend with both manual analysis and automated discovery capabilities.