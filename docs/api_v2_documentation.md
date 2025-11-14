# API v2 Documentation - Enhanced Copy Trading Analysis

## 🚀 Overview

API v2 provides enhanced P&L analysis specifically designed for copy trading evaluation. It exposes rich data from the new P&L engine without legacy conversion, offering comprehensive insights into trading patterns, risk metrics, and performance indicators.

### Key Improvements Over v1
- **Direct P&L Engine Access**: No legacy conversion, preserving all FIFO matching details
- **Copy Trading Metrics**: Advanced analytics for trading style classification and risk assessment
- **Enhanced Performance**: ~1-2 second response times for comprehensive analysis
- **Quality Scoring**: Automated analysis quality assessment (0-100 scale)
- **Rich Trade Data**: Individual trade details with performance categorization

## 🔗 Base URL

```
Production: http://134.199.211.155:8080
Local Dev:  http://localhost:8080
```

## 📋 Quick Start

### Basic Wallet Analysis
```bash
curl "http://localhost:8080/api/v2/wallets/5ngDQ3vMT7GHeFvXP5GLX79t6fqG2t15kyQTtst9zFdw/analysis"
```

### Key Response Fields
```json
{
  "data": {
    "wallet_address": "5ngDQ3vMT7GHeFvXP5GLX79t6fqG2t15kyQTtst9zFdw",
    "portfolio_result": {
      "total_pnl_usd": "5361.34",
      "tokens_analyzed": 2,
      "events_processed": 18
    },
    "copy_trading_metrics": {
      "trading_style": {"Scalper": {"avg_hold_minutes": "0"}},
      "consistency_score": "0"
    },
    "metadata": {
      "analysis_duration_ms": 1272,
      "quality_score": "65"
    }
  }
}
```

## 🎯 Core Endpoints

### 1. Comprehensive Wallet Analysis
**`GET /api/v2/wallets/:wallet_address/analysis`**

The primary endpoint for complete wallet P&L analysis with copy trading metrics.

#### Query Parameters
- `max_transactions` (optional): Maximum transactions to analyze (default: 500)
- `time_range` (optional): Time period to analyze (e.g., "7d", "1h", "30d"). When combined with `max_transactions`, uses Hybrid Mode
- `include_copy_metrics` (optional): Include copy trading analysis (default: true)

#### Example Request
```bash
curl "http://localhost:8080/api/v2/wallets/5ngDQ3vMT7GHeFvXP5GLX79t6fqG2t15kyQTtst9zFdw/analysis?max_transactions=100&include_copy_metrics=true"
```

#### Response Structure
```json
{
  "data": {
    "wallet_address": "string",
    "portfolio_result": {
      "wallet_address": "string",
      "token_results": [
        {
          "token_address": "string",
          "token_symbol": "string",
          "matched_trades": [],
          "unmatched_sells": [],
          "remaining_position": {
            "token_address": "string",
            "token_symbol": "string",
            "quantity": "decimal",
            "avg_cost_basis_usd": "decimal",
            "total_cost_basis_usd": "decimal"
          },
          "total_realized_pnl_usd": "decimal",
          "total_unrealized_pnl_usd": "decimal",
          "total_pnl_usd": "decimal",
          "total_trades": 0,
          "winning_trades": 0,
          "losing_trades": 0,
          "win_rate_percentage": "decimal",
          "avg_hold_time_minutes": "decimal"
        }
      ],
      "total_realized_pnl_usd": "decimal",
      "total_unrealized_pnl_usd": "decimal",
      "total_pnl_usd": "decimal",
      "total_trades": 0,
      "overall_win_rate_percentage": "decimal",
      "avg_hold_time_minutes": "decimal",
      "tokens_analyzed": 2,
      "events_processed": 18,
      "analysis_timestamp": "2025-07-15T18:26:45.840506574Z"
    },
    "copy_trading_metrics": {
      "trading_style": {
        "Scalper": {"avg_hold_minutes": "decimal"},
        "SwingTrader": {"avg_hold_hours": "decimal"},
        "LongTerm": {"avg_hold_days": "decimal"},
        "Mixed": {"predominant_style": "TradingStyle"}
      },
      "consistency_score": "decimal",
      "risk_metrics": {
        "max_position_percentage": "decimal",
        "diversification_score": "decimal",
        "max_consecutive_losses": 0,
        "avg_loss_per_trade": "decimal",
        "max_win_streak": 0,
        "risk_adjusted_return": "decimal"
      },
      "position_patterns": {
        "avg_hold_time_minutes": "decimal",
        "position_size_consistency": "decimal",
        "winner_hold_ratio": "decimal",
        "partial_exit_frequency": "decimal",
        "dca_frequency": "decimal"
      },
      "profit_distribution": {
        "high_profit_trades_pct": "decimal",
        "breakeven_trades_pct": "decimal",
        "avg_winning_trade_pct": "decimal",
        "avg_losing_trade_pct": "decimal",
        "profit_factor": "decimal"
      }
    },
    "metadata": {
      "analyzed_at": "2025-07-15T18:26:45.840679766Z",
      "data_source": "BirdEye",
      "tokens_processed": 2,
      "events_processed": 18,
      "analysis_duration_ms": 1272,
      "algorithm_version": "new_pnl_engine_v1.0",
      "quality_score": "65"
    }
  },
  "timestamp": "2025-07-15T18:26:45.840735472Z"
}
```

