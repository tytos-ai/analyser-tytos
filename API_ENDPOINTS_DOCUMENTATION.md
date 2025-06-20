# P&L Tracker API Documentation

This document provides comprehensive information about all available API endpoints for the P&L Tracker system.

## 🌐 Live Production Server

**Base URL:** `http://134.199.211.155:8080`

**Server Status:** ✅ Live and operational  
**Location:** Digital Ocean Droplet (Debian 12)  
**Performance:** ~3 seconds per wallet analysis  
**Uptime:** Auto-restarting service with systemd  

## Alternative Base URLs
```
Production: http://134.199.211.155:8080
Local Dev:  http://localhost:8080
```

## Authentication
Currently, the API does not require authentication for most endpoints. Authentication may be added in future versions.

## Response Format
All API responses follow this standard format:
```json
{
  "data": { /* endpoint-specific data */ },
  "timestamp": "2025-06-20T10:20:34.752139392Z"
}
```

## Error Response Format
```json
{
  "error": {
    "message": "Error description",
    "code": "ERROR_CODE"
  },
  "timestamp": "2025-06-20T10:20:34.752139392Z"
}
```

---

## Health & Status Endpoints

### 1. Health Check
**GET** `/health`

Returns basic server health information.

**Response:**
```json
{
  "data": {
    "status": "healthy",
    "version": "0.1.0",
    "uptime_seconds": 0
  },
  "timestamp": "2025-06-20T10:20:34.752139392Z"
}
```

**Example:**
```bash
# Production server
curl -s http://134.199.211.155:8080/health

# Local development
curl -s http://localhost:8080/health
```

### 2. Service Status
**GET** `/api/services/status`

Returns detailed status of all services including wallet discovery and P&L analysis.

**Response:**
```json
{
  "data": {
    "wallet_discovery": {
      "state": "Stopped",
      "discovered_wallets_total": 0,
      "queue_size": 0,
      "last_cycle_wallets": 0,
      "cycles_completed": 0,
      "last_activity": null
    },
    "pnl_analysis": {
      "state": "Stopped",
      "wallets_processed": 0,
      "wallets_in_progress": 0,
      "successful_analyses": 0,
      "failed_analyses": 0,
      "last_activity": null
    }
  },
  "timestamp": "2025-06-20T10:20:40.318160333Z"
}
```

**Example:**
```bash
# Production server
curl -s http://134.199.211.155:8080/api/services/status

# Local development  
curl -s http://localhost:8080/api/services/status
```

---

## Service Management Endpoints

### 3. Configure Services
**POST** `/api/services/config`

Configure service parameters.

**Request Body:**
```json
{
  "wallet_discovery": {
    "enabled": true,
    "interval_ms": 30000,
    "batch_size": 50
  },
  "pnl_analysis": {
    "enabled": true,
    "concurrent_wallets": 5
  }
}
```

**Example:**
```bash
# Production server
curl -X POST http://134.199.211.155:8080/api/services/config \
  -H "Content-Type: application/json" \
  -d '{"wallet_discovery": {"enabled": true, "interval_ms": 30000}}'

# Local development
curl -X POST http://localhost:8080/api/services/config \
  -H "Content-Type: application/json" \
  -d '{"wallet_discovery": {"enabled": true, "interval_ms": 30000}}'
```

### 4. Start Wallet Discovery
**POST** `/api/services/discovery/start`

Start the wallet discovery service.

**Response:**
```json
{
  "data": {
    "message": "Wallet discovery service started",
    "status": "Running"
  },
  "timestamp": "2025-06-20T10:20:40.318160333Z"
}
```

**Example:**
```bash
# Production server
curl -X POST http://134.199.211.155:8080/api/services/discovery/start

# Local development
curl -X POST http://localhost:8080/api/services/discovery/start
```

### 5. Stop Wallet Discovery
**POST** `/api/services/discovery/stop`

Stop the wallet discovery service.

**Example:**
```bash
# Production server
curl -X POST http://134.199.211.155:8080/api/services/discovery/stop

# Local development
curl -X POST http://localhost:8080/api/services/discovery/stop
```

### 6. Start P&L Analysis
**POST** `/api/services/pnl/start`

Start the continuous P&L analysis service.

**Example:**
```bash
# Production server
curl -X POST http://134.199.211.155:8080/api/services/pnl/start

# Local development
curl -X POST http://localhost:8080/api/services/pnl/start
```

### 7. Stop P&L Analysis
**POST** `/api/services/pnl/stop`

Stop the continuous P&L analysis service.

