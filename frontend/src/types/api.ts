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
  discovered_at: string
  analyzed_at?: string
  pnl_usd?: number
  win_rate?: number
  trade_count?: number
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