### 2. Individual Trade Details
**`GET /api/v2/wallets/:wallet_address/trades`**

Detailed breakdown of individual trades with performance categorization.

#### Example Request
```bash
curl "http://localhost:8080/api/v2/wallets/5ngDQ3vMT7GHeFvXP5GLX79t6fqG2t15kyQTtst9zFdw/trades"
```

#### Response Structure
```json
{
  "data": {
    "matched_trades": [
      {
        "trade": {
          "token_address": "string",
          "token_symbol": "string",
          "buy_event": {
            "timestamp": "2025-07-15T18:26:45Z",
            "quantity": "decimal",
            "usd_price_per_token": "decimal",
            "usd_value": "decimal"
          },
          "sell_event": {
            "timestamp": "2025-07-15T18:26:45Z",
            "quantity": "decimal",
            "usd_price_per_token": "decimal",
            "usd_value": "decimal"
          },
          "quantity_matched": "decimal",
          "realized_pnl_usd": "decimal",
          "realized_pnl_percentage": "decimal",
          "hold_time_seconds": 3600
        },
        "performance_category": "HighlyProfitable",
        "hold_time_category": "ShortTerm",
        "position_size_percentage": "decimal",
        "timing_score": "decimal"
      }
    ],
    "unmatched_sells": [
      {
        "sell": {
          "sell_event": {
            "wallet_address": "string",
            "token_address": "string",
            "token_symbol": "string",
            "event_type": "Sell",
            "quantity": "decimal",
            "usd_price_per_token": "decimal",
            "usd_value": "decimal",
            "timestamp": "2025-07-15T18:26:45Z"
          },
          "unmatched_quantity": "decimal",
          "phantom_buy_price": "decimal",
          "phantom_pnl_usd": "decimal"
        },
        "likely_reason": "PreExistingPosition",
        "portfolio_impact": "decimal"
      }
    ],
    "statistics": {
      "total_trades": 0,
      "win_rate": "decimal",
      "avg_trade_duration_minutes": "decimal",
      "best_trade_pnl": "decimal",
      "worst_trade_pnl": "decimal",
      "consistency_metrics": {
        "return_volatility": "decimal",
        "trades_within_1_stddev": "decimal",
        "longest_win_streak": 0,
        "longest_lose_streak": 0,
        "avg_time_between_trades_hours": "decimal"
      }
    }
  }
}
```

### 3. Current Positions
**`GET /api/v2/wallets/:wallet_address/positions`**

Current holdings with risk assessment and portfolio allocation.

#### Example Request
```bash
curl "http://localhost:8080/api/v2/wallets/5ngDQ3vMT7GHeFvXP5GLX79t6fqG2t15kyQTtst9zFdw/positions"
```

#### Response Structure
```json
{
  "data": {
    "positions": [
      {
        "position": {
          "token_address": "string",
          "token_symbol": "string",
          "quantity": "decimal",
          "avg_cost_basis_usd": "decimal",
          "total_cost_basis_usd": "decimal"
        },
        "current_value_usd": "decimal",
        "unrealized_pnl_usd": "decimal",
        "unrealized_pnl_percentage": "decimal",
        "days_held": 30,
        "portfolio_percentage": "decimal",
        "risk_level": "Medium"
      }
    ],
    "allocation": {
      "position_count": 1,
      "largest_position_pct": "decimal",
      "smallest_position_pct": "decimal",
      "avg_position_pct": "decimal",
      "concentration_score": "decimal"
    },
    "management_metrics": {
      "avg_hold_time_days": "decimal",
      "sizing_consistency_score": "decimal",
      "diversification_score": "decimal",
      "risk_management_score": "decimal"
    }
  }
}
```

