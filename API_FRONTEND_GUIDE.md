# 🚀 Wallet Analyzer API Frontend Integration Guide

**Version**: 1.0  
**Last Updated**: December 30, 2024  
**Base URL**: `http://localhost:8080` (configurable)

## 📋 Table of Contents

1. [Overview](#overview)
2. [Authentication](#authentication)
3. [Common Data Types](#common-data-types)
4. [Health & Status Endpoints](#health--status-endpoints)
5. [Configuration Management](#configuration-management)
6. [Batch P&L Analysis](#batch-pnl-analysis)
7. [Service Management](#service-management)
8. [Results Retrieval](#results-retrieval)
9. [Error Handling](#error-handling)
10. [WebSocket Events](#websocket-events)
11. [Rate Limiting](#rate-limiting)
12. [Examples](#examples)

---

## 🔍 Overview

The Wallet Analyzer API provides comprehensive Solana wallet analysis capabilities including:
- **Batch P&L Analysis**: Analyze multiple wallets with custom parameters
- **Continuous Discovery**: 24/7 monitoring of trending tokens for wallet discovery
- **Real-time Configuration**: Runtime parameter overrides without service restart
- **Time-filtered Analysis**: Analyze specific time periods with optimal API usage

### Key Features
- ✅ **Runtime Configuration**: Override default parameters per request
- ✅ **Time Filtering**: Efficient BirdEye API usage with time bounds
- ✅ **Smart Parameter Merging**: User overrides + config defaults
- ✅ **Real-time Status**: WebSocket updates for long-running operations
- ✅ **Comprehensive Validation**: Parameter validation with helpful error messages

---

## 🔐 Authentication

Currently, the API runs in development mode without authentication. In production:
- All `/api/config` endpoints require authentication
- Service control endpoints require authentication
- Read-only endpoints are public

```javascript
// Future authentication header
const headers = {
  'Authorization': 'Bearer YOUR_API_KEY',
  'Content-Type': 'application/json'
};
```

---

## 📊 Common Data Types

### PnLFilters
Core filtering parameters for P&L analysis:

```typescript
interface PnLFilters {
  // Capital filtering
  min_capital_sol?: number;           // Minimum wallet capital in SOL (default: 0.0)
  
  // Trading behavior filtering  
  min_hold_minutes?: number;          // Minimum average hold time in minutes (default: 0.0)
  min_trades?: number;                // Minimum number of trades (default: 0)
  min_win_rate?: number;              // Minimum win rate percentage 0-100 (default: 0.0)
  
  // Transaction limits (IMPORTANT: Controls API efficiency)
  max_transactions_to_fetch?: number; // Limit BirdEye API calls (default: 500)
  max_signatures?: number;            // Limit P&L processing (default: 500, auto-adjusted)
  
  // Time filtering (Optimizes BirdEye API usage)
  timeframe_filter?: AnalysisTimeframe;
}
```

### AnalysisTimeframe
Time period specification:

```typescript
interface AnalysisTimeframe {
  start_time?: string;    // ISO 8601: "2024-12-01T00:00:00Z" 
  end_time?: string;      // ISO 8601: "2024-12-31T23:59:59Z" (optional)
  mode: string;           // "specific" | "general" | "none"
}
```

### ServiceControlRequest
Universal service management:

```typescript
interface ServiceControlRequest {
  action: "start" | "stop" | "restart";
  service: "wallet_discovery" | "pnl_analysis";
  config_override?: PnLFilters;  // Runtime configuration override
}
```

### BatchJobRequest
Batch P&L analysis request:

```typescript
interface BatchJobRequest {
  wallet_addresses: string[];  // Array of Solana wallet addresses
  filters?: PnLFilters;       // Optional parameter overrides
}
```

---

## 🏥 Health & Status Endpoints

### GET `/health`
Quick health check

**Response:**
```json
{
  "data": {
    "status": "healthy",
    "version": "0.1.0", 
    "uptime_seconds": 3600
  },
  "timestamp": "2024-12-30T10:00:00Z"
}
```

### GET `/health/detailed`
Comprehensive system health

**Response:**
```json
{
  "data": {
    "status": "healthy",
    "version": "0.1.0",
    "uptime_seconds": 3600,
    "components": {
      "redis": {
        "connected": true,
        "latency_ms": 2,
        "error": null
      },
      "birdeye_api": {
        "accessible": true,
        "latency_ms": 150,
        "error": null
      },
      "services": {
        "wallet_discovery": "Running",
        "pnl_analysis": "Running"
      }
    }
  },
  "timestamp": "2024-12-30T10:00:00Z"
}
```

---

## ⚙️ Configuration Management

### GET `/api/config`
Retrieve current system configuration

**Response:**
```json
{
  "data": {
    "system": {
      "debug_mode": true,
      "redis_mode": true,
      "process_loop_ms": 30000
    },
    "birdeye": {
      "default_max_transactions": 500,
      "max_transactions_per_trader": 100,
      "rate_limit_per_second": 100
    },
    "pnl": {
      "timeframe_mode": "none",
      "timeframe_general": "7d",
      "wallet_min_capital": 0.0,
      "aggregator_min_hold_minutes": 0.0,
      "amount_trades": 0,
      "win_rate": 0.0
    }
  },
  "timestamp": "2024-12-30T10:00:00Z"
}
```

### POST `/api/config`
Update system configuration (requires authentication)

**Request:**
```json
{
  "pnl_filters": {
    "timeframe_mode": "general",
    "timeframe_general": "30d",
    "wallet_min_capital": 1.0
  }
}
```

---

## 📈 Batch P&L Analysis

### POST `/api/pnl/batch/run`
Submit batch P&L analysis job

**Request Examples:**

#### Basic Request (Use All Defaults)
```json
{
  "wallet_addresses": [
    "GBJ4MZe8fqpA6UVgjh19BwJPMb79KDfMv78XnFVxgH2Q",
    "Hr9pzexrBge3vgmBNRR8u42CNQgBXdHm4UkUN2DH4a7r"
  ]
}
```

#### Advanced Request (Custom Parameters)
```json
{
  "wallet_addresses": [
    "GBJ4MZe8fqpA6UVgjh19BwJPMb79KDfMv78XnFVxgH2Q"
  ],
  "filters": {
    "min_capital_sol": 5.0,
    "min_trades": 10,
    "min_win_rate": 60.0,
    "max_transactions_to_fetch": 1000,
    "timeframe_filter": {
      "start_time": "2024-12-01T00:00:00Z",
      "end_time": "2024-12-31T23:59:59Z", 
      "mode": "specific"
    }
  }
}
```

#### Time-Filtered Request (Last 7 Days)
```json
{
  "wallet_addresses": ["GBJ4MZe8fqpA6UVgjh19BwJPMb79KDfMv78XnFVxgH2Q"],
  "filters": {
    "timeframe_filter": {
      "start_time": "2024-12-23T00:00:00Z",
      "mode": "specific"
    }
  }
}
```

**Response:**
```json
{
  "data": {
    "job_id": "123e4567-e89b-12d3-a456-426614174000",
    "wallet_count": 2,
    "status": "Pending",
    "submitted_at": "2024-12-30T10:00:00Z"
  },
  "timestamp": "2024-12-30T10:00:00Z"
}
```

### GET `/api/pnl/batch/status/{job_id}`
Check batch job status

**Response:**
```json
{
  "data": {
    "job_id": "123e4567-e89b-12d3-a456-426614174000",
    "status": "Running",
    "wallet_count": 2,
    "created_at": "2024-12-30T10:00:00Z",
    "started_at": "2024-12-30T10:00:05Z",
    "completed_at": null,
    "progress": {
      "total_wallets": 2,
      "completed_wallets": 1,
      "successful_wallets": 1,
      "failed_wallets": 0,
      "progress_percentage": 50.0
    }
  },
  "timestamp": "2024-12-30T10:01:00Z"
}
```

### GET `/api/pnl/batch/results/{job_id}`
Retrieve batch job results

**Response:**
```json
{
  "data": {
    "job_id": "123e4567-e89b-12d3-a456-426614174000",
    "status": "Completed",
    "summary": {
      "total_wallets": 2,
      "successful_analyses": 2,
      "failed_analyses": 0,
      "total_pnl_usd": 1250.50,
      "average_pnl_usd": 625.25,
      "profitable_wallets": 1
    },
    "results": {
      "GBJ4MZe8fqpA6UVgjh19BwJPMb79KDfMv78XnFVxgH2Q": {
        "wallet_address": "GBJ4MZe8fqpA6UVgjh19BwJPMb79KDfMv78XnFVxgH2Q",
        "status": "success",
        "pnl_report": {
          "wallet_address": "GBJ4MZe8fqpA6UVgjh19BwJPMb79KDfMv78XnFVxgH2Q",
          "timeframe": {
            "start_time": "2024-12-01T00:00:00Z",
            "end_time": null,
            "mode": "specific"
          },
          "summary": {
            "total_pnl_usd": 1250.50,
            "realized_pnl_usd": 980.30,
            "unrealized_pnl_usd": 270.20,
            "total_trades": 25,
            "winning_trades": 18,
            "losing_trades": 7,
            "win_rate": 72.0,
            "roi_percentage": 15.2
          },
          "token_breakdown": [...],
          "current_holdings": [...],
          "metadata": {
            "generated_at": "2024-12-30T10:05:00Z",
            "events_processed": 245,
            "events_filtered": 55,
            "analysis_duration_seconds": 2.5
          }
        },
        "error_message": null
      }
    }
  },
  "timestamp": "2024-12-30T10:05:00Z"
}
```

### GET `/api/pnl/batch/results/{job_id}/export.csv`
Download results as CSV

**Response:** CSV file with headers:
```csv
wallet_address,token_symbol,total_pnl_usd,realized_pnl_usd,unrealized_pnl_usd,roi_percentage,total_trades,win_rate,analyzed_at
```

---

## 🎛️ Service Management

### GET `/api/services/status`
Get service status

**Response:**
```json
{
  "data": {
    "orchestrator": {
      "total_running_jobs": 2,
      "pending_batch_jobs": 1,
      "running_batch_jobs": 1,
      "completed_batch_jobs": 15,
      "failed_batch_jobs": 0,
      "queue_size": 5
    },
    "dex_client": {
      "enabled": true,
      "connected": true,
      "last_activity": "2024-12-30T10:04:30Z",
      "processed_pairs": 125,
      "discovered_wallets": 45
    },
    "config": {
      "redis_mode": true,
      "birdeye_api_configured": true,
      "pnl_filters": {
        "timeframe_mode": "none",
        "min_capital_sol": 0.0,
        "min_trades": 0,
        "win_rate": 0.0
      }
    }
  },
  "timestamp": "2024-12-30T10:05:00Z"
}
```

### POST `/api/services/control`
Universal service control with optional configuration

**Request Examples:**

#### Start Wallet Discovery (Default Config)
```json
{
  "action": "start",
  "service": "wallet_discovery"
}
```

#### Start with Custom Configuration
```json
{
  "action": "start", 
  "service": "wallet_discovery",
  "config_override": {
    "min_capital_sol": 10.0,
    "min_trades": 5,
    "max_transactions_to_fetch": 200,
    "timeframe_filter": {
      "start_time": "2024-12-01T00:00:00Z",
      "mode": "specific"
    }
  }
}
```

#### Stop Service
```json
{
  "action": "stop",
  "service": "wallet_discovery"
}
```

#### Restart with New Config
```json
{
  "action": "restart",
  "service": "wallet_discovery", 
  "config_override": {
    "max_transactions_to_fetch": 1000
  }
}
```

**Response:**
```json
{
  "data": {
    "message": "Wallet discovery service started successfully"
  },
  "timestamp": "2024-12-30T10:00:00Z"
}
```

---

## 📊 Results Retrieval

### GET `/api/results`
Get all P&L results with pagination

**Query Parameters:**
- `offset`: Number of results to skip (default: 0)
- `limit`: Number of results to return (default: 50, max: 1000)
- `sort_by`: "pnl" | "analyzed_at" | "wallet_address" (default: "analyzed_at")
- `order`: "asc" | "desc" (default: "desc")

**Example Request:**
```
GET /api/results?offset=0&limit=10&sort_by=pnl&order=desc
```

**Response:**
```json
{
  "data": {
    "results": [
      {
        "wallet_address": "GBJ4MZe8fqpA6UVgjh19BwJPMb79KDfMv78XnFVxgH2Q",
        "token_address": "So11111111111111111111111111111111111111112",
        "token_symbol": "SOL",
        "total_pnl_usd": 1250.50,
        "realized_pnl_usd": 980.30,
        "unrealized_pnl_usd": 270.20,
        "roi_percentage": 15.2,
        "total_trades": 25,
        "win_rate": 72.0,
        "analyzed_at": "2024-12-30T10:05:00Z"
      }
    ],
    "pagination": {
      "total_count": 1250,
      "limit": 10,
      "offset": 0,
      "has_more": true
    },
    "summary": {
      "total_wallets": 1250,
      "profitable_wallets": 890,
      "total_pnl_usd": 125000.50,
      "average_pnl_usd": 100.0,
      "total_trades": 25000,
      "profitability_rate": 71.2,
      "last_updated": "2024-12-30T10:05:00Z"
    }
  },
  "timestamp": "2024-12-30T10:06:00Z"
}
```

### GET `/api/results/{wallet_address}/{token_address}`
Get detailed P&L report for specific wallet-token pair

**Response:**
```json
{
  "data": {
    "wallet_address": "GBJ4MZe8fqpA6UVgjh19BwJPMb79KDfMv78XnFVxgH2Q",
    "token_address": "So11111111111111111111111111111111111111112", 
    "token_symbol": "SOL",
    "pnl_report": {
      // Full PnLReport object
    },
    "analyzed_at": "2024-12-30T10:05:00Z"
  },
  "timestamp": "2024-12-30T10:06:00Z"
}
```

---

## ❌ Error Handling

### Error Response Format
```json
{
  "error": "Detailed error message",
  "timestamp": "2024-12-30T10:00:00Z"
}
```

### Common HTTP Status Codes

| Code | Status | Description |
|------|--------|-------------|
| 200 | OK | Request successful |
| 400 | Bad Request | Invalid request parameters |
| 401 | Unauthorized | Authentication required |
| 403 | Forbidden | Insufficient permissions |
| 404 | Not Found | Resource not found |
| 429 | Too Many Requests | Rate limit exceeded |
| 500 | Internal Server Error | Server error |

### Validation Errors

The API automatically validates and fixes parameter conflicts:

#### Example: max_signatures > max_transactions_to_fetch
**Request:**
```json
{
  "filters": {
    "max_transactions_to_fetch": 100,
    "max_signatures": 500
  }
}
```

**Behavior:** 
- System automatically adjusts `max_signatures` to 100
- Warning logged: "max_signatures (500) > max_transactions_to_fetch (100), adjusting max_signatures to match fetch limit"
- Request proceeds with corrected values

#### Example: Invalid timeframe
**Request:**
```json
{
  "filters": {
    "timeframe_filter": {
      "start_time": "2024-12-31T00:00:00Z",
      "end_time": "2024-12-01T00:00:00Z",
      "mode": "specific"
    }
  }
}
```

**Behavior:**
- System clears `end_time` to fix the conflict
- Warning logged: "Invalid timeframe: start_time >= end_time, clearing end_time"
- Analysis proceeds from start_time to now

---

## 🔌 WebSocket Events

### Connection
```javascript
const ws = new WebSocket('ws://localhost:8080/ws');
```

### Event Types

#### Job Progress Updates
```json
{
  "type": "job_progress",
  "job_id": "123e4567-e89b-12d3-a456-426614174000",
  "progress": {
    "completed_wallets": 15,
    "total_wallets": 50,
    "progress_percentage": 30.0
  }
}
```

#### Service Status Changes
```json
{
  "type": "service_status",
  "service": "wallet_discovery",
  "status": "Running",
  "timestamp": "2024-12-30T10:00:00Z"
}
```

#### New Wallet Discoveries
```json
{
  "type": "wallet_discovered",
  "wallet_address": "GBJ4MZe8fqpA6UVgjh19BwJPMb79KDfMv78XnFVxgH2Q",
  "token_symbol": "BONK",
  "discovery_reason": "trending_token"
}
```

---

## ⏱️ Rate Limiting

### Current Limits
- **Configuration endpoints**: 10 requests/minute
- **Batch job submission**: 5 requests/minute  
- **Status/results endpoints**: 100 requests/minute
- **Service control**: 20 requests/minute

### Rate Limit Headers
```
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 95
X-RateLimit-Reset: 1640995200
```

---

## 📚 Frontend Integration Examples

### React Hook for Batch Analysis

```javascript
import { useState, useEffect } from 'react';

export function useBatchAnalysis() {
  const [jobs, setJobs] = useState(new Map());
  
  const submitJob = async (wallets, filters = {}) => {
    const response = await fetch('/api/pnl/batch/run', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ 
        wallet_addresses: wallets,
        filters
      })
    });
    
    const { data } = await response.json();
    setJobs(prev => new Map(prev.set(data.job_id, { 
      ...data, 
      status: 'Pending' 
    })));
    
    return data.job_id;
  };
  
  const pollJobStatus = async (jobId) => {
    const response = await fetch(`/api/pnl/batch/status/${jobId}`);
    const { data } = await response.json();
    
    setJobs(prev => new Map(prev.set(jobId, data)));
    
    if (data.status === 'Completed') {
      const resultsResponse = await fetch(`/api/pnl/batch/results/${jobId}`);
      const results = await resultsResponse.json();
      
      setJobs(prev => new Map(prev.set(jobId, { 
        ...data, 
        results: results.data 
      })));
    }
    
    return data;
  };
  
  return { jobs, submitJob, pollJobStatus };
}
```

### Time-Filtered Analysis Component

```javascript
function TimeFilteredAnalysis() {
  const { submitJob, pollJobStatus } = useBatchAnalysis();
  
  const analyzeLastWeek = async (wallets) => {
    const oneWeekAgo = new Date();
    oneWeekAgo.setDate(oneWeekAgo.getDate() - 7);
    
    const jobId = await submitJob(wallets, {
      timeframe_filter: {
        start_time: oneWeekAgo.toISOString(),
        mode: 'specific'
      },
      max_transactions_to_fetch: 200, // Optimize for recent data
      min_trades: 5
    });
    
    // Poll until complete
    const interval = setInterval(async () => {
      const status = await pollJobStatus(jobId);
      if (status.status === 'Completed') {
        clearInterval(interval);
        console.log('Analysis complete:', status);
      }
    }, 2000);
  };
  
  return (
    <button onClick={() => analyzeLastWeek(['wallet1', 'wallet2'])}>
      Analyze Last Week
    </button>
  );
}
```

### Service Management Hook

```javascript
export function useServiceManagement() {
  const [services, setServices] = useState({});
  
  const controlService = async (action, service, config = null) => {
    const response = await fetch('/api/services/control', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        action,
        service,
        ...(config && { config_override: config })
      })
    });
    
    const result = await response.json();
    
    // Refresh service status
    await refreshStatus();
    
    return result;
  };
  
  const refreshStatus = async () => {
    const response = await fetch('/api/services/status');
    const { data } = await response.json();
    setServices(data);
  };
  
  const startDiscoveryWithFilters = (filters) => {
    return controlService('start', 'wallet_discovery', filters);
  };
  
  useEffect(() => {
    refreshStatus();
    const interval = setInterval(refreshStatus, 10000); // Poll every 10s
    return () => clearInterval(interval);
  }, []);
  
  return { 
    services, 
    controlService, 
    startDiscoveryWithFilters, 
    refreshStatus 
  };
}
```

---

## 🎯 Best Practices

### 1. **Optimize Transaction Limits**
```javascript
// For recent analysis (last 30 days)
const recentFilters = {
  timeframe_filter: {
    start_time: thirtyDaysAgo.toISOString(),
    mode: 'specific'
  },
  max_transactions_to_fetch: 200 // Reduced for efficiency
};

// For comprehensive analysis (all time)
const comprehensiveFilters = {
  max_transactions_to_fetch: 1000 // Higher limit
};
```

### 2. **Handle Long-Running Jobs**
```javascript
const pollWithExponentialBackoff = async (jobId, maxAttempts = 50) => {
  let attempt = 0;
  let delay = 1000; // Start with 1 second
  
  while (attempt < maxAttempts) {
    const status = await pollJobStatus(jobId);
    
    if (status.status === 'Completed' || status.status === 'Failed') {
      return status;
    }
    
    await new Promise(resolve => setTimeout(resolve, delay));
    delay = Math.min(delay * 1.5, 10000); // Cap at 10 seconds
    attempt++;
  }
  
  throw new Error('Job polling timeout');
};
```

### 3. **Validate Parameters**
```javascript
const validateFilters = (filters) => {
  if (filters.max_signatures && filters.max_transactions_to_fetch) {
    if (filters.max_signatures > filters.max_transactions_to_fetch) {
      console.warn('max_signatures will be auto-adjusted to max_transactions_to_fetch');
    }
  }
  
  if (filters.timeframe_filter) {
    const { start_time, end_time } = filters.timeframe_filter;
    if (start_time && end_time && new Date(start_time) >= new Date(end_time)) {
      throw new Error('start_time must be before end_time');
    }
  }
  
  return filters;
};
```

---

## 📞 Support & Resources

### Useful Endpoints for Development
- **Health Check**: `GET /health` - Quick API availability test
- **Detailed Health**: `GET /health/detailed` - Component status
- **Service Status**: `GET /api/services/status` - Current service state
- **Configuration**: `GET /api/config` - Current system settings

### Common Integration Patterns
1. **Polling**: Use exponential backoff for job status
2. **WebSocket**: Subscribe to real-time updates
3. **Caching**: Cache service status for better UX
4. **Error Handling**: Always handle rate limits and validation errors
5. **Time Filtering**: Use specific timeframes for better performance

### Performance Tips
- Use `max_transactions_to_fetch` to control API costs
- Implement proper timeframe filtering for recent analysis
- Poll job status with increasing intervals
- Cache results and service status appropriately
- Handle rate limits gracefully with exponential backoff

This API provides powerful wallet analysis capabilities with flexible configuration options. The smart parameter merging and validation ensure reliable operation while giving frontend developers full control over analysis parameters.