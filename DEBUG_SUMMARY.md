# DexScreener API Schema Debug Summary

## Problem Identified

The error "missing field `label` at line 1 column 1091" was occurring when trying to parse DexScreener API responses because the data structures in `dex_client/src/types.rs` didn't match the actual API response format.

## Root Cause Analysis

### 1. API Endpoints Tested
- `https://api.dexscreener.com/token-boosts/latest/v1?chainId=solana`
- `https://api.dexscreener.com/token-boosts/top/v1?chainId=solana`

### 2. Actual vs Expected Schema Issues

#### Missing Field: `openGraph`
The API responses include an `openGraph` field that wasn't defined in our `BoostedToken` struct.

**API Response Sample:**
```json
{
  "url": "...",
  "chainId": "solana",
  "tokenAddress": "...",
  "icon": "...",
  "header": "...",
  "openGraph": "https://cdn.dexscreener.com/token-images/og/solana/...",
  "description": "...",
  "links": [...],
  "totalAmount": 50
}
```

#### Inconsistent Link Structure
The `TokenLink` struct expected both `type` and `label` fields to be required, but the API uses them inconsistently:

**Type 1 - Social media links:**
```json
{
  "type": "twitter",
  "url": "https://x.com/example"
}
```

**Type 2 - Website links:**
```json
{
  "label": "Website", 
  "url": "https://example.com"
}
```

**Type 3 - Mixed:**
```json
{
  "label": "Website",
  "url": "https://meshclub.xyz"
},
{
  "type": "twitter", 
  "url": "https://x.com/MeshClubs"
}
```

## Fixes Applied

### 1. Updated `BoostedToken` struct
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoostedToken {
    pub url: String,
    #[serde(rename = "chainId")]
    pub chain_id: String,
    #[serde(rename = "tokenAddress")]
    pub token_address: String,
    pub icon: Option<String>,
    pub header: Option<String>,
    #[serde(rename = "openGraph")]     // ✅ ADDED
    pub open_graph: Option<String>,    // ✅ ADDED
    pub description: Option<String>,
    pub links: Option<Vec<TokenLink>>,
    pub amount: Option<u64>,
    #[serde(rename = "totalAmount")]
    pub total_amount: Option<u64>,
}
```

### 2. Updated `TokenLink` struct
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenLink {
    #[serde(rename = "type")]
    pub link_type: Option<String>,     // ✅ CHANGED: Required -> Optional
    pub label: Option<String>,         // ✅ CHANGED: Required -> Optional  
    pub url: String,
}
```

### 3. Fixed Test Compilation Issues
Fixed missing `trending_client` field in test constructors in `dex_client/src/lib.rs`.

## Verification

Created comprehensive tests that successfully parse both API endpoints:

```bash
cargo test -p dex_client --test api_parsing_test -- --nocapture
```

**Results:**
- ✅ Latest API: Successfully parsed 30 tokens
- ✅ Top API: Successfully parsed 30 tokens  
- ✅ Both `type` and `label` link variations handled correctly
- ✅ All fields including `openGraph` parsed successfully

## API Response Patterns Observed

### Field Presence
- `openGraph`: Always present
- `amount`: Only in "latest" API responses
- `links`: Present but can be empty array
- `description`: Usually present but can be null

### Link Types Observed
- `type: "twitter"` - Twitter/X links
- `type: "telegram"` - Telegram links  
- `label: "Website"` - Website links
- Some tokens have mixed link types in the same array

## Recommendation

The schema fixes are backward compatible and handle all observed variations in the DexScreener API responses. The optional fields approach provides flexibility for future API changes while maintaining parsing reliability.