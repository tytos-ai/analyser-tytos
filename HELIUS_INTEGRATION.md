# Helius Integration Documentation

## Overview

The Rust wallet analyzer now supports Helius as an alternative transaction data source alongside the existing BirdEye integration. This implementation provides users with flexible data source options, improved reliability through fallback mechanisms, and enhanced transaction data coverage.

## Key Features

### 🔄 **Dual Data Source Support**
- **BirdEye**: Primary data source with comprehensive trader analytics
- **Helius**: Alternative data source using Enhanced Transactions API
- **Both**: Automatic fallback from primary to secondary source

### 🏗️ **Seamless Integration**
- Zero breaking changes to existing P&L algorithms
- Transparent data source switching
- Consistent API interfaces across data sources

### 📊 **Enhanced Data Processing**
- Token metadata enrichment via BirdEye API
- Automatic conversion between data formats
- Comprehensive transaction parsing and validation

## Architecture

### Data Flow

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   Data Source   │    │   Transformation │    │   P&L Engine    │
│   Selection     │ -> │     Layer        │ -> │   (Existing)    │
└─────────────────┘    └──────────────────┘    └─────────────────┘
         │                        │                       │
    ┌────┴────┐              ┌────┴────┐             ┌────┴────┐
    │ BirdEye │              │ Helius  │             │ New P&L │
    │ Helius  │              │ Adapter │             │Algorithm│
    │ Both    │              │ Layer   │             │         │
    └─────────┘              └─────────┘             └─────────┘
```

### Component Architecture

#### 1. **HeliusClient** (`dex_client/src/helius_client.rs`)
- **Purpose**: Direct interface to Helius Enhanced Transactions API
- **Key Methods**:
  - `fetch_wallet_transactions()`: Retrieve raw transaction data
  - `helius_to_financial_events()`: Convert to FinancialEvents format
  - `helius_to_general_trader_transactions()`: Convert to BirdEye-compatible format

#### 2. **TokenMetadataService** (`dex_client/src/token_metadata_service.rs`)
- **Purpose**: Fetch token metadata from BirdEye API
- **Features**:
  - Batch and single token metadata requests
  - In-memory caching for performance
  - Fallback mechanisms for failed requests

#### 3. **DataSource Configuration** (`config_manager/src/lib.rs`)
- **Purpose**: Flexible data source configuration
- **Options**:
  - `BirdEye`: Use BirdEye exclusively
  - `Helius`: Use Helius exclusively  
  - `Both { primary, fallback }`: Use primary with fallback

#### 4. **Job Orchestrator Updates** (`job_orchestrator/src/lib.rs`)
- **Purpose**: Route requests to appropriate data source
- **Features**:
  - Automatic data source selection
  - Fallback handling
  - Consistent P&L calculation pipeline

## Configuration

### Environment Variables

```bash
# Helius Configuration
PNL_HELIUS_API_KEY=your_helius_api_key_here
PNL_HELIUS_ENABLED=true
PNL_HELIUS_API_BASE_URL=https://api.helius.xyz/v0
PNL_HELIUS_REQUEST_TIMEOUT_SECONDS=30
PNL_HELIUS_RATE_LIMIT_MS=100
PNL_HELIUS_MAX_RETRY_ATTEMPTS=3

# Data Source Selection
PNL_DATA_SOURCE=BirdEye  # Options: BirdEye, Helius, Both
```

### TOML Configuration

```toml
[data_source]
# Simple configuration
data_source = "BirdEye"  # or "Helius"

# Advanced configuration with fallback
[data_source.both]
primary = "BirdEye"
fallback = "Helius"

[helius]
api_key = "your_helius_api_key"
api_base_url = "https://api.helius.xyz/v0"
request_timeout_seconds = 30
rate_limit_ms = 100
max_retry_attempts = 3
enabled = true

[birdeye]
api_key = "your_birdeye_api_key"
api_base_url = "https://public-api.birdeye.so"
request_timeout_seconds = 30
# ... other birdeye config
```

## API Usage

### Rust Code Examples

#### Basic Usage

```rust
use config_manager::{SystemConfig, DataSource};
use dex_client::HeliusClient;
use job_orchestrator::JobOrchestrator;

// Load configuration
let config = SystemConfig::load()?;

// Create orchestrator (automatically handles data source selection)
let orchestrator = JobOrchestrator::new(config).await?;

