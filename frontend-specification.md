# 🎯 Wallet Analyzer Frontend Specification

## 📋 **Application Overview**

A comprehensive React-based dashboard for the Wallet Analyzer system with 5 main sections:

1. **Dashboard** - General analytics and system overview
2. **Service Control** - Start/stop/configure services
3. **System Monitoring** - Real-time system health
4. **Batch Jobs** - Submit and manage batch analysis jobs
5. **Results/Wallets** - View all analyzed wallets with filtering

---

## 🏠 **1. Dashboard Page**

### **Purpose**: High-level system overview and key metrics

### **Components**:
- **System Health Widget**: Overall system status
- **Key Metrics Cards**: Total wallets analyzed, success rate, average P&L
- **Recent Activity Feed**: Latest batch jobs and discoveries
- **Performance Charts**: P&L trends over time
- **Quick Actions**: Start common operations

### **API Endpoints**:

```typescript
// System Health
GET /health/detailed
Response: {
  data: {
    status: "healthy" | "degraded" | "down",
    version: string,
    uptime_seconds: number,
    components: {
      redis: { connected: boolean, latency_ms: number, error?: string },
      birdeye_api: { accessible: boolean, latency_ms: number, error?: string },
      services: {
        wallet_discovery: "Running" | "Stopped" | "Error",
        pnl_analysis: "Running" | "Stopped" | "Error"
      }
    }
  }
}

// Summary Statistics
GET /api/results?limit=1&summary=true
Response: {
  data: {
    summary: {
      total_wallets: number,
      profitable_wallets: number,
      total_pnl_usd: number,
      average_pnl_usd: number,
      profitability_rate: number,
      last_updated: string
    }
  }
}

// Recent Batch Jobs
GET /api/pnl/batch/history?limit=10
Response: {
  data: {
    jobs: Array<{
      id: string,
      status: "Pending" | "Running" | "Completed" | "Failed",
      wallet_count: number,
      created_at: string,
      completed_at?: string
    }>
  }
}

// Service Status
GET /api/services/status
Response: {
  data: {
    orchestrator: {
      total_running_jobs: number,
      pending_batch_jobs: number,
      running_batch_jobs: number,
      completed_batch_jobs: number,
      queue_size: number
    },
    dex_client: {
      enabled: boolean,
      connected: boolean,
      last_activity: string,
      processed_pairs: number,
      discovered_wallets: number
    }
  }
}
```

---

## ⚙️ **2. Service Control Page**

### **Purpose**: Start, stop, and configure system services

### **Components**:
- **Service Status Cards**: Current status of each service
- **Service Control Buttons**: Start/Stop/Restart actions
- **Configuration Forms**: Service-specific settings
- **Configuration Presets**: Save/load common configurations

### **API Endpoints**:

```typescript
// Get Current Configuration
GET /api/config
Response: {
  data: {
    system: {
      debug_mode: boolean,
      redis_mode: boolean,
      process_loop_ms: number
    },
    birdeye: {
      default_max_transactions: number,
      max_transactions_per_trader: number,
      rate_limit_per_second: number
    },
    pnl: {
      timeframe_mode: "none" | "general" | "specific",
      timeframe_general?: string,
      wallet_min_capital: number,
      aggregator_min_hold_minutes: number,
      amount_trades: number,
      win_rate: number
    }
  }
}

// Universal Service Control
POST /api/services/control
Request: {
  action: "start" | "stop" | "restart",
  service: "wallet_discovery" | "pnl_analysis",
  config_override?: {
    min_capital_sol?: number,
    min_trades?: number,
    max_transactions_to_fetch?: number,
    timeframe_filter?: {
      start_time?: string,
      end_time?: string,
      mode: "specific" | "general" | "none"
    }
  }
}
Response: {
  data: {
    message: string
  }
}

// Update System Configuration
POST /api/config
Request: {
  pnl_filters: {
    timeframe_mode?: string,
    timeframe_general?: string,
    wallet_min_capital?: number,
    aggregator_min_hold_minutes?: number,
    amount_trades?: number,
    win_rate?: number
  }
}
Response: {
  data: {
    message: string
  }
}
```

---

## 📊 **3. System Monitoring Page**

### **Purpose**: Real-time system health and performance monitoring

### **Components**:
- **Real-time Health Dashboard**: Live system metrics
- **Service Performance Charts**: CPU, memory, API response times
- **Error Logs**: Recent errors and warnings
- **API Status Indicators**: External API health (BirdEye, Helius)
- **Auto-refresh Controls**: Configurable refresh intervals