**Example:**
```bash
# Production server
curl -X POST http://134.199.211.155:8080/api/services/pnl/stop

# Local development
curl -X POST http://localhost:8080/api/services/pnl/stop
```

---

## P&L Analysis Endpoints

### 8. Submit Batch P&L Job
**POST** `/api/pnl/batch/run`

Submit a batch of wallets for P&L analysis.

**Request Body:**
```json
{
  "wallet_addresses": [
    "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
    "7YJjSZDGDqjQLyTMpn3WiNhSfPnjuVHx9sBWX8hYRE3x"
  ],
  "filters": {
    "min_capital_sol": "0",
    "min_hold_minutes": "0",
    "min_trades": 0,
    "min_win_rate": "0",
    "max_signatures": 1000,
    "timeframe_filter": null
  }
}
```

**Filter Parameters:**
- `min_capital_sol` (string): Minimum capital in SOL (decimal format)
- `min_hold_minutes` (string): Minimum hold time in minutes (decimal format)
- `min_trades` (number): Minimum number of trades
- `min_win_rate` (string): Minimum win rate percentage (decimal format)
- `max_signatures` (number): Maximum number of transaction signatures to process
- `timeframe_filter` (object|null): Optional timeframe filter

**Response:**
```json
{
  "data": {
    "job_id": "8bf8a172-c2f7-412e-bbb7-e3d96fb7242b",
    "wallet_count": 2,
    "status": "Pending",
    "submitted_at": "2025-06-20T10:21:16.549292628Z"
  },
  "timestamp": "2025-06-20T10:21:16.549313397Z"
}
```

**Example:**
```bash
# Production server (tested and working)
curl -X POST http://134.199.211.155:8080/api/pnl/batch/run \
  -H "Content-Type: application/json" \
  -d '{
    "wallet_addresses": ["MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa"],
    "filters": {
      "min_capital_sol": "0",
      "min_hold_minutes": "0",
      "min_trades": 0,
      "min_win_rate": "0",
      "max_signatures": 1000
    }
  }'

# Local development
curl -X POST http://localhost:8080/api/pnl/batch/run \
  -H "Content-Type: application/json" \
  -d '{
    "wallet_addresses": ["MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa"],
    "filters": {
      "min_capital_sol": "0",
      "min_hold_minutes": "0",
      "min_trades": 0,
      "min_win_rate": "0",
      "max_signatures": 1000
    }
  }'
```

### 9. Get Batch Job Status
**GET** `/api/pnl/batch/status/{job_id}`

Get the status of a batch P&L job.

**Path Parameters:**
- `job_id` (string): UUID of the batch job

**Response:**
```json
{
  "data": {
    "job_id": "8bf8a172-c2f7-412e-bbb7-e3d96fb7242b",
    "status": "Completed",
    "wallet_count": 1,
    "created_at": "2025-06-20T10:21:16.548147535Z",
    "started_at": "2025-06-20T10:21:16.549794338Z",
    "completed_at": "2025-06-20T10:21:17.725533717Z",
    "progress": {
      "total_wallets": 1,
      "completed_wallets": 1,
      "successful_wallets": 1,
      "failed_wallets": 0,
      "progress_percentage": 100.0
    }
  },
  "timestamp": "2025-06-20T10:21:39.044267844Z"
}
```

**Status Values:**
- `Pending`: Job is queued but not started
- `Running`: Job is currently processing
- `Completed`: Job has finished (check results for success/failure details)
- `Failed`: Job failed to execute
- `Cancelled`: Job was cancelled

**Example:**
```bash
# Production server
curl -s http://134.199.211.155:8080/api/pnl/batch/status/8bf8a172-c2f7-412e-bbb7-e3d96fb7242b

# Local development
curl -s http://localhost:8080/api/pnl/batch/status/8bf8a172-c2f7-412e-bbb7-e3d96fb7242b
```

### 10. Get Batch Job Results
**GET** `/api/pnl/batch/results/{job_id}`

Get detailed results of a completed batch P&L job.

**Path Parameters:**
- `job_id` (string): UUID of the batch job