### 4. Enhanced Batch Analysis
**`POST /api/v2/pnl/batch/run`**

Submit multiple wallets for enhanced batch analysis with copy trading metrics.

#### Request Body
```json
{
  "wallet_addresses": [
    "5ngDQ3vMT7GHeFvXP5GLX79t6fqG2t15kyQTtst9zFdw",
    "another_wallet_address"
  ],
  "include_copy_trading_metrics": true,
  "include_trade_details": true,
  "max_transactions": 500
}
```

#### Example Request
```bash
curl -X POST "http://localhost:8080/api/v2/pnl/batch/run" \
  -H "Content-Type: application/json" \
  -d '{
    "wallet_addresses": ["5ngDQ3vMT7GHeFvXP5GLX79t6fqG2t15kyQTtst9zFdw"],
    "include_copy_trading_metrics": true,
    "include_trade_details": true
  }'
```

#### Response Structure
```json
{
  "data": {
    "job_id": "uuid",
    "status": "pending",
    "wallet_count": 1,
    "estimated_completion_minutes": 1,
    "features": {
      "copy_trading_metrics": true,
      "trade_details": true
    }
  }
}
```

## 📊 Copy Trading Features

### Trading Style Classification

API v2 automatically classifies trading styles based on hold times and patterns:

#### Types
- **Scalper**: `avg_hold_minutes < 60`
- **SwingTrader**: `60 ≤ avg_hold_hours < 24 * 60`
- **LongTerm**: `avg_hold_days ≥ 1`
- **Mixed**: Combination with predominant style

#### Example Classification
```json
{
  "trading_style": {
    "Scalper": {
      "avg_hold_minutes": "45.2"
    }
  }
}
```

### Risk Assessment

#### Risk Levels
- **Low**: `position_percentage < 5%`
- **Medium**: `5% ≤ position_percentage < 15%`
- **High**: `15% ≤ position_percentage < 30%`
- **VeryHigh**: `position_percentage ≥ 30%`

#### Risk Metrics
```json
{
  "risk_metrics": {
    "max_position_percentage": "25.5",
    "diversification_score": "75.2",
    "max_consecutive_losses": 3,
    "avg_loss_per_trade": "-8.5",
    "max_win_streak": 5,
    "risk_adjusted_return": "12.3"
  }
}
```

### Performance Categories

#### Trade Performance
- **HighlyProfitable**: P&L > 50%
- **Profitable**: 10% < P&L ≤ 50%
- **ModerateGain**: 0% < P&L ≤ 10%
- **BreakEven**: -5% < P&L ≤ 0%
- **ModereLoss**: -20% < P&L ≤ -5%
- **SignificantLoss**: -50% < P&L ≤ -20%
- **MajorLoss**: P&L ≤ -50%

#### Hold Time Categories
- **Scalp**: < 1 hour
- **Intraday**: 1-24 hours
- **ShortTerm**: 1-7 days
- **MediumTerm**: 7-30 days
- **LongTerm**: > 30 days

### Quality Scoring

Quality scores range from 0-100 and consider:
- **Base Score**: 50
- **Trade Count Bonus**: +20 for ≥10 trades, +10 for ≥5 trades
- **Profitability Bonus**: +15 for positive P&L
- **Win Rate Bonus**: +15 for >60% win rate, +10 for >40% win rate

#### Example
```json
{
  "metadata": {
    "quality_score": "75",
    "total_trades": 15,
    "win_rate_percentage": "65.5",
    "total_pnl_usd": "2500.50"
  }
}
```

## 🔧 Frontend Integration

### React TypeScript Example

```typescript
interface WalletAnalysisV2 {
  wallet_address: string;
  portfolio_result: {
    total_pnl_usd: string;
    tokens_analyzed: number;
    events_processed: number;
    overall_win_rate_percentage: string;
  };
  copy_trading_metrics: {
    trading_style: TradingStyle;
    consistency_score: string;
    risk_metrics: RiskMetrics;
  };
  metadata: {
    analysis_duration_ms: number;
    quality_score: string;
    data_source: string;
  };
}

// API Hook
const useWalletAnalysis = (walletAddress: string) => {
  const [data, setData] = useState<WalletAnalysisV2 | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchAnalysis = async () => {
    setLoading(true);
    setError(null);
    
    try {
      const response = await fetch(
        `http://localhost:8080/api/v2/wallets/${walletAddress}/analysis`
      );
      
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
      }
      
      const result = await response.json();
      setData(result.data);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (walletAddress) {
      fetchAnalysis();
    }
  }, [walletAddress]);

  return { data, loading, error, refetch: fetchAnalysis };
};

