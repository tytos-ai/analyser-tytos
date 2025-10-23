# MessagePack Binary Format Implementation Plan

**Document Version:** 1.0
**Date:** 2025-10-15
**Author:** System Analysis
**Status:** Planning Phase

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Problem Analysis](#problem-analysis)
3. [MessagePack Solution](#messagepack-solution)
4. [Architecture Analysis](#architecture-analysis)
5. [Backend Implementation](#backend-implementation)
6. [Frontend Implementation](#frontend-implementation)
7. [Migration Strategy](#migration-strategy)
8. [Testing Plan](#testing-plan)
9. [Performance Benchmarking](#performance-benchmarking)
10. [Rollback Strategy](#rollback-strategy)
11. [Implementation Checklist](#implementation-checklist)

---

## Executive Summary

### Current Performance Issue

The `/api/results` endpoint is experiencing severe performance degradation when loading large result sets:

```
First page (25,000 records):
- Network fetch: 7,045ms (7s)
- JSON parse: 97,580ms (97.5s) ← BOTTLENECK
- Total: 104,626ms (104s)

Second page (25,000 records):
- Network fetch: 20,278ms (20s)
- JSON parse: 25,100ms (25s)
- Total: 45,382ms (45s)
```

**Root Cause:** Browser's `JSON.parse()` is extremely slow for large payloads (97 seconds for 25k records).

### Proposed Solution

Replace JSON serialization with **MessagePack binary format** for:
- **10-20x faster parsing** (97s → 5-10s)
- **30-50% smaller payload size**
- **Better browser performance** (less memory pressure)

### Expected Results

After implementation:
- First page load: **104s → 15-20s** (80% improvement)
- Second page load: **45s → 8-12s** (75% improvement)
- Total memory usage: **Reduced by 40-60%**

---

## Problem Analysis

### Current JSON Serialization Flow

#### Backend (Rust)

```rust
// api_server/src/handlers.rs:970
let json_response = Json(SuccessResponse::new(response));
```

Uses `axum::Json` which:
1. Serializes with `serde_json` (fast)
2. Sends as `Content-Type: application/json`
3. Browser receives JSON string

#### Frontend (TypeScript)

```typescript
// frontend/src/lib/api.ts:74
const data = await response.json()
```

Uses browser's native `JSON.parse()` which:
1. Parses entire string synchronously
2. Blocks main thread during parsing
3. Creates deep object hierarchy (expensive)
4. Takes **3.9ms per record** for first page

### Why JSON is Slow

1. **String-based format**: Must parse text character by character
2. **Type inference**: Must determine types during parsing
3. **String escaping**: Must process escape sequences
4. **Object creation**: Must allocate and initialize objects
5. **Single-threaded**: No Web Worker support in fetch API

### Measured Bottleneck

From performance logs:
```
⏱️ Network fetch completed: /api/results?limit=25000 in 7045.40ms
⏱️ JSON parse completed: /api/results?limit=25000 in 97580.00ms
```

**JSON parsing takes 14x longer than network transfer!**

---

## MessagePack Solution

### What is MessagePack?

MessagePack is a binary serialization format that:
- Encodes data as compact binary instead of text
- Preserves type information (no inference needed)
- Supports all JSON types plus binary data
- Has fast parsers for all languages

### Why MessagePack?

1. **Fast Parsing**: Binary format → direct memory mapping
2. **Compact Size**: 30-50% smaller than JSON
3. **Type Safety**: Types encoded in format
4. **Wide Support**: Libraries for Rust, JavaScript, etc.

### Performance Expectations

Based on benchmarks for similar data:

| Metric | JSON | MessagePack | Improvement |
|--------|------|-------------|-------------|
| Parse time (25k records) | 97,580ms | 5,000-10,000ms | **10-20x faster** |
| Payload size | 100% | 50-70% | **30-50% smaller** |
| Memory usage | 100% | 40-60% | **40-60% less** |

---

## Architecture Analysis

### Current API Flow

```
┌─────────────────────────────────────────────────────────┐
│ Backend (Rust)                                           │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  PostgreSQL → Rust Structs (f64 types)                 │
│       ↓                                                  │
│  serde_json::to_string() [Serialization: ~500ms]       │
│       ↓                                                  │
│  Axum Response: Content-Type: application/json          │
│                                                          │
└─────────────────────────────────────────────────────────┘
                         ↓ HTTP
┌─────────────────────────────────────────────────────────┐
│ Frontend (TypeScript)                                    │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  fetch().then(r => r.json())                            │
│       ↓                                                  │
│  JSON.parse() [BOTTLENECK: 97,580ms]                   │
│       ↓                                                  │
│  TypeScript objects with number types                    │
│                                                          │
└─────────────────────────────────────────────────────────┘
```

### Proposed MessagePack Flow

```
┌─────────────────────────────────────────────────────────┐
│ Backend (Rust)                                           │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  PostgreSQL → Rust Structs (f64 types)                 │
│       ↓                                                  │
│  rmp_serde::to_vec() [Serialization: ~300ms]           │
│       ↓                                                  │
│  Axum Response: Content-Type: application/msgpack       │
│                                                          │
└─────────────────────────────────────────────────────────┘
                         ↓ HTTP (Binary)
┌─────────────────────────────────────────────────────────┐
│ Frontend (TypeScript)                                    │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  fetch().then(r => r.arrayBuffer())                     │
│       ↓                                                  │
│  msgpack.decode() [FAST: 5,000-10,000ms]               │
│       ↓                                                  │
│  TypeScript objects with number types                    │
│                                                          │
└─────────────────────────────────────────────────────────┘
```

### Key Components

#### Backend Components

1. **Dependency**: `rmp-serde = "1.1"`
2. **Serializer**: `rmp_serde::to_vec()`
3. **Custom Axum Extractor**: `MsgPack<T>` (similar to `Json<T>`)
4. **Content-Type Header**: `application/msgpack`

#### Frontend Components

1. **Dependency**: `@msgpack/msgpack@^3.0.0`
2. **Decoder**: `msgpack.decode(arrayBuffer)`
3. **Fetch Header**: `Accept: application/msgpack`
4. **Fallback Support**: Keep JSON for backward compatibility

---

## Backend Implementation

### Step 1: Add Dependencies

**File:** `api_server/Cargo.toml`

```toml
[dependencies]
# ... existing dependencies ...

# MessagePack serialization
rmp-serde = "1.1"
```

### Step 2: Create Custom Axum Extractor

**File:** `api_server/src/msgpack_extractor.rs` (NEW FILE)

```rust
use axum::{
    extract::{FromRequest, Request},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use rmp_serde;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Custom Axum extractor for MessagePack requests
pub struct MsgPack<T>(pub T);

#[async_trait::async_trait]
impl<T, S> FromRequest<S> for MsgPack<T>
where
    T: for<'de> Deserialize<'de>,
    S: Send + Sync,
{
    type Rejection = MsgPackRejection;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = axum::body::Bytes::from_request(req, state)
            .await
            .map_err(|_| MsgPackRejection::BytesRejection)?;

        let value = rmp_serde::from_slice(&bytes)
            .map_err(|err| MsgPackRejection::DeserializeError(err.to_string()))?;

        Ok(MsgPack(value))
    }
}

/// Custom Axum response for MessagePack
impl<T> IntoResponse for MsgPack<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        match rmp_serde::to_vec(&self.0) {
            Ok(bytes) => {
                let mut res = Response::new(bytes.into());
                res.headers_mut().insert(
                    header::CONTENT_TYPE,
                    header::HeaderValue::from_static("application/msgpack"),
                );
                res
            }
            Err(err) => {
                tracing::error!("MessagePack serialization error: {}", err);
                (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response()
            }
        }
    }
}

#[derive(Debug)]
pub enum MsgPackRejection {
    BytesRejection,
    DeserializeError(String),
}

impl fmt::Display for MsgPackRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BytesRejection => write!(f, "Failed to buffer request body"),
            Self::DeserializeError(e) => write!(f, "Failed to deserialize MessagePack: {}", e),
        }
    }
}

impl IntoResponse for MsgPackRejection {
    fn into_response(self) -> Response {
        let status = match self {
            Self::BytesRejection => StatusCode::INTERNAL_SERVER_ERROR,
            Self::DeserializeError(_) => StatusCode::BAD_REQUEST,
        };
        (status, self.to_string()).into_response()
    }
}
```

### Step 3: Add Module Declaration

**File:** `api_server/src/lib.rs` or `api_server/src/main.rs`

```rust
mod msgpack_extractor;
use msgpack_extractor::MsgPack;
```

### Step 4: Update Handler with Content Negotiation

**File:** `api_server/src/handlers.rs`

```rust
use crate::msgpack_extractor::MsgPack;
use axum::http::header;

pub async fn get_all_results(
    State(state): State<AppState>,
    Query(query): Query<AllResultsQuery>,
    headers: axum::http::HeaderMap, // Add headers parameter
) -> Result<impl IntoResponse, ApiError> {
    let start_time = std::time::Instant::now();
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(50).min(25000);

    info!("⏱️  [PERF] get_all_results START - offset: {}, limit: {}", offset, limit);

    // ... existing database query logic ...

    let response = AllResultsResponse {
        results,
        pagination,
        summary,
    };

    // Content negotiation: Check Accept header
    let accept_header = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json");

    info!("⏱️  [PERF] Serialization format: {}", accept_header);

    if accept_header.contains("application/msgpack") {
        // Return MessagePack response
        let serialization_start = std::time::Instant::now();
        let msgpack_response = MsgPack(SuccessResponse::new(response));
        info!("⏱️  [PERF] MessagePack serialization completed in {}ms", serialization_start.elapsed().as_millis());
        info!("⏱️  [PERF] get_all_results TOTAL: {}ms", start_time.elapsed().as_millis());
        Ok(msgpack_response.into_response())
    } else {
        // Return JSON response (default/fallback)
        let serialization_start = std::time::Instant::now();
        let json_response = Json(SuccessResponse::new(response));
        info!("⏱️  [PERF] JSON serialization completed in {}ms", serialization_start.elapsed().as_millis());
        info!("⏱️  [PERF] get_all_results TOTAL: {}ms", start_time.elapsed().as_millis());
        Ok(json_response.into_response())
    }
}
```

### Step 5: Return Type Update

Change return type to support both responses:

```rust
// Before:
pub async fn get_all_results(...) -> Result<impl IntoResponse, ApiError>

// After: (already correct!)
pub async fn get_all_results(...) -> Result<impl IntoResponse, ApiError>
```

The `impl IntoResponse` trait already handles both `Json<T>` and `MsgPack<T>`.

---

## Frontend Implementation

### Step 1: Install MessagePack Dependency

**File:** `frontend/package.json`

```bash
npm install @msgpack/msgpack
# or
yarn add @msgpack/msgpack
# or
pnpm add @msgpack/msgpack
```

### Step 2: Update API Client

**File:** `frontend/src/lib/api.ts`

```typescript
import { decode as msgpackDecode } from '@msgpack/msgpack'

class ApiClient {
  private baseUrl: string
  private useMsgpack: boolean = true // Feature flag

  constructor(baseUrl: string) {
    this.baseUrl = baseUrl
  }

  private async request<T>(endpoint: string, options?: RequestInit): Promise<T> {
    const url = `${this.baseUrl}${endpoint}`
    const requestStart = performance.now()
    console.log(`⏱️  [PERF] API request START: ${endpoint}`)

    try {
      const fetchStart = performance.now()

      // Add Accept header for MessagePack
      const headers: HeadersInit = {
        ...options?.headers,
      }

      if (this.useMsgpack) {
        headers['Accept'] = 'application/msgpack'
      } else {
        headers['Content-Type'] = 'application/json'
      }

      const response = await fetch(url, {
        headers,
        ...options,
      })

      const fetchEnd = performance.now()
      console.log(`⏱️  [PERF] Network fetch completed: ${endpoint} in ${(fetchEnd - fetchStart).toFixed(2)}ms`)

      if (!response.ok) {
        const errorText = await response.text()
        throw new Error(`HTTP ${response.status}: ${errorText || response.statusText}`)
      }

      const parseStart = performance.now()
      let data: any

      const contentType = response.headers.get('Content-Type')

      if (contentType?.includes('application/msgpack')) {
        // MessagePack decoding
        console.log(`⏱️  [PERF] Using MessagePack decoder`)
        const buffer = await response.arrayBuffer()
        data = msgpackDecode(new Uint8Array(buffer))
      } else {
        // JSON fallback
        console.log(`⏱️  [PERF] Using JSON parser (fallback)`)
        data = await response.json()
      }

      const parseEnd = performance.now()
      console.log(`⏱️  [PERF] Parse completed: ${endpoint} in ${(parseEnd - parseStart).toFixed(2)}ms`)

      // All API responses are wrapped in {data: {...}, timestamp: "..."}
      if (data && typeof data === 'object' && 'data' in data) {
        console.log(`✅ API ${endpoint}:`, data.data)
        const totalTime = performance.now() - requestStart
        console.log(`⏱️  [PERF] API request TOTAL: ${endpoint} in ${totalTime.toFixed(2)}ms`)
        return data.data
      }

      console.log(`⚠️ API ${endpoint} (unwrapped):`, data)
      const totalTime = performance.now() - requestStart
      console.log(`⏱️  [PERF] API request TOTAL: ${endpoint} in ${totalTime.toFixed(2)}ms`)
      return data
    } catch (error) {
      if (error instanceof TypeError && error.message.includes('fetch')) {
        console.error(`Network error for ${endpoint}:`, error)
        throw new Error(`Network error: Unable to connect to API server at ${this.baseUrl}`)
      }
      console.error(`API request failed for ${endpoint}:`, error)
      throw error
    }
  }

  // ... rest of methods remain the same ...
}
```

### Step 3: Add Feature Flag (Optional)

Create environment variable for gradual rollout:

**File:** `frontend/.env.local`

```bash
# Feature flags
NEXT_PUBLIC_USE_MSGPACK=true
```

**File:** `frontend/src/lib/api.ts`

```typescript
class ApiClient {
  private useMsgpack: boolean =
    process.env.NEXT_PUBLIC_USE_MSGPACK === 'true' ||
    typeof window !== 'undefined' &&
    localStorage.getItem('useMsgpack') === 'true'

  // ... rest of implementation ...
}
```

---

## Migration Strategy

### Phase 1: Backend Implementation (Week 1)

1. **Add dependencies** to `Cargo.toml`
2. **Create MessagePack extractor** module
3. **Update `/api/results` handler** with content negotiation
4. **Test both formats** with curl:
   ```bash
   # JSON (default)
   curl http://localhost:8080/api/results?limit=10

   # MessagePack
   curl -H "Accept: application/msgpack" \
        http://localhost:8080/api/results?limit=10 \
        --output response.msgpack
   ```
5. **Deploy to staging** environment

### Phase 2: Frontend Implementation (Week 2)

1. **Install @msgpack/msgpack** dependency
2. **Update API client** with MessagePack support
3. **Add feature flag** (default: false)
4. **Test locally** with real backend
5. **Deploy to staging** with flag enabled

### Phase 3: Gradual Rollout (Week 3-4)

1. **Enable for 10% of users** (canary deployment)
2. **Monitor performance metrics**:
   - Parse time
   - Error rates
   - User complaints
3. **Increase to 50%** if metrics good
4. **Enable for 100%** after validation
5. **Remove JSON fallback** (optional, after 30 days)

### Phase 4: Cleanup (Week 5)

1. **Remove feature flags**
2. **Remove JSON logging**
3. **Update documentation**
4. **Celebrate 80% performance improvement!**

---

## Testing Plan

### Unit Tests

#### Backend Tests

**File:** `api_server/tests/msgpack_tests.rs` (NEW FILE)

```rust
#[cfg(test)]
mod msgpack_tests {
    use super::*;
    use crate::types::*;
    use rmp_serde;

    #[test]
    fn test_serialize_all_results_response() {
        let response = AllResultsResponse {
            results: vec![
                StoredPnLResultSummary {
                    wallet_address: "test123".to_string(),
                    chain: "solana".to_string(),
                    token_address: "portfolio".to_string(),
                    token_symbol: "PORTFOLIO".to_string(),
                    total_pnl_usd: 1234.56,
                    realized_pnl_usd: 800.0,
                    unrealized_pnl_usd: 434.56,
                    roi_percentage: 123.45,
                    total_trades: 50,
                    win_rate: 0.68,
                    avg_hold_time_minutes: 120.5,
                    unique_tokens_count: Some(5),
                    active_days_count: Some(10),
                    analyzed_at: Utc::now(),
                    is_favorited: false,
                    is_archived: false,
                },
            ],
            pagination: PaginationInfo {
                total_count: 1,
                limit: 50,
                offset: 0,
                has_more: false,
            },
            summary: AllResultsSummary {
                total_wallets: 1,
                profitable_wallets: 1,
                total_pnl_usd: 1234.56,
                average_pnl_usd: 1234.56,
                total_trades: 50,
                profitability_rate: 100.0,
                last_updated: Utc::now(),
            },
        };

        // Serialize to MessagePack
        let msgpack_bytes = rmp_serde::to_vec(&response).unwrap();

        // Deserialize back
        let decoded: AllResultsResponse = rmp_serde::from_slice(&msgpack_bytes).unwrap();

        assert_eq!(decoded.results[0].wallet_address, "test123");
        assert_eq!(decoded.results[0].total_pnl_usd, 1234.56);
    }

    #[test]
    fn test_msgpack_size_vs_json() {
        let response = AllResultsResponse {
            // ... same data as above ...
        };

        let json_bytes = serde_json::to_vec(&response).unwrap();
        let msgpack_bytes = rmp_serde::to_vec(&response).unwrap();

        println!("JSON size: {} bytes", json_bytes.len());
        println!("MessagePack size: {} bytes", msgpack_bytes.len());

        let reduction_percent = ((json_bytes.len() - msgpack_bytes.len()) as f64
                                / json_bytes.len() as f64) * 100.0;

        println!("Size reduction: {:.1}%", reduction_percent);

        // Assert MessagePack is smaller
        assert!(msgpack_bytes.len() < json_bytes.len());
    }
}
```

#### Frontend Tests

**File:** `frontend/src/lib/__tests__/api-msgpack.test.ts` (NEW FILE)

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { encode as msgpackEncode } from '@msgpack/msgpack'
import { api } from '../api'

describe('MessagePack API Client', () => {
  beforeEach(() => {
    global.fetch = vi.fn()
  })

  it('should request MessagePack format with Accept header', async () => {
    const mockData = {
      data: {
        results: [
          { wallet_address: 'test123', total_pnl_usd: 1234.56 }
        ],
        pagination: { total_count: 1, limit: 50, offset: 0, has_more: false },
        summary: { total_wallets: 1, profitable_wallets: 1 }
      },
      timestamp: new Date().toISOString()
    }

    const msgpackBuffer = msgpackEncode(mockData)

    global.fetch = vi.fn().mockResolvedValue({
      ok: true,
      headers: new Headers({ 'Content-Type': 'application/msgpack' }),
      arrayBuffer: async () => msgpackBuffer.buffer
    })

    const result = await api.results.getResults({ limit: 10 })

    expect(global.fetch).toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({
        headers: expect.objectContaining({
          'Accept': 'application/msgpack'
        })
      })
    )

    expect(result.results[0].wallet_address).toBe('test123')
  })

  it('should fallback to JSON when MessagePack fails', async () => {
    const mockData = {
      data: {
        results: [
          { wallet_address: 'test456', total_pnl_usd: 567.89 }
        ]
      }
    }

    global.fetch = vi.fn().mockResolvedValue({
      ok: true,
      headers: new Headers({ 'Content-Type': 'application/json' }),
      json: async () => mockData
    })

    const result = await api.results.getResults({ limit: 10 })

    expect(result.results[0].wallet_address).toBe('test456')
  })
})
```

### Integration Tests

**File:** `api_server/tests/integration_msgpack.rs` (NEW FILE)

```rust
#[tokio::test]
async fn test_results_endpoint_msgpack() {
    let app = create_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/results?limit=10")
                .header("Accept", "application/msgpack")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let content_type = response.headers().get("Content-Type").unwrap();
    assert_eq!(content_type, "application/msgpack");

    let body_bytes = hyper::body::to_bytes(response.into_body()).await.unwrap();

    // Decode MessagePack
    let decoded: SuccessResponse<AllResultsResponse> =
        rmp_serde::from_slice(&body_bytes).unwrap();

    assert!(decoded.data.results.len() <= 10);
}
```

### Load Testing

**File:** `scripts/benchmark_msgpack.sh` (NEW FILE)

```bash
#!/bin/bash

# Benchmark MessagePack vs JSON performance

echo "=== MessagePack vs JSON Load Test ==="
echo ""

# Test JSON endpoint
echo "Testing JSON (10 requests, 5000 records each)..."
ab -n 10 -c 1 \
   -H "Accept: application/json" \
   "http://localhost:8080/api/results?limit=5000" \
   > /tmp/json_benchmark.txt

# Test MessagePack endpoint
echo "Testing MessagePack (10 requests, 5000 records each)..."
ab -n 10 -c 1 \
   -H "Accept: application/msgpack" \
   "http://localhost:8080/api/results?limit=5000" \
   > /tmp/msgpack_benchmark.txt

# Extract response times
JSON_TIME=$(grep "Time per request" /tmp/json_benchmark.txt | head -1 | awk '{print $4}')
MSGPACK_TIME=$(grep "Time per request" /tmp/msgpack_benchmark.txt | head -1 | awk '{print $4}')

echo ""
echo "Results:"
echo "  JSON:        ${JSON_TIME}ms per request"
echo "  MessagePack: ${MSGPACK_TIME}ms per request"

# Calculate improvement
python3 -c "print(f'  Improvement: {((${JSON_TIME} - ${MSGPACK_TIME}) / ${JSON_TIME} * 100):.1f}%')"
```

---

## Performance Benchmarking

### Metrics to Track

1. **Parse Time**
   - JSON: Current 97,580ms (first page)
   - MessagePack: Target <10,000ms
   - Goal: 10x improvement

2. **Payload Size**
   - JSON: Baseline 100%
   - MessagePack: Target 50-70%
   - Goal: 30-50% reduction

3. **Memory Usage**
   - JSON: Baseline 100%
   - MessagePack: Target 40-60%
   - Goal: 40-60% reduction

4. **Network Transfer Time**
   - Should improve due to smaller payload
   - Track with performance.now()

### Benchmarking Tools

1. **Browser DevTools**
   - Network tab: Transfer size
   - Performance tab: Parse time
   - Memory tab: Heap snapshots

2. **Lighthouse**
   - Total Blocking Time (TBT)
   - Largest Contentful Paint (LCP)
   - Time to Interactive (TTI)

3. **Custom Logging**
   - Already implemented in api.ts (lines 53-76)
   - Compare before/after logs

### Expected Results Table

| Metric | JSON (Current) | MessagePack (Target) | Improvement |
|--------|---------------|---------------------|-------------|
| First page parse | 97,580ms | 5,000-10,000ms | **10-20x** |
| Second page parse | 25,100ms | 2,500-5,000ms | **5-10x** |
| Payload size (25k) | ~8MB | ~4-5MB | **30-50%** |
| Memory usage | Baseline | 40-60% of baseline | **40-60%** |
| Total load time | 104,626ms | 15,000-20,000ms | **80-85%** |

---

## Rollback Strategy

### Triggers for Rollback

1. **Parse errors >1%**
2. **Performance degradation** (MessagePack slower than JSON)
3. **Memory issues** (OOM errors)
4. **User complaints** (visual bugs, data corruption)

### Rollback Steps

#### Frontend Rollback (Immediate)

```bash
# Update environment variable
NEXT_PUBLIC_USE_MSGPACK=false

# Or via localStorage for individual users
localStorage.setItem('useMsgpack', 'false')

# Redeploy frontend
npm run build
npm run deploy
```

#### Backend Rollback (If Needed)

If backend has issues:

```bash
# Revert Git commit
git revert <commit-hash>

# Rebuild and redeploy
cargo build --release
systemctl restart api-server
```

### Monitoring During Rollout

```typescript
// Add error tracking
if (contentType?.includes('application/msgpack')) {
  try {
    const buffer = await response.arrayBuffer()
    data = msgpackDecode(new Uint8Array(buffer))

    // Track success
    analytics.track('msgpack_parse_success', {
      endpoint,
      size: buffer.byteLength,
      parseTime: parseEnd - parseStart
    })
  } catch (error) {
    // Track failure and fallback to JSON
    analytics.track('msgpack_parse_error', {
      endpoint,
      error: error.message
    })

    console.error('MessagePack parse failed, falling back to JSON:', error)
    // Fallback logic...
  }
}
```

---

## Implementation Checklist

### Backend Tasks

- [ ] Add `rmp-serde = "1.1"` to `api_server/Cargo.toml`
- [ ] Create `api_server/src/msgpack_extractor.rs`
- [ ] Add `mod msgpack_extractor;` to `lib.rs` or `main.rs`
- [ ] Update `get_all_results` handler with content negotiation
- [ ] Add `headers: axum::http::HeaderMap` parameter
- [ ] Implement `if accept_header.contains("application/msgpack")`
- [ ] Return `MsgPack(response).into_response()` for MessagePack
- [ ] Return `Json(response).into_response()` for JSON fallback
- [ ] Add performance logging for both formats
- [ ] Write unit tests for MessagePack serialization
- [ ] Write integration tests for endpoint
- [ ] Test with curl (JSON and MessagePack)
- [ ] Deploy to staging environment

### Frontend Tasks

- [ ] Run `npm install @msgpack/msgpack`
- [ ] Import `{ decode as msgpackDecode }` in `api.ts`
- [ ] Add `Accept: application/msgpack` header to fetch
- [ ] Check `Content-Type` response header
- [ ] Implement MessagePack decoding path
- [ ] Implement JSON fallback path
- [ ] Add feature flag support (`NEXT_PUBLIC_USE_MSGPACK`)
- [ ] Add error tracking for MessagePack failures
- [ ] Write unit tests for MessagePack client
- [ ] Test locally with real backend
- [ ] Deploy to staging with flag disabled
- [ ] Enable flag for 10% of users (canary)
- [ ] Monitor metrics and errors
- [ ] Increase to 100% if successful

### Testing Tasks

- [ ] Create `msgpack_tests.rs` with unit tests
- [ ] Create `api-msgpack.test.ts` with frontend tests
- [ ] Create `integration_msgpack.rs` with integration tests
- [ ] Create `benchmark_msgpack.sh` load test script
- [ ] Run all tests and verify they pass
- [ ] Benchmark JSON vs MessagePack performance
- [ ] Document performance improvement metrics
- [ ] Load test with 25k+ records
- [ ] Memory profiling (heap snapshots)
- [ ] Browser compatibility testing (Chrome, Firefox, Safari)

### Deployment Tasks

- [ ] Create deployment plan document
- [ ] Set up staging environment
- [ ] Deploy backend to staging
- [ ] Deploy frontend to staging
- [ ] Run smoke tests on staging
- [ ] Deploy backend to production
- [ ] Deploy frontend to production (flag disabled)
- [ ] Enable feature flag for 10% users
- [ ] Monitor for 24 hours
- [ ] Increase to 50% users
- [ ] Monitor for 48 hours
- [ ] Enable for 100% users
- [ ] Monitor for 1 week
- [ ] Remove JSON fallback (optional)
- [ ] Update documentation

### Cleanup Tasks

- [ ] Remove feature flags after 30 days
- [ ] Remove JSON-specific logging
- [ ] Update API documentation
- [ ] Update README with MessagePack info
- [ ] Archive this implementation plan
- [ ] Celebrate performance win!

---

## Appendix

### MessagePack Format Specification

MessagePack uses type tags to encode data:

```
Positive FixInt:  0x00 - 0x7f
FixMap:           0x80 - 0x8f
FixArray:         0x90 - 0x9f
FixStr:           0xa0 - 0xbf
nil:              0xc0
false:            0xc2
true:             0xc3
float 32:         0xca
float 64:         0xcb
uint 8:           0xcc
uint 16:          0xcd
uint 32:          0xce
uint 64:          0xcf
int 8:            0xd0
int 16:           0xd1
int 32:           0xd2
int 64:           0xd3
```

### Resource Links

- **MessagePack Official**: https://msgpack.org/
- **rmp-serde Docs**: https://docs.rs/rmp-serde/
- **@msgpack/msgpack Docs**: https://github.com/msgpack/msgpack-javascript
- **Axum IntoResponse**: https://docs.rs/axum/latest/axum/response/trait.IntoResponse.html

### Performance Comparison Studies

1. **msgpack-javascript benchmarks**: https://github.com/msgpack/msgpack-javascript#benchmarks
2. **Binary formats comparison**: https://github.com/eishay/jvm-serializers/wiki

---

**End of Implementation Plan**

_This document should be updated as implementation progresses. Track changes in Git._