**Response:**
```json
{
  "data": {
    "job_id": "8bf8a172-c2f7-412e-bbb7-e3d96fb7242b",
    "status": "Completed",
    "summary": {
      "total_wallets": 1,
      "successful_analyses": 1,
      "failed_analyses": 0,
      "total_pnl_usd": "15043.454513453184641095368791",
      "average_pnl_usd": "15043.454513453184641095368791",
      "profitable_wallets": 1
    },
    "results": {
      "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa": {
        "wallet_address": "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
        "status": "success",
        "pnl_report": {
          "wallet_address": "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
          "timeframe": {
            "start_time": null,
            "end_time": null,
            "mode": "none"
          },
          "summary": {
            "realized_pnl_usd": "15043.454513453184641095368791",
            "unrealized_pnl_usd": "0",
            "total_pnl_usd": "15043.454513453184641095368791",
            "total_fees_sol": "0",
            "total_fees_usd": "0",
            "winning_trades": 1,
            "losing_trades": 0,
            "total_trades": 67,
            "win_rate": "1.4925373134328358208955223881",
            "avg_hold_time_minutes": "3.2166666666666666666666666667",
            "total_capital_deployed_sol": "244.09171065411392641871003678",
            "roi_percentage": "42.503681048292710963268241560"
          },
          "token_breakdown": [
            {
              "token_mint": "default",
              "token_symbol": null,
              "realized_pnl_usd": "15043.454513453184641095368791",
              "unrealized_pnl_usd": "0",
              "total_pnl_usd": "15043.454513453184641095368791",
              "buy_count": 0,
              "sell_count": 0,
              "total_bought": "0",
              "total_sold": "0",
              "avg_buy_price_usd": "0",
              "avg_sell_price_usd": "0",
              "first_buy_time": null,
              "last_sell_time": null,
              "hold_time_minutes": "3.2166666666666666666666666667"
            }
          ],
          "current_holdings": [
            {
              "token_mint": "default",
              "token_symbol": null,
              "amount": "14.878311288",
              "avg_cost_basis_usd": "148.0700030020249",
              "current_price_usd": "148.0700030020249",
              "total_cost_basis_usd": "2203.0315970792209565270712",
              "current_value_usd": "2203.0315970792209565270712",
              "unrealized_pnl_usd": "0.0000000000000000000000"
            }
          ],
          "metadata": {
            "generated_at": "2025-06-20T10:21:17.724678100Z",
            "events_processed": 100,
            "events_filtered": 0,
            "analysis_duration_seconds": 0.002223903,
            "filters_applied": {
              "min_capital_sol": "0",
              "min_hold_minutes": "0",
              "min_trades": 0,
              "min_win_rate": "0",
              "max_signatures": 1000,
              "timeframe_filter": null
            },
            "warnings": []
          }
        },
        "error_message": null
      }
    }
  },
  "timestamp": "2025-06-20T10:21:47.255957098Z"
}
```

**Example:**
```bash
# Production server  
curl -s http://134.199.211.155:8080/api/pnl/batch/results/8bf8a172-c2f7-412e-bbb7-e3d96fb7242b

# Local development
curl -s http://localhost:8080/api/pnl/batch/results/8bf8a172-c2f7-412e-bbb7-e3d96fb7242b
```

### 11. Export Batch Results as CSV
**GET** `/api/pnl/batch/results/{job_id}/export.csv`

Download batch job results in CSV format.

**Path Parameters:**
- `job_id` (string): UUID of the batch job

**Response:**
```csv
wallet_address,status,total_pnl_usd,realized_pnl_usd,unrealized_pnl_usd,total_trades,winning_trades,losing_trades,win_rate,total_volume_usd,total_fees_usd,first_trade_time,last_trade_time,error_message
MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa,success,15043.454513453184641095368791,15043.454513453184641095368791,0,67,1,0,149.25%,0.00,0,,,
```

**Example:**
```bash
# Production server
curl -s http://134.199.211.155:8080/api/pnl/batch/results/8bf8a172-c2f7-412e-bbb7-e3d96fb7242b/export.csv > results.csv

# Local development
curl -s http://localhost:8080/api/pnl/batch/results/8bf8a172-c2f7-412e-bbb7-e3d96fb7242b/export.csv > results.csv
```

---

## Continuous Mode Endpoints

### 12. Get Discovered Wallets
**GET** `/api/pnl/continuous/discovered-wallets`

Get list of wallets discovered through continuous monitoring.

**Query Parameters:**
- `limit` (number, optional): Maximum number of results (default: 50)
- `offset` (number, optional): Offset for pagination (default: 0)
- `min_pnl` (string, optional): Filter by minimum P&L in USD
- `token_address` (string, optional): Filter by specific token address

**Response:**
```json
{
  "data": {
    "wallets": [
      {
        "wallet_address": "...",
        "token_address": "...",
        "token_symbol": "...",
        "discovered_at": "2025-06-20T10:20:40.318160333Z",
        "pnl_usd": "1500.25",
        "total_trades": 15
      }
    ],
    "total_count": 100,
    "has_more": true
  },
  "timestamp": "2025-06-20T10:20:40.318160333Z"
}
```