// Component Usage
const WalletDashboard: React.FC<{ walletAddress: string }> = ({ walletAddress }) => {
  const { data, loading, error } = useWalletAnalysis(walletAddress);

  if (loading) return <div>Loading analysis...</div>;
  if (error) return <div>Error: {error}</div>;
  if (!data) return <div>No data available</div>;

  return (
    <div className="wallet-dashboard">
      <h2>Wallet Analysis: {data.wallet_address}</h2>
      
      <div className="pnl-summary">
        <h3>P&L Summary</h3>
        <p>Total P&L: ${data.portfolio_result.total_pnl_usd}</p>
        <p>Win Rate: {data.portfolio_result.overall_win_rate_percentage}%</p>
        <p>Tokens: {data.portfolio_result.tokens_analyzed}</p>
        <p>Quality Score: {data.metadata.quality_score}/100</p>
      </div>

      <div className="trading-style">
        <h3>Trading Style</h3>
        {JSON.stringify(data.copy_trading_metrics.trading_style)}
      </div>

      <div className="risk-metrics">
        <h3>Risk Assessment</h3>
        <p>Max Position: {data.copy_trading_metrics.risk_metrics.max_position_percentage}%</p>
        <p>Diversification: {data.copy_trading_metrics.risk_metrics.diversification_score}/100</p>
      </div>
    </div>
  );
};
```

### Error Handling

```typescript
interface ApiError {
  error: string;
  timestamp: string;
}

const handleApiError = (error: ApiError) => {
  switch (true) {
    case error.error.includes('BirdEye error'):
      return 'Data source temporarily unavailable. Please try again.';
    case error.error.includes('Invalid wallet address'):
      return 'Please enter a valid Solana wallet address.';
    case error.error.includes('timeout'):
      return 'Analysis is taking longer than expected. Please try again.';
    default:
      return 'An unexpected error occurred. Please try again.';
  }
};
```

## 🎯 Common Use Cases

### 1. Copy Trading Evaluation
```typescript
const evaluateTrader = async (walletAddress: string) => {
  const analysis = await fetchWalletAnalysis(walletAddress);
  
  // Quality checks
  if (analysis.metadata.quality_score < 60) {
    return { recommended: false, reason: 'Low quality score' };
  }
  
  // Risk assessment
  if (analysis.copy_trading_metrics.risk_metrics.max_position_percentage > 30) {
    return { recommended: false, reason: 'High risk - large position sizes' };
  }
  
  // Performance check
  if (parseFloat(analysis.portfolio_result.total_pnl_usd) < 0) {
    return { recommended: false, reason: 'Negative overall performance' };
  }
  
  return { recommended: true, score: analysis.metadata.quality_score };
};
```

### 2. Portfolio Tracking
```typescript
const trackPortfolio = async (walletAddress: string) => {
  const positions = await fetchWalletPositions(walletAddress);
  
  const portfolio = {
    totalValue: positions.positions.reduce((sum, pos) => 
      sum + parseFloat(pos.current_value_usd), 0
    ),
    topHolding: positions.positions.reduce((max, pos) => 
      parseFloat(pos.current_value_usd) > parseFloat(max.current_value_usd) ? pos : max
    ),
    riskLevel: positions.allocation.concentration_score < 50 ? 'High' : 'Medium'
  };
  
  return portfolio;
};
```

### 3. Performance Monitoring
```typescript
const monitorPerformance = async (walletAddress: string) => {
  const analysis = await fetchWalletAnalysis(walletAddress);
  
  const metrics = {
    totalPnL: parseFloat(analysis.portfolio_result.total_pnl_usd),
    winRate: parseFloat(analysis.portfolio_result.overall_win_rate_percentage),
    avgHoldTime: parseFloat(analysis.portfolio_result.avg_hold_time_minutes),
    tradingStyle: analysis.copy_trading_metrics.trading_style,
    riskScore: analysis.copy_trading_metrics.risk_metrics.risk_adjusted_return
  };
  
  return metrics;
};
```

## 📈 Performance Considerations

### Response Times
- **Wallet Analysis**: ~1-2 seconds
- **Trade Details**: ~1-3 seconds (depends on trade count)
- **Current Positions**: ~1-2 seconds
- **Batch Analysis**: ~10-30 seconds per wallet

### Rate Limits
- **BirdEye API**: 100 requests per second
- **Transaction Limit**: 500 transactions per request (default)
- **Batch Limit**: 100 wallets per batch request

### Caching Strategy
```typescript
const cache = new Map<string, { data: WalletAnalysisV2; timestamp: number }>();
const CACHE_DURATION = 5 * 60 * 1000; // 5 minutes