// Analyze wallet P&L (uses configured data source)
let report = orchestrator.process_single_wallet(
    "wallet_address_here",
    PnLFilters::default()
).await?;
```

#### Direct Helius Usage

```rust
use dex_client::{HeliusClient, TokenMetadataService};
use config_manager::HeliusConfig;

// Create Helius client
let helius_config = HeliusConfig {
    api_key: "your_api_key".to_string(),
    api_base_url: "https://api.helius.xyz/v0".to_string(),
    request_timeout_seconds: 30,
    rate_limit_ms: 100,
    max_retry_attempts: 3,
    enabled: true,
};

let client = HeliusClient::new(helius_config)?;

// Fetch transactions
let transactions = client.fetch_wallet_transactions(
    "wallet_address",
    Some(1000), // transaction count
    None,       // timeframe
).await?;

// Convert to GeneralTraderTransaction format for P&L analysis
let general_transactions = client.helius_to_general_trader_transactions(
    &transactions,
    "wallet_address"
).await?;
```

#### Data Source Configuration

```rust
use config_manager::DataSource;

// Simple configurations
let birdeye_only = DataSource::BirdEye;
let helius_only = DataSource::Helius;

// Advanced configuration with fallback
let both_with_fallback = DataSource::Both {
    primary: Box::new(DataSource::BirdEye),
    fallback: Box::new(DataSource::Helius),
};

// Validate configuration
both_with_fallback.validate()?;