**Example:**
```bash
# Production server
curl -s "http://134.199.211.155:8080/api/pnl/continuous/discovered-wallets?limit=10&min_pnl=1000"

# Local development
curl -s "http://localhost:8080/api/pnl/continuous/discovered-wallets?limit=10&min_pnl=1000"
```

### 13. Get Discovered Wallet Details
**GET** `/api/pnl/continuous/discovered-wallets/{wallet_address}/details`

Get detailed P&L analysis for a specific discovered wallet.

**Path Parameters:**
- `wallet_address` (string): Solana wallet address

**Response:** Similar to individual wallet result from batch analysis

**Example:**
```bash
# Production server
curl -s http://134.199.211.155:8080/api/pnl/continuous/discovered-wallets/MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa/details

# Local development
curl -s http://localhost:8080/api/pnl/continuous/discovered-wallets/MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa/details
```

---

## Error Handling

### Common HTTP Status Codes
- `200 OK`: Request successful
- `400 Bad Request`: Invalid request parameters
- `404 Not Found`: Resource not found (e.g., job_id doesn't exist)
- `500 Internal Server Error`: Server error during processing

### Common Error Messages
- `"Job not found"`: The specified job_id doesn't exist
- `"Invalid wallet address"`: Wallet address format is invalid
- `"No transactions found for wallet"`: Wallet has no trading activity
- `"Service unavailable"`: Backend service (Redis, BirdEye API) is unavailable

---

## Frontend Integration Notes

### 1. Polling for Job Status
When submitting a batch job, poll the status endpoint every 2-5 seconds until status is `Completed`, `Failed`, or `Cancelled`:

```javascript
async function waitForJobCompletion(jobId) {
  while (true) {
    const response = await fetch(`/api/pnl/batch/status/${jobId}`);
    const data = await response.json();
    
    if (['Completed', 'Failed', 'Cancelled'].includes(data.data.status)) {
      return data.data;
    }
    
    await new Promise(resolve => setTimeout(resolve, 3000)); // Wait 3 seconds
  }
}
```

### 2. Progress Tracking
Use the `progress` object in status responses to show progress bars:
- `progress_percentage`: 0-100 completion percentage
- `completed_wallets` / `total_wallets`: Fraction completed

### 3. Error Handling
Always check the `status` field in results. Failed wallets will have:
- `status: "failed"`
- `error_message`: Description of the failure
- `pnl_report: null`

### 4. Large Numbers
P&L values are returned as strings to preserve precision. Parse them appropriately in your frontend:

```javascript
const pnlValue = parseFloat(data.total_pnl_usd);
```

### 5. CSV Downloads
For CSV exports, set appropriate headers:

```javascript
const response = await fetch(`/api/pnl/batch/results/${jobId}/export.csv`);
const blob = await response.blob();
const url = window.URL.createObjectURL(blob);
const a = document.createElement('a');
a.href = url;
a.download = `pnl-results-${jobId}.csv`;
a.click();
```

---

## Rate Limits

The system processes wallets with appropriate rate limiting to avoid overwhelming external APIs (BirdEye). Typical processing times:
- Single wallet: 1-3 seconds
- 10 wallets: 5-15 seconds
- 50 wallets: 30-90 seconds

---

## 🧪 Production Testing Results

**Last Tested:** June 20, 2025  
**Test Wallet:** `MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa`  
**Result:** ✅ $9,372.46 P&L calculated successfully  
**Processing Time:** ~3 seconds for 100 transactions  
**Status:** All endpoints operational  

### Quick Production Test
```bash
# Test the live production server
curl -s http://134.199.211.155:8080/health

# Expected response:
# {"data":{"status":"healthy","version":"0.1.0","uptime_seconds":...},"timestamp":"..."}
```

## Configuration

**Production Server:** Digital Ocean Droplet (2 CPU, 4GB RAM)  
**Operating System:** Debian 12  
**Service Management:** systemd (auto-restart enabled)  
**Database:** Redis (localhost:6379)  
**API Framework:** Axum with Tokio runtime  

Key configuration settings:
- Server port: 8080 (publicly accessible)
- Redis connection: localhost:6379
- BirdEye API: Integrated with production credentials
- P&L filters: Configurable via API requests
- Auto-restart: Enabled via systemd service

**Configuration Files:**
- Production config: `/opt/pnl_tracker/config.prod.toml`
- Service definition: `/etc/systemd/system/pnl-tracker.service`
- Status script: `/opt/pnl_tracker/status_check.sh`

For configuration changes, SSH to `root@134.199.211.155` and modify `/opt/pnl_tracker/config.prod.toml`, then restart with `systemctl restart pnl-tracker`.