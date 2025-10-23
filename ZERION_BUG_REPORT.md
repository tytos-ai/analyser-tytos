# Zerion API Bug Report: 500 Errors on High Page Numbers

## Issue Summary

We are experiencing **HTTP 500 Internal Server Error** responses from the Zerion API when fetching wallet transactions via pagination. The errors occur inconsistently, typically on **high page numbers (e.g., page 300+)** during continuous pagination requests.

## Environment

- **API Endpoint**: `https://api.zerion.io/v1/wallets/{wallet_address}/transactions/`
- **Client**: Rust application using `reqwest` HTTP client library
- **Authentication**: HTTP Basic Authentication
- **API Key**: `zk_prod_b0bbb7857c74422582eb39d50f970006`

## Reproduction Details

### Affected Wallet Address
```
HytEnZY8kd4cZqeVfintmBnZ2VfqQdfinbzZ2ot6mRNZ
```

This wallet consistently triggers 500 errors during pagination at high page numbers.

### Complete HTTP Request

**Method**: `GET`

**URL Pattern**:
```
https://api.zerion.io/v1/wallets/HytEnZY8kd4cZqeVfintmBnZ2VfqQdfinbzZ2ot6mRNZ/transactions/?currency=usd&page[size]=100&filter[chain_ids]=solana&filter[trash]=only_non_trash&filter[operation_types]=trade,send,receive
```

**Headers**:
```http
Authorization: Basic emtfcHJvZF9iMGJiYjc4NTdjNzQ0MjI1ODJlYjM5ZDUwZjk3MDAwNjo=
Accept: */*
Accept-Encoding: gzip, deflate, br
Connection: keep-alive
```

**Authentication Details**:
- Type: HTTP Basic Authentication
- API Key: `zk_prod_b0bbb7857c74422582eb39d50f970006`
- Format: `{api_key}:` (note the trailing colon)
- Base64 Encoded: `emtfcHJvZF9iMGJiYjc4NTdjNzQ0MjI1ODJlYjM5ZDUwZjk3MDAwNjo=`

**Query Parameters**:
| Parameter | Value | Description |
|-----------|-------|-------------|
| `currency` | `usd` | Price denomination |
| `page[size]` | `100` | Transactions per page |
| `filter[chain_ids]` | `solana` | Blockchain network |
| `filter[trash]` | `only_non_trash` | Exclude spam tokens |
| `filter[operation_types]` | `trade,send,receive` | Transaction types |

**Request Timeout**: 120 seconds

### Pagination Behavior

Our application follows this pagination pattern:

1. **First Request**: Use the URL pattern above
2. **Subsequent Requests**: Use the exact `links.next` URL from the previous response
3. **Termination**: Continue until `links.next` is `null` or an error occurs

### Example cURL Command

```bash
curl -v -X GET 'https://api.zerion.io/v1/wallets/HytEnZY8kd4cZqeVfintmBnZ2VfqQdfinbzZ2ot6mRNZ/transactions/?currency=usd&page[size]=100&filter[chain_ids]=solana&filter[trash]=only_non_trash&filter[operation_types]=trade,send,receive' \
  -H 'Authorization: Basic emtfcHJvZF9iMGJiYjc4NTdjNzQ0MjI1ODJlYjM5ZDUwZjky3MDAwNjo=' \
  -H 'Accept: */*' \
  -H 'Accept-Encoding: gzip, deflate, br' \
  -H 'Connection: keep-alive' \
  --max-time 120
```

### Alternative cURL (Using -u flag)

```bash
curl -v -X GET 'https://api.zerion.io/v1/wallets/HytEnZY8kd4cZqeVfintmBnZ2VfqQdfinbzZ2ot6mRNZ/transactions/?currency=usd&page[size]=100&filter[chain_ids]=solana&filter[trash]=only_non_trash&filter[operation_types]=trade,send,receive' \
  -u 'zk_prod_b0bbb7857c74422582eb39d50f970006:' \
  --max-time 120
```

## Error Characteristics

### When Errors Occur
- **Page Range**: Typically page 200-400+ in pagination sequence
- **Frequency**: Intermittent but reproducible with the same wallet
- **Pattern**: May succeed for first N pages, then fail on subsequent pages

### Expected Behavior
- Pagination should continue until all transactions are fetched
- Each page should return 200 OK with valid JSON response
- The `links.next` field should guide pagination until no more transactions exist

### Actual Behavior
- API returns **HTTP 500 Internal Server Error**
- Pagination stops prematurely
- No error details in response body (or generic server error message)

### Impact
- Cannot fetch complete transaction history for wallets with large transaction counts
- Production analysis pipeline fails for active trader wallets
- Data incompleteness affects P&L calculations

## Reproduction Scripts

We have provided two scripts that exactly replicate our application's behavior:

### Bash Script (Requires: bash, curl, jq)
```bash
./zerion_bug_reproduction.sh
```

### Python Script (Requires: python3, requests library)
```bash
python3 zerion_bug_reproduction.py
```

### What the Scripts Do

Both scripts:
1. Start fetching transactions from page 1
2. Follow `links.next` for pagination (matching our Rust application)
3. Log each page number, transaction count, response time
4. Save all responses to `zerion_bug_logs_{timestamp}/` directory
5. **Stop and save detailed error info when 500 error occurs**
6. Show exactly which page number causes the failure

### Expected Output

**Success Case** (if bug doesn't reproduce):
```
Page 1: Fetched 100 transactions in 450ms, has_next: true
Page 2: Fetched 100 transactions in 380ms, has_next: true
...
Page N: Fetched 45 transactions in 420ms, has_next: false
PAGINATION SUMMARY: Total Pages: N, Total Transactions: X
```

**Failure Case** (when bug reproduces):
```
Page 1: Fetched 100 transactions in 450ms, has_next: true
Page 2: Fetched 100 transactions in 380ms, has_next: true
...
Page 312: Fetched 100 transactions in 520ms, has_next: true
=========================================
ERROR DETECTED ON PAGE 313
=========================================
HTTP Status Code: 500
Page Number: 313
Response Time: 1850ms
```

All request/response data is saved to timestamped log directories for your analysis.

## Additional Context

### Other Wallets Tested

The issue has been observed with multiple wallets but is most reproducible with:
- `HytEnZY8kd4cZqeVfintmBnZ2VfqQdfinbzZ2ot6mRNZ` (Solana)

### Chain-Specific Behavior

We primarily test with:
- **Solana**: Most affected
- **Ethereum**: Less frequent but similar issues observed
- **Base**: Limited testing
- **Binance Smart Chain**: Limited testing

### Multi-Chain Requests

We also test with comma-separated chain filters:
```
filter[chain_ids]=solana,ethereum,base,binance-smart-chain
```

This might exacerbate the issue.

### Client Implementation Notes

Our Rust application uses:
- **HTTP Client**: `reqwest 0.11.x` (Tokio-based async runtime)
- **Timeout**: 120 seconds per request
- **Rate Limiting**: Currently NOT enforced (config has 200ms but code doesn't implement it)
- **Retry Logic**: None (fails immediately on 500 error)
- **Compression**: Accepts gzip, deflate, br (standard reqwest behavior)

### Time Range Filters

We also use time-based filtering in some requests:
```
&filter[min_mined_at]={unix_timestamp_ms}&filter[max_mined_at]={unix_timestamp_ms}
```

Example:
```
filter[min_mined_at]=1736467200000&filter[max_mined_at]=1737072000000
```

The issue occurs with and without time filters.

## Hypotheses

Potential causes we've considered:

1. **Database timeout on backend** when fetching deep pages
2. **Pagination cursor corruption** in `links.next` URLs
3. **Server-side resource limits** exceeded at high page numbers
4. **Query complexity** increasing with page offset
5. **Caching issues** for wallet transaction history
6. **Race conditions** in backend transaction indexing

## Request for Investigation

Could you please investigate:

1. **Server logs** for wallet `HytEnZY8kd4cZqeVfintmBnZ2VfqQdfinbzZ2ot6mRNZ` around pagination failures
2. **Database query performance** at high page offsets
3. **Pagination cursor validation** in your backend
4. **Resource limits** that might trigger 500 errors
5. **Any known issues** with large transaction history wallets

## Contact Information

- **Project**: Solana Wallet P&L Analysis System
- **Implementation**: Rust (production), providing Python/Bash scripts for reproduction
- **API Key**: `zk_prod_b0bbb7857c74422582eb39d50f970006`

## Attachments

1. `zerion_bug_reproduction.sh` - Bash reproduction script
2. `zerion_bug_reproduction.py` - Python reproduction script

Both scripts will generate a `zerion_bug_logs_{timestamp}/` directory containing:
- All API responses (successful pages + error response)
- Summary log with timestamps and performance metrics
- Error log with detailed failure information

---

**Generated**: 2025-10-21
**Severity**: High (blocking production use for high-activity wallets)
**Priority**: Please investigate at your earliest convenience