### **API Endpoints**:

```typescript
// Detailed Health Check (Auto-refresh every 5s)
GET /health/detailed
Response: {
  data: {
    status: "healthy" | "degraded" | "down",
    version: string,
    uptime_seconds: number,
    components: {
      redis: { connected: boolean, latency_ms: number, error?: string },
      birdeye_api: { accessible: boolean, latency_ms: number, error?: string },
      helius_api: { accessible: boolean, latency_ms: number, error?: string },
      services: {
        wallet_discovery: "Running" | "Stopped" | "Error",
        pnl_analysis: "Running" | "Stopped" | "Error"
      }
    }
  }
}

// System Logs (if available)
GET /api/logs?limit=100&level=error
Response: {
  data: {
    logs: Array<{
      timestamp: string,
      level: "error" | "warn" | "info" | "debug",
      message: string,
      component?: string
    }>
  }
}

// Service Metrics
GET /api/services/status
Response: {
  data: {
    orchestrator: {
      total_running_jobs: number,
      pending_batch_jobs: number,
      running_batch_jobs: number,
      completed_batch_jobs: number,
      failed_batch_jobs: number,
      queue_size: number
    },
    dex_client: {
      enabled: boolean,
      connected: boolean,
      last_activity: string,
      processed_pairs: number,
      discovered_wallets: number
    }
  }
}
```

---

## 🎯 **4. Batch Jobs Page**

### **Purpose**: Submit, manage, and view batch analysis jobs

### **Components**:
- **Job Submission Form**: 
  - Wallet input (textarea for newline-separated addresses OR CSV upload)
  - Configuration options (use defaults or custom)
  - Filter settings (timeframe, min capital, etc.)
- **Active Jobs List**: Currently running jobs with progress
- **Job History Table**: All past jobs with status and results
- **Job Details Modal**: Click into specific job to view:
  - Individual wallet results
  - Success/failure breakdown
  - Export options

### **API Endpoints**:

```typescript
// Submit Enhanced Batch Job (API v2)
POST /api/v2/pnl/batch/run
Request: {
  wallet_addresses: string[], // Array of Solana wallet addresses
  filters?: {
    min_portfolio_value_usd?: number,
    min_trades?: number,
    min_win_rate?: number,
    timeframe_days?: number,
    active_traders_only?: boolean,
    min_trade_frequency?: number
  },
  include_copy_trading_metrics?: boolean,
  include_trade_details?: boolean
}
Response: {
  data: {
    job_id: string,
    wallet_count: number,
    status: "Pending",
    submitted_at: string,
    estimated_completion_time?: string
  }
}

// Get Job Status (Poll every 2s for active jobs)
GET /api/pnl/batch/status/{job_id}
Response: {
  data: {
    job_id: string,
    status: "Pending" | "Running" | "Completed" | "Failed",
    wallet_count: number,
    created_at: string,
    started_at?: string,
    completed_at?: string,
    progress?: {
      total_wallets: number,
      completed_wallets: number,
      successful_wallets: number,
      failed_wallets: number,
      progress_percentage: number
    }
  }
}

// Get Enhanced Job Results (API v2)
GET /api/v2/pnl/batch/results/{job_id}
Response: {
  data: {
    job_id: string,
    results: {
      [wallet_address: string]: {
        wallet_address: string,
        portfolio_result: {
          total_pnl_usd: number,
          realized_pnl_usd: number,
          unrealized_pnl_usd: number,
          roi_percentage: number,
          total_trades: number,
          win_rate: number,
          avg_hold_time_minutes: number,
          risk_adjusted_return: number
        },
        copy_trading_metrics: {
          trading_style: "Scalper" | "SwingTrader" | "LongTerm" | "Mixed",
          consistency_score: number,
          risk_metrics: {
            max_position_percentage: number,
            diversification_score: number,
            max_consecutive_losses: number,
            risk_adjusted_return: number
          },
          position_patterns: {
            avg_hold_time_minutes: number,
            position_size_consistency: number,
            winner_hold_ratio: number
          }
        },
        metadata: {
          analyzed_at: string,
          data_source: string,
          quality_score: number
        }
      }
    },
    batch_statistics: {
      total_wallets: number,
      successful_analyses: number,
      failed_analyses: number,
      avg_analysis_time_ms: number,
      top_performer?: string,
      avg_portfolio_value_usd: number
    },
    copy_trading_ranking: Array<{
      wallet_address: string,
      copy_trading_score: number,
      rank: number,
      strengths: string[],
      risk_factors: string[],
      recommended_allocation_pct: number
    }>
  }
}

// Export Job Results as CSV
GET /api/pnl/batch/results/{job_id}/export.csv
Response: CSV file download

// Get Job History
GET /api/pnl/batch/history?limit=50&offset=0
Response: {
  data: {
    jobs: Array<{
      id: string,
      status: "Pending" | "Running" | "Completed" | "Failed",
      wallet_count: number,
      created_at: string,
      completed_at?: string,
      summary?: {
        successful_analyses: number,
        total_pnl_usd: number
      }
    }>,
    pagination: {
      total_count: number,
      limit: number,
      offset: number,
      has_more: boolean
    }
  }
}
```