const getCachedAnalysis = (walletAddress: string): WalletAnalysisV2 | null => {
  const cached = cache.get(walletAddress);
  if (cached && Date.now() - cached.timestamp < CACHE_DURATION) {
    return cached.data;
  }
  return null;
};
```

## 🚨 Error Handling

### Common Errors
- **400 Bad Request**: Invalid wallet address format
- **404 Not Found**: Wallet not found or no trading history
- **500 Internal Server Error**: Data source issues or processing errors
- **503 Service Unavailable**: Rate limiting or temporary unavailability

### Retry Strategy
```typescript
const fetchWithRetry = async (url: string, maxRetries = 3): Promise<Response> => {
  for (let i = 0; i < maxRetries; i++) {
    try {
      const response = await fetch(url);
      if (response.ok) return response;
      
      if (response.status === 503) {
        // Rate limited, wait and retry
        await new Promise(resolve => setTimeout(resolve, 1000 * (i + 1)));
        continue;
      }
      
      throw new Error(`HTTP ${response.status}`);
    } catch (error) {
      if (i === maxRetries - 1) throw error;
      await new Promise(resolve => setTimeout(resolve, 1000));
    }
  }
  
  throw new Error('Max retries exceeded');
};
```

## 🔄 Migration from v1

### Key Changes
1. **Endpoint Structure**: `/api/v2/wallets/:address/analysis` vs `/api/pnl/batch/run`
2. **Response Format**: Direct portfolio data vs wrapped results
3. **Copy Trading Features**: New metrics and classifications
4. **Performance**: Faster response times and better caching

### Migration Example
```typescript
// v1 API
const v1Analysis = await fetch('/api/pnl/batch/run', {
  method: 'POST',
  body: JSON.stringify({ wallet_addresses: [address] })
});

// v2 API
const v2Analysis = await fetch(`/api/v2/wallets/${address}/analysis`);
```

## 📚 Testing

### Test Wallet
Use this wallet address for testing and development:
```
5ngDQ3vMT7GHeFvXP5GLX79t6fqG2t15kyQTtst9zFdw
```

### Expected Results
- **Total P&L**: ~$5,361
- **Tokens**: 2 (MASHA, SOL)
- **Events**: 18 transactions
- **Quality Score**: 65/100
- **Trading Style**: Scalper

### Integration Tests
```bash
# Basic functionality
curl "http://localhost:8080/api/v2/wallets/5ngDQ3vMT7GHeFvXP5GLX79t6fqG2t15kyQTtst9zFdw/analysis"

# With parameters
curl "http://localhost:8080/api/v2/wallets/5ngDQ3vMT7GHeFvXP5GLX79t6fqG2t15kyQTtst9zFdw/analysis?max_transactions=50"

# Trade details
curl "http://localhost:8080/api/v2/wallets/5ngDQ3vMT7GHeFvXP5GLX79t6fqG2t15kyQTtst9zFdw/trades"

# Current positions
curl "http://localhost:8080/api/v2/wallets/5ngDQ3vMT7GHeFvXP5GLX79t6fqG2t15kyQTtst9zFdw/positions"
```

## 🔮 Future Endpoints

### Coming Soon
- **Token-Specific Analysis**: `/api/v2/wallets/:address/tokens/:token_address`
- **Wallet Comparison**: `/api/v2/wallets/compare`
- **Copy Trading Leaderboard**: `/api/v2/analytics/leaderboard`
- **Market Trends**: `/api/v2/analytics/market-trends`

### Planned Features
- Real-time WebSocket updates
- Advanced filtering and sorting
- Performance benchmarking
- Social trading features
- Risk management tools

---

## 📞 Support

For issues or questions regarding API v2 integration:
- Check the error response format for specific error details
- Verify wallet address format (Solana base58 encoding)
- Ensure proper query parameter formatting
- Test with the provided example wallet address

**Last Updated**: July 15, 2025
**API Version**: v2.0.0
**Documentation Version**: 1.0.0