// Check what data sources are used
assert!(both_with_fallback.uses_birdeye());
assert!(both_with_fallback.uses_helius());
```

## Data Transformation

### Helius → FinancialEvents

The Helius integration converts raw Helius Enhanced Transactions into `FinancialEvent` structures:

```rust
pub struct FinancialEvent {
    pub id: Uuid,
    pub transaction_id: String,
    pub wallet_address: String,
    pub event_type: EventType,      // Buy/Sell
    pub token_mint: String,
    pub token_amount: Decimal,
    pub sol_amount: Decimal,
    pub usd_value: Decimal,
    pub timestamp: DateTime<Utc>,
    pub transaction_fee: Decimal,
    pub metadata: EventMetadata,
}
```

### Helius → GeneralTraderTransaction

For compatibility with existing P&L algorithms, Helius data is also converted to the `GeneralTraderTransaction` format:

```rust
pub struct GeneralTraderTransaction {
    pub quote: TokenTransactionSide,    // Token being sold
    pub base: TokenTransactionSide,     // Token being bought
    pub tx_hash: String,
    pub source: String,                 // DEX source (Jupiter, Orca, etc.)
    pub block_unix_time: i64,
    pub owner: String,                  // Wallet address
    // ... other fields
}
```

## Token Metadata Enrichment

### BirdEye Integration

Helius transactions are enriched with token metadata from BirdEye API:

```rust
pub struct TokenMetadata {
    pub address: String,
    pub symbol: String,           // e.g., "USDC", "SOL"
    pub name: String,             // e.g., "USD Coin", "Solana"
    pub decimals: u8,
    pub logo_uri: Option<String>,
    pub extensions: Option<TokenExtensions>,
}
```

### Caching Strategy

- **In-memory caching**: Frequently accessed token metadata is cached
- **Batch requests**: Multiple tokens fetched in single API call
- **Fallback handling**: Graceful degradation when metadata unavailable

## Error Handling

### Comprehensive Error Types

```rust
#[derive(Error, Debug)]
pub enum HeliusError {
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),
    
    #[error("JSON parsing failed: {0}")]
    JsonParsingFailed(#[from] serde_json::Error),
    
    #[error("API error: {status} - {message}")]
    ApiError { status: u16, message: String },
    
    #[error("Rate limit exceeded, retry after: {retry_after_ms}ms")]
    RateLimitExceeded { retry_after_ms: u64 },
    
    #[error("Token metadata error: {0}")]
    TokenMetadata(#[from] TokenMetadataError),
    
    // ... other error types
}
```

### Fallback Mechanisms

1. **Data Source Fallback**: Automatic switch from primary to fallback data source
2. **Token Metadata Fallback**: Use mint address when metadata unavailable
3. **Retry Logic**: Exponential backoff for transient failures
4. **Rate Limit Handling**: Intelligent waiting and retry strategies

## Performance Optimizations

### Batch Processing

- **Token Metadata**: Fetch up to 50 tokens per API call
- **Transaction Pagination**: Efficient handling of large transaction sets
- **Concurrent Processing**: Parallel processing where possible

### Caching

- **Token Metadata**: In-memory cache with configurable TTL
- **Price Data**: Redis-based caching for historical prices
- **Request Deduplication**: Avoid duplicate API calls

### Rate Limiting

- **Configurable Delays**: Adjustable rate limiting between requests
- **Exponential Backoff**: Intelligent retry strategies
- **Circuit Breaker**: Fail-fast for persistent errors

## Testing

### Comprehensive Test Coverage

The implementation includes extensive testing:

```bash
# Run all tests
cargo test

# Run specific module tests
cargo test -p dex_client
cargo test -p config_manager
cargo test -p job_orchestrator

# Test results: 24/24 passing
```

### Key Test Areas

1. **Helius Transaction Parsing**: Validation of JSON deserialization
2. **Token Balance Extraction**: Correct identification of buy/sell operations
3. **Data Source Configuration**: Validation of configuration logic
4. **Error Handling**: Comprehensive error scenario testing
5. **Integration Testing**: End-to-end data flow validation

## Migration Guide

### For Existing Users

**No changes required** - existing configurations continue to work unchanged.

### For New Features

1. **Add Helius API Key** (if using Helius):
   ```bash
   export PNL_HELIUS_API_KEY=your_helius_api_key
   ```

2. **Configure Data Source** (optional):
   ```bash
   export PNL_DATA_SOURCE=Both  # or BirdEye, Helius
   ```

3. **Update Configuration File** (optional):
   ```toml
   [data_source]
   data_source = "Both"
   
   [helius]
   api_key = "your_helius_api_key"
   enabled = true
   ```

## Benefits

### 🚀 **Improved Reliability**
- Automatic fallback between data sources
- Reduced single point of failure
- Better uptime for P&L analysis

### 📈 **Enhanced Data Coverage**
- Access to Helius Enhanced Transactions API
- Comprehensive transaction parsing
- Better support for complex DeFi transactions

### 🛠️ **Flexible Configuration**
- Choose optimal data source for your use case
- Easy switching between providers
- Cost optimization through source selection

### 🔧 **Developer Experience**
- Consistent API interfaces
- Comprehensive error handling
- Extensive test coverage
- Clear documentation and examples

## Troubleshooting

### Common Issues

#### 1. **Missing API Keys**
```
Error: BirdEye API key is required when BirdEye is used as a data source
```
**Solution**: Set the appropriate API key in environment variables or config file.

#### 2. **Rate Limit Exceeded**
```
Error: Rate limit exceeded, retry after: 1000ms
```
**Solution**: The system automatically handles rate limits with exponential backoff.

#### 3. **Data Source Configuration**
```
Error: Primary and fallback data sources cannot be the same
```
**Solution**: Use different data sources for primary and fallback in `Both` configuration.

### Debug Mode

Enable debug logging to troubleshoot issues:

```bash
export RUST_LOG=debug
export PNL_SYSTEM_DEBUG_MODE=true
```

## Roadmap

### Future Enhancements

1. **Additional Data Sources**: Support for other blockchain data providers
2. **Real-time Streaming**: WebSocket support for live transaction monitoring
3. **Advanced Caching**: Redis-based distributed caching
4. **Analytics Dashboard**: Web interface for data source management
5. **Custom Filters**: Advanced transaction filtering capabilities

### Performance Improvements

1. **Connection Pooling**: Reuse HTTP connections for better performance
2. **Parallel Processing**: Concurrent transaction processing
3. **Streaming JSON**: Handle large responses more efficiently
4. **Compression**: Reduce network bandwidth usage

## Support

For issues, questions, or feature requests:

1. **GitHub Issues**: Create detailed bug reports or feature requests
2. **Documentation**: Refer to inline code documentation
3. **Tests**: Check test files for usage examples
4. **Configuration**: Review configuration validation messages

## Contributing

When contributing to the Helius integration:

1. **Follow existing patterns**: Maintain consistency with BirdEye implementation
2. **Add tests**: Ensure comprehensive test coverage
3. **Update documentation**: Keep this document current
4. **Error handling**: Follow established error handling patterns
5. **Performance**: Consider caching and rate limiting implications

---

*This documentation covers the Helius integration features as implemented in the Rust wallet analyzer. For the latest updates and detailed API documentation, refer to the inline code documentation and test files.*