---

## 🔍 **5. Results/Wallets Page**

### **Purpose**: View all analyzed wallets with advanced filtering and sorting

### **Components**:
- **Advanced Filter Panel**:
  - P&L range slider (min/max USD)
  - Win rate range slider (0-100%)
  - Hold time range slider (minutes)
  - Copy trading score range
  - Trading style filter (Scalper, Swing, Long-term)
  - Date range picker
- **Sortable Data Table**:
  - Columns: Wallet Address, Total P&L, Win Rate, Total Trades, Copy Trading Score, Trading Style, Quality Score, Last Analyzed
  - Click wallet row to view detailed analysis
- **Wallet Details Modal**:
  - Complete enhanced P&L breakdown
  - Copy trading metrics
  - Risk assessment
  - Position patterns
  - Performance charts

### **API Endpoints**:

```typescript
// Get Enhanced Wallet Analysis (API v2) - Primary endpoint for wallet details
GET /api/v2/wallets/{wallet_address}/analysis
Response: {
  data: {
    wallet_address: string,
    portfolio_result: {
      total_pnl_usd: number,
      realized_pnl_usd: number,
      unrealized_pnl_usd: number,
      roi_percentage: number,
      total_trades: number,
      win_rate: number,
      avg_hold_time_minutes: number,
      total_capital_deployed: number,
      risk_adjusted_return: number,
      sharpe_ratio: number,
      max_drawdown: number,
      profit_factor: number
    },
    copy_trading_metrics: {
      trading_style: {
        type: "Scalper" | "SwingTrader" | "LongTerm" | "Mixed",
        avg_hold_minutes?: number,
        avg_hold_hours?: number,
        avg_hold_days?: number,
        predominant_style?: string
      },
      consistency_score: number,
      risk_metrics: {
        max_position_percentage: number,
        diversification_score: number,
        max_consecutive_losses: number,
        avg_loss_per_trade: number,
        max_win_streak: number,
        risk_adjusted_return: number
      },
      position_patterns: {
        avg_hold_time_minutes: number,
        position_size_consistency: number,
        winner_hold_ratio: number,
        partial_exit_frequency: number,
        dca_frequency: number
      },
      profit_distribution: {
        high_profit_trades_pct: number,
        breakeven_trades_pct: number,
        avg_winning_trade_pct: number,
        avg_losing_trade_pct: number,
        profit_factor: number
      }
    },
    metadata: {
      analyzed_at: string,
      data_source: string,
      tokens_processed: number,
      events_processed: number,
      analysis_duration_ms: number,
      algorithm_version: string,
      quality_score: number
    }
  }
}

// Get Enhanced Trade Details (API v2)
GET /api/v2/wallets/{wallet_address}/trades
Response: {
  data: {
    matched_trades: Array<{
      trade: {
        buy_timestamp: string,
        sell_timestamp: string,
        token_mint: string,
        token_symbol?: string,
        quantity: number,
        buy_price_usd: number,
        sell_price_usd: number,
        pnl_usd: number,
        pnl_percentage: number,
        hold_time_minutes: number,
        fees_usd: number
      },
      performance_category: "HighlyProfitable" | "Profitable" | "ModerateGain" | "BreakEven" | "ModerateLoss" | "SignificantLoss" | "MajorLoss",
      hold_time_category: "Scalp" | "Intraday" | "ShortTerm" | "MediumTerm" | "LongTerm",
      position_size_percentage: number,
      timing_score: number
    }>,
    unmatched_sells: Array<{
      sell: {
        timestamp: string,
        token_mint: string,
        token_symbol?: string,
        quantity: number,
        sell_price_usd: number,
        proceeds_usd: number
      },
      likely_reason: "PreExistingPosition" | "Airdrop" | "Transfer" | "DataGap" | "Other",
      portfolio_impact: number
    }>,
    statistics: {
      total_trades: number,
      win_rate: number,
      avg_trade_duration_minutes: number,
      best_trade_pnl: number,
      worst_trade_pnl: number,
      consistency_metrics: {
        return_volatility: number,
        trades_within_1_stddev: number,
        longest_win_streak: number,
        longest_lose_streak: number,
        avg_time_between_trades_hours: number
      }
    }
  }
}

// Get Enhanced Current Positions (API v2)
GET /api/v2/wallets/{wallet_address}/positions
Response: {
  data: {
    positions: Array<{
      position: {
        token_mint: string,
        token_symbol?: string,
        quantity: number,
        avg_cost_basis_usd: number,
        total_cost_basis_usd: number,
        first_acquired: string,
        last_acquired: string
      },
      current_value_usd: number,
      unrealized_pnl_usd: number,
      unrealized_pnl_percentage: number,
      days_held: number,
      portfolio_percentage: number,
      risk_level: "Low" | "Medium" | "High" | "VeryHigh"
    }>,
    allocation: {
      position_count: number,
      largest_position_pct: number,
      smallest_position_pct: number,
      avg_position_pct: number,
      concentration_score: number
    },
    management_metrics: {
      avg_hold_time_days: number,
      sizing_consistency_score: number,
      diversification_score: number,
      risk_management_score: number
    }
  }
}

// Get All Wallet Results with Enhanced Filtering
GET /api/results?offset=0&limit=50&sort_by=pnl&order=desc&min_pnl=100&max_pnl=10000&min_win_rate=50&max_win_rate=100&min_hold_time=60&max_hold_time=1440&min_copy_score=70&trading_style=SwingTrader
Query Parameters:
- offset: number (default: 0)
- limit: number (default: 50, max: 1000)
- sort_by: "pnl" | "win_rate" | "hold_time" | "copy_score" | "quality_score" | "analyzed_at" | "wallet_address" (default: "analyzed_at")
- order: "asc" | "desc" (default: "desc")
- min_pnl: number (minimum P&L in USD)
- max_pnl: number (maximum P&L in USD)
- min_win_rate: number (minimum win rate percentage)
- max_win_rate: number (maximum win rate percentage)
- min_hold_time: number (minimum hold time in minutes)
- max_hold_time: number (maximum hold time in minutes)
- min_copy_score: number (minimum copy trading score)
- max_copy_score: number (maximum copy trading score)
- trading_style: "Scalper" | "SwingTrader" | "LongTerm" | "Mixed"
- start_date: string (ISO 8601 date)
- end_date: string (ISO 8601 date)

Response: {
  data: {
    results: Array<{
      wallet_address: string,
      total_pnl_usd: number,
      realized_pnl_usd: number,
      unrealized_pnl_usd: number,
      roi_percentage: number,
      total_trades: number,
      win_rate: number,
      avg_hold_time_minutes: number,
      copy_trading_score: number,
      trading_style: string,
      quality_score: number,
      risk_level: "Low" | "Medium" | "High" | "VeryHigh",
      analyzed_at: string
    }>,
    pagination: {
      total_count: number,
      limit: number,
      offset: number,
      has_more: boolean
    },
    summary: {
      total_wallets: number,
      profitable_wallets: number,
      total_pnl_usd: number,
      average_pnl_usd: number,
      avg_copy_trading_score: number,
      profitability_rate: number
    }
  }
}
```

---

## 🔄 **6. Real-time DEX Monitoring Page**

### **Purpose**: Monitor DEX discovery and trending token analysis

### **Components**:
- **Trending Tokens Feed**: Live feed of discovered trending tokens
- **Wallet Discovery Stats**: Real-time statistics on wallet discovery
- **DEX Connection Status**: Status of DexScreener/BirdEye connections
- **Discovery Queue**: Current queue of wallets being processed
- **Performance Metrics**: Discovery success rates and processing times

### **API Endpoints**:

```typescript
// Get Discovered Wallets from Continuous Mode
GET /api/pnl/continuous/discovered-wallets?limit=50&offset=0&sort_by=discovered_at&order=desc
Query Parameters:
- offset: number (default: 0)
- limit: number (default: 50)
- sort_by: "discovered_at" | "pnl" | "volume" | "trades" (default: "discovered_at")
- order: "asc" | "desc" (default: "desc")
- min_volume: number (minimum trading volume in USD)
- min_trades: number (minimum number of trades)

Response: {
  data: {
    discovered_wallets: Array<{
      wallet_address: string,
      token_address: string,
      token_symbol: string,
      trader_volume_usd: number,
      trader_trades: number,
      discovered_at: string,
      discovery_reason: "trending_token" | "high_volume" | "profitable_pattern",
      analysis_status: "pending" | "processing" | "completed" | "failed"
    }>,
    pagination: {
      total_count: number,
      limit: number,
      offset: number,
      has_more: boolean
    },
    statistics: {
      total_discovered: number,
      pending_analysis: number,
      successful_analyses: number,
      discovery_rate_per_hour: number
    }
  }
}

// Get DEX Service Status
GET /api/dex/status
Response: {
  data: {
    service_status: "Running" | "Stopped" | "Error",
    connections: {
      dexscreener: {
        connected: boolean,
        last_activity: string,
        processed_pairs: number
      },
      birdeye: {
        connected: boolean,
        last_activity: string,
        api_calls_remaining: number
      }
    },
    discovery_stats: {
      trending_tokens_found: number,
      wallets_discovered: number,
      queue_size: number,
      processing_rate_per_minute: number
    },
    performance_metrics: {
      avg_discovery_time_ms: number,
      success_rate: number,
      error_rate: number
    }
  }
}

// Control DEX Service
POST /api/dex/control
Request: {
  action: "start" | "stop" | "restart",
  config_override?: {
    min_volume_threshold?: number,
    max_tokens_per_discovery?: number,
    discovery_interval_minutes?: number
  }
}
Response: {
  data: {
    message: string,
    service_status: "Running" | "Stopped" | "Error"
  }
}
```

---

## 🎨 **Frontend Technical Stack**

### **Recommended Technologies**:
- **Framework**: React 18 with TypeScript
- **Styling**: Tailwind CSS + Shadcn/UI components
- **State Management**: Zustand for global state + React Query for API state
- **Charts**: Recharts for data visualization
- **Tables**: TanStack Table v8 for advanced data tables
- **Forms**: React Hook Form + Zod validation
- **Routing**: React Router v6
- **Real-time**: Socket.IO client for WebSocket connections
- **Date Handling**: date-fns for date operations
- **Icons**: Lucide React for icons

### **Key Features to Implement**:
1. **Real-time Updates**: WebSocket integration for live job progress and system status
2. **Responsive Design**: Mobile-first approach with Tailwind CSS
3. **Advanced Data Visualization**: Interactive charts with drill-down capabilities
4. **Complex Filtering**: Multi-criteria search with real-time results
5. **Batch Operations**: Handle multiple wallet analysis efficiently
6. **Export Functionality**: CSV/PDF downloads with custom formatting
7. **Error Handling**: Comprehensive error boundaries and user feedback
8. **Loading States**: Skeleton screens and progress indicators
9. **Optimistic Updates**: Immediate UI feedback for user actions
10. **Client-side Caching**: React Query for efficient data fetching

### **Layout Structure**:
```
App
├── Layout
│   ├── Sidebar Navigation
│   │   ├── Dashboard
│   │   ├── Service Control
│   │   ├── System Monitoring
│   │   ├── Batch Jobs
│   │   ├── Results/Wallets
│   │   └── DEX Monitoring
│   ├── Header
│   │   ├── System Status Indicator
│   │   ├── Notifications Bell
│   │   ├── Real-time Stats
│   │   └── Settings Menu
│   └── Main Content Area
│       ├── Breadcrumbs
│       ├── Page Title & Actions
│       └── Page-specific Components
└── Global Components
    ├── Modals
    ├── Toasts
    ├── Loading Overlays
    └── Error Boundaries
```

### **Component Architecture**:
```
components/
├── ui/                    # Shadcn/UI base components
├── charts/               # Chart components (Recharts)
├── tables/               # Table components (TanStack)
├── forms/                # Form components (React Hook Form)
├── wallet/               # Wallet-specific components
├── batch/                # Batch job components
├── monitoring/           # System monitoring components
└── shared/               # Shared utility components
```

### **Data Flow**:
1. **API Layer**: React Query for server state management
2. **Global State**: Zustand for UI state (filters, selections, preferences)
3. **Component State**: React useState/useReducer for local state
4. **Real-time Updates**: WebSocket context provider for live data
5. **Caching Strategy**: React Query with background updates

### **Performance Considerations**:
- **Virtualization**: For large tables (React Window)
- **Pagination**: Server-side pagination for large datasets
- **Lazy Loading**: Route-based code splitting
- **Memoization**: React.memo and useMemo for expensive computations
- **Debouncing**: For search/filter inputs
- **Background Sync**: React Query background updates

This specification provides a comprehensive guide for building a modern, scalable frontend for the Wallet Analyzer system using the latest v2 API endpoints! 🚀