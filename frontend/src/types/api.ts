// API Response Types for Wallet Analyzer System

export interface ApiResponse<T> {
  data: T
}

// Health Check Types
export interface ComponentHealth {
  connected?: boolean
  accessible?: boolean
  latency_ms?: number
  error?: string | null
  status?: 'Running' | 'Stopped' | 'Error'
}

export interface HealthComponents {
  redis: ComponentHealth
  birdeye_api: ComponentHealth
  services: {
    wallet_discovery: 'Running' | 'Stopped' | 'Error'
    pnl_analysis: 'Running' | 'Stopped' | 'Error'
  }
}

export interface HealthResponse {
  status: 'healthy' | 'degraded' | 'down'
  version: string
  uptime_seconds: number
  components: HealthComponents
}

// Service Status Types
export interface ServiceState {
  state: 'Running' | 'Stopped' | 'Error'
  last_activity?: string
  cycles_completed?: number
  queue_size?: number
}

export interface ServiceStats {
  wallet_discovery: ServiceState
  pnl_analysis: ServiceState
}

export interface OrchestratorMetrics {
  total_running_jobs: number
  pending_batch_jobs: number
  running_batch_jobs: number
  completed_batch_jobs: number
  queue_size: number
}

export interface ServicesStatusResponse {
  orchestrator: OrchestratorMetrics
  dex_client: {
    enabled: boolean
    connected: boolean
    last_activity?: string
    processed_pairs?: number
    discovered_wallets?: number
  }
}

// Results Summary Types
export interface PaginationInfo {
  total_count: number
  limit: number
  offset: number
  has_more: boolean
}

export interface ResultsSummary {
  total_wallets: number
  profitable_wallets: number
  total_pnl_usd: number
  average_pnl_usd: number
  profitability_rate: number
  last_updated: string
}

export interface WalletResult {
  wallet_address: string
  chain: string
  token_address: string
  token_symbol?: string
  total_pnl_usd: number
  realized_pnl_usd: number
  unrealized_pnl_usd: number
  roi_percentage: number
  total_trades: number
  win_rate: number
  avg_hold_time_minutes: number
  analyzed_at: string
}

export interface AllResultsResponse {
  results: WalletResult[]
  pagination: PaginationInfo
  summary: ResultsSummary
}

// Batch Jobs Types
export interface BatchJobSummary {
  id: string
  status: 'Pending' | 'Running' | 'Completed' | 'Failed'
  wallet_count: number
  chain: string
  created_at: string
  started_at?: string
  completed_at?: string
  success_count?: number
  failure_count?: number
}

export interface BatchJobHistoryResponse {
  jobs: BatchJobSummary[]
  pagination: PaginationInfo
  summary: {
    total_jobs: number
    pending_jobs: number
    running_jobs: number
    completed_jobs: number
    failed_jobs: number
  }
}

// Discovered Wallets Types
export interface DiscoveredWallet {
  wallet_address: string
  chain: string
  discovered_at: string
  analyzed_at?: string
  pnl_usd?: number
  win_rate?: number
  trade_count?: number
  avg_hold_time_minutes?: number
  status: string
}

export interface DiscoveredWalletsResponse {
  wallets: DiscoveredWallet[]
  pagination: PaginationInfo
  summary: {
    total_discovered: number
    analyzed_count: number
    profitable_count: number
    average_pnl_usd: number
    total_pnl_usd: number
  }
}

// Dashboard Metrics (derived from various endpoints)
export interface DashboardMetrics {
  totalWallets: number
  totalVolume: number
  profitableWallets: number
  averagePnl: number
  totalTrades: number
  profitabilityRate: number
  activeServices: number
  queueSize: number
}

export interface ChartDataPoint {
  date: string
  volume: number
  profit: number
  wallets: number
}

// System Metrics (simplified from monitoring)
export interface SystemMetrics {
  cpu: number
  memory: number
  disk: number
  network: number
}

// Activity Feed Item
export interface ActivityItem {
  id: string
  type: 'batch_completed' | 'wallet_discovered' | 'service_started' | 'service_stopped'
  title: string
  description: string
  timestamp: string
  value?: number
  status?: 'success' | 'error' | 'warning'
}

// Detailed Portfolio Types
export interface MatchedTrade {
  buy_event: FinancialEvent
  sell_event: FinancialEvent
  matched_quantity: string
  realized_pnl_usd: string
  hold_time_seconds: number
}

export interface UnmatchedSell {
  sell_event: FinancialEvent
  unmatched_quantity: string
  phantom_buy_price: string
  phantom_pnl_usd: string
}

export interface RemainingPosition {
  token_address: string
  token_symbol: string
  quantity: string
  avg_cost_basis_usd: string
  total_cost_basis_usd: string
}

export interface FinancialEvent {
  event_type: 'Buy' | 'Sell'
  transaction_signature: string
  timestamp: string
  token_address: string
  token_symbol: string
  token_name: string
  quantity: string
  price_per_token_usd: string
  total_value_usd: string
  platform?: string
}

export interface TokenPnLResult {
  token_address: string
  token_symbol: string
  token_name: string
  matched_trades: MatchedTrade[]
  remaining_position?: RemainingPosition
  total_realized_pnl_usd: string
  total_unrealized_pnl_usd: string
  total_pnl_usd: string
  total_invested_usd?: string
  total_returned_usd?: string
  total_trades: number
  winning_trades?: number
  losing_trades?: number
  win_rate_percentage: string
  avg_hold_time_minutes: string
  min_hold_time_minutes: string
  max_hold_time_minutes: string
  current_winning_streak?: number
  longest_winning_streak?: number
  current_losing_streak?: number
  longest_losing_streak?: number
}

export interface PortfolioPnLResult {
  wallet_address: string
  token_results: TokenPnLResult[]
  total_realized_pnl_usd: string
  total_unrealized_pnl_usd: string
  total_pnl_usd: string
  total_invested_usd?: string
  total_returned_usd?: string
  total_trades: number
  winning_trades?: number
  losing_trades?: number
  overall_win_rate_percentage: string
  avg_hold_time_minutes: string
  tokens_analyzed: number
  events_processed: number
  analysis_timestamp: string
  current_winning_streak?: number
  longest_winning_streak?: number
  current_losing_streak?: number
  longest_losing_streak?: number
  profit_percentage?: string
}

export interface WalletDetailResponse {
  wallet_address: string
  chain: string
  portfolio_result: PortfolioPnLResult
  analyzed_at: string
}