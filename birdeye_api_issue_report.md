# BirdEye API Issue Report: Inconsistent ui_amount_mode Behavior

## Issue Summary
The `/v1/wallet/tx_list` endpoint exhibits inconsistent behavior with the `ui_amount_mode` parameter. Some wallet addresses return transaction data only with `ui_amount_mode=scaled`, while others only work with `ui_amount_mode=raw`. This inconsistency requires API consumers to implement fallback logic to handle both cases.

## Affected Endpoint
```
GET https://public-api.birdeye.so/v1/wallet/tx_list
```

## Issue Details

### Expected Behavior
The API should return consistent transaction data for valid wallet addresses regardless of the `ui_amount_mode` parameter value, or at least document which wallets require which mode.

### Actual Behavior
- Some wallets return empty results with one mode but valid data with the other
- No error indication when the wrong mode is used - just an empty successful response
- No documentation about this behavior or how to determine which mode to use

## Reproduction Examples

### Example 1: Wallet that ONLY works with `ui_amount_mode=raw`

**Wallet Address:** `GJwTJ16cU41yUFHxaTCxB2uUAZW6me2yQRd8ZgF3t6GK`

**Request with `scaled` (returns empty):**
```bash
curl --request GET \
     --url 'https://public-api.birdeye.so/v1/wallet/tx_list?wallet=GJwTJ16cU41yUFHxaTCxB2uUAZW6me2yQRd8ZgF3t6GK&limit=100&ui_amount_mode=scaled' \
     --header 'X-API-KEY: [YOUR_API_KEY]' \
     --header 'accept: application/json' \
     --header 'x-chain: solana'
```

**Response (200 OK):**
```json
{
  "success": true,
  "data": {
    "solana": []
  }
}
```

**Request with `raw` (returns data):**
```bash
curl --request GET \
     --url 'https://public-api.birdeye.so/v1/wallet/tx_list?wallet=GJwTJ16cU41yUFHxaTCxB2uUAZW6me2yQRd8ZgF3t6GK&limit=100&ui_amount_mode=raw' \
     --header 'X-API-KEY: [YOUR_API_KEY]' \
     --header 'accept: application/json' \
     --header 'x-chain: solana'
```

**Response (200 OK):**
```json
{
  "success": true,
  "data": {
    "solana": [
      // ... transaction data returned ...
    ]
  }
}
```

### Example 2: Wallet that works with `ui_amount_mode=scaled`

**Wallet Address:** `5ngDQ3vMT7GHeFvXP5GLX79t6fqG2t15kyQTtst9zFdw`

This wallet successfully returns 50 transactions when using `ui_amount_mode=scaled`:

**Request:**
```bash
curl --request GET \
     --url 'https://public-api.birdeye.so/v1/wallet/tx_list?wallet=5ngDQ3vMT7GHeFvXP5GLX79t6fqG2t15kyQTtst9zFdw&limit=100&ui_amount_mode=scaled' \
     --header 'X-API-KEY: [YOUR_API_KEY]' \
     --header 'accept: application/json' \
     --header 'x-chain: solana'
```

**Response (200 OK):**
```json
{
  "success": true,
  "data": {
    "solana": [
      // ... 50 transaction records returned ...
    ]
  }
}
```

## Impact

1. **API Reliability:** Developers cannot reliably fetch transaction data without implementing complex fallback logic
2. **Performance:** Requires potentially double the API calls (trying both modes) for affected wallets
3. **User Experience:** Silent failures (empty successful responses) make debugging difficult
4. **Documentation Gap:** No guidance on when to use which mode or why this behavior exists

## Suggested Solutions

1. **Option 1 (Preferred):** Make the API internally handle both modes and return the appropriate data regardless of the `ui_amount_mode` parameter

2. **Option 2:** Include a field in the response indicating the required mode when returning empty results:
   ```json
   {
     "success": true,
     "data": {
       "solana": []
     },
     "metadata": {
       "suggested_ui_amount_mode": "raw",
       "reason": "wallet_requires_raw_mode"
     }
   }
   ```

3. **Option 3:** Document this behavior clearly and provide an endpoint to check which mode a wallet requires

## Workaround Implementation

We've had to implement the following fallback logic to handle this inconsistency:

```rust
// Try 'scaled' mode first
let result = fetch_with_mode("scaled");
if result.is_empty() {
    // Fallback to 'raw' mode
    let fallback_result = fetch_with_mode("raw");
    return fallback_result;
}
```

## Environment Details
- API Version: v1
- Endpoint: `/v1/wallet/tx_list`
- Chain: Solana
- Date Observed: September 12, 2025
- Consistent Reproduction: Yes

## Additional Notes

- Both wallet addresses are valid Solana addresses with transaction history
- The issue is consistent and reproducible
- HTTP status is always 200 OK regardless of whether data is returned
- The `success` field is always `true` even when no data is returned due to wrong mode

## Request

Please investigate why certain wallets require specific `ui_amount_mode` values and either:
1. Fix the API to handle both modes transparently
2. Provide clear documentation on this behavior
3. Add response metadata to indicate the correct mode to use

This would greatly improve the developer experience and reliability of integrations with the BirdEye API.