'use client'

import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Progress } from '@/components/ui/progress'
import { Skeleton } from '@/components/ui/skeleton'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { SystemHealthIndicator } from '@/components/ui/loading-spinner'
import { api } from '@/lib/api'
import { 
  Cpu, 
  HardDrive, 
  MemoryStick as Memory, 
  Network, 
  AlertCircle, 
  CheckCircle, 
  Clock,
  Server,
  Database,
  Zap,
  Activity,
  Filter,
  RefreshCw,
  Users,
  TrendingUp,
  BarChart3,
  Eye,
  Wallet,
  Target,
  Timer,
  AlertTriangle
} from 'lucide-react'

type LogLevel = 'INFO' | 'WARN' | 'ERROR' | 'DEBUG' | 'ALL'

interface LogEntry {
  id: number
  timestamp: string
  level: string
  component: string
  message: string
}

export default function Monitoring() {
  const [logFilter, setLogFilter] = useState<LogLevel>('ALL')

  // Enhanced health check with auto-refresh
  const { data: health, isLoading: healthLoading } = useQuery({
    queryKey: ['detailed-health'],
    queryFn: api.monitoring.getDetailedHealth,
    refetchInterval: 8000
  })

  // Service-specific monitoring with auto-refresh
  const { data: discoveryStats, isLoading: discoveryLoading } = useQuery({
    queryKey: ['discovery-stats'],
    queryFn: api.monitoring.getDiscoveryStats,
    refetchInterval: 5000
  })

  const { data: pnlStats, isLoading: pnlLoading } = useQuery({
    queryKey: ['pnl-stats'],
    queryFn: api.monitoring.getPnLStats,
    refetchInterval: 5000
  })

  // Queue metrics with auto-refresh
  const { data: queueMetrics, isLoading: queueLoading } = useQuery({
    queryKey: ['queue-metrics'],
    queryFn: api.monitoring.getQueueMetrics,
    refetchInterval: 3000
  })

  // Enhanced orchestrator metrics
  const { data: orchestrator, isLoading: orchestratorLoading } = useQuery({
    queryKey: ['orchestrator-metrics'],
    queryFn: api.monitoring.getOrchestratorMetrics,
    refetchInterval: 6000
  })

  // Real-time logs with auto-refresh
  const { data: logs, isLoading: logsLoading } = useQuery({
    queryKey: ['system-logs'],
    queryFn: api.monitoring.getLogs,
    refetchInterval: 5000
  })

  const getStatusColor = (status: string) => {
    switch (status?.toLowerCase()) {
      case 'healthy':
      case 'running':
        return 'bg-green-success text-white'
      case 'degraded':
      case 'starting':
      case 'stopping':
        return 'bg-orange-warning text-white'
      case 'down':
      case 'stopped':
      case 'error':
        return 'bg-red-500 text-white'
      default:
        return 'bg-gray-500 text-white'
    }
  }

  const getServiceStateIcon = (state: string) => {
    switch (state) {
      case 'Running':
        return <CheckCircle className="w-5 h-5 text-green-success" />
      case 'Starting':
      case 'Stopping':
        return <Clock className="w-5 h-5 text-orange-warning" />
      case 'Stopped':
        return <AlertTriangle className="w-5 h-5 text-gray-400" />
      default:
        return <AlertCircle className="w-5 h-5 text-red-400" />
    }
  }

  const getComponentIcon = (component: string) => {
    switch (component) {
      case 'redis':
        return <Database className="w-5 h-5" />
      case 'birdeye_api':
        return <Network className="w-5 h-5" />
      case 'wallet_discovery':
        return <Eye className="w-5 h-5" />
      case 'pnl_analysis':
        return <BarChart3 className="w-5 h-5" />
      case 'services':
        return <Server className="w-5 h-5" />
      default:
        return <Activity className="w-5 h-5" />
    }
  }

  const getLogLevelColor = (level: string) => {
    switch (level) {
      case 'ERROR':
        return 'text-red-400'
      case 'WARN':
        return 'text-orange-warning'
      case 'INFO':
        return 'text-cyan-bright'
      case 'DEBUG':
        return 'text-gray-400'
      default:
        return 'text-gray-400'
    }
  }

  const filteredLogs = logs?.filter((log: LogEntry) => {
    if (logFilter === 'ALL') return true
    return log.level === logFilter
  }) || []

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold text-white mb-2">System Monitoring</h1>
          <p className="text-gray-400">Real-time wallet analyzer system insights and performance metrics</p>
        </div>
        <div className="flex items-center space-x-2 text-sm text-gray-400">
          <RefreshCw className="w-4 h-4 animate-spin" />
          <span>Auto-refreshing</span>
        </div>
      </div>

      {/* Overall System Health Widget */}
      <Card className="glass-card border-blue-ice/20">
        <CardContent className="pt-6">
          {healthLoading ? (
            <div className="flex items-center justify-center py-8">
              <Skeleton className="h-16 w-64" />
            </div>
          ) : (
            <div className="text-center">
              <div className={`inline-flex items-center px-6 py-3 rounded-lg text-2xl font-bold ${getStatusColor(health?.status || 'HEALTHY')}`}>
                {health?.status === 'healthy' ? (
                  <CheckCircle className="w-8 h-8 mr-3" />
                ) : (
                  <AlertCircle className="w-8 h-8 mr-3" />
                )}
                SYSTEM {(health?.status || 'HEALTHY').toUpperCase()}
              </div>
              <div className="mt-4 flex items-center justify-center space-x-8 text-gray-400">
                <div className="flex items-center space-x-2">
                  <Zap className="w-4 h-4" />
                  <span>Version {health?.version || '2.1.4'}</span>
                </div>
                <div className="flex items-center space-x-2">
                  <Clock className="w-4 h-4" />
                  <span>Uptime: {health?.uptime_seconds ? `${Math.floor(health.uptime_seconds / 3600)}h ${Math.floor((health.uptime_seconds % 3600) / 60)}m` : '0h 0m'}</span>
                </div>
              </div>
            </div>
          )}
        </CardContent>
      </Card>

      {/* Service Status Overview */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {/* Wallet Discovery Service */}
        <Card className="glass-card border-blue-ice/20">
          <CardHeader>
            <CardTitle className="text-white flex items-center">
              <Eye className="w-5 h-5 mr-2" />
              Wallet Discovery Service
              {getServiceStateIcon(discoveryStats?.service_state || 'Stopped')}
            </CardTitle>
            <CardDescription className="text-gray-400">
              Real-time wallet discovery and trending token monitoring
            </CardDescription>
          </CardHeader>
          <CardContent>
            {discoveryLoading ? (
              <div className="grid grid-cols-2 gap-4">
                {Array.from({ length: 4 }).map((_, i) => (
                  <div key={i} className="text-center">
                    <Skeleton className="h-8 w-16 mx-auto mb-2" />
                    <Skeleton className="h-4 w-20 mx-auto" />
                  </div>
                ))}
              </div>
            ) : (
              <div className="space-y-4">
                <div className="grid grid-cols-2 gap-4">
                  <div className="text-center p-3 bg-navy-deep/50 rounded-lg">
                    <div className="text-2xl font-bold text-cyan-bright mb-1">
                      {discoveryStats?.discovered_wallets_total || 0}
                    </div>
                    <p className="text-xs text-gray-400">Total Discovered</p>
                  </div>
                  <div className="text-center p-3 bg-navy-deep/50 rounded-lg">
                    <div className="text-2xl font-bold text-orange-warning mb-1">
                      {discoveryStats?.queue_size || 0}
                    </div>
                    <p className="text-xs text-gray-400">Queue Size</p>
                  </div>
                  <div className="text-center p-3 bg-navy-deep/50 rounded-lg">
                    <div className="text-2xl font-bold text-green-success mb-1">
                      {discoveryStats?.cycles_completed || 0}
                    </div>
                    <p className="text-xs text-gray-400">Cycles Done</p>
                  </div>
                  <div className="text-center p-3 bg-navy-deep/50 rounded-lg">
                    <div className="text-2xl font-bold text-blue-steel mb-1">
                      {discoveryStats?.last_cycle_wallets || 0}
                    </div>
                    <p className="text-xs text-gray-400">Last Cycle</p>
                  </div>
                </div>
                {discoveryStats?.last_activity && (
                  <div className="text-center text-sm text-gray-400">
                    Last Activity: {new Date(discoveryStats.last_activity).toLocaleString()}
                  </div>
                )}
              </div>
            )}
          </CardContent>
        </Card>

        {/* P&L Analysis Service */}
        <Card className="glass-card border-blue-ice/20">
          <CardHeader>
            <CardTitle className="text-white flex items-center">
              <BarChart3 className="w-5 h-5 mr-2" />
              P&L Analysis Service
              {getServiceStateIcon(pnlStats?.service_state || 'Stopped')}
            </CardTitle>
            <CardDescription className="text-gray-400">
              Profit & loss calculation and wallet analysis processing
            </CardDescription>
          </CardHeader>
          <CardContent>
            {pnlLoading ? (
              <div className="grid grid-cols-2 gap-4">
                {Array.from({ length: 4 }).map((_, i) => (
                  <div key={i} className="text-center">
                    <Skeleton className="h-8 w-16 mx-auto mb-2" />
                    <Skeleton className="h-4 w-20 mx-auto" />
                  </div>
                ))}
              </div>
            ) : (
              <div className="space-y-4">
                <div className="grid grid-cols-2 gap-4">
                  <div className="text-center p-3 bg-navy-deep/50 rounded-lg">
                    <div className="text-2xl font-bold text-green-success mb-1">
                      {pnlStats?.successful_analyses || 0}
                    </div>
                    <p className="text-xs text-gray-400">Successful</p>
                  </div>
                  <div className="text-center p-3 bg-navy-deep/50 rounded-lg">
                    <div className="text-2xl font-bold text-red-400 mb-1">
                      {pnlStats?.failed_analyses || 0}
                    </div>
                    <p className="text-xs text-gray-400">Failed</p>
                  </div>
                  <div className="text-center p-3 bg-navy-deep/50 rounded-lg">
                    <div className="text-2xl font-bold text-cyan-bright mb-1">
                      {pnlStats?.wallets_processed || 0}
                    </div>
                    <p className="text-xs text-gray-400">Processed</p>
                  </div>
                  <div className="text-center p-3 bg-navy-deep/50 rounded-lg">
                    <div className="text-2xl font-bold text-orange-warning mb-1">
                      {pnlStats?.wallets_in_progress || 0}
                    </div>
                    <p className="text-xs text-gray-400">In Progress</p>
                  </div>
                </div>
                {pnlStats?.last_activity && (
                  <div className="text-center text-sm text-gray-400">
                    Last Activity: {new Date(pnlStats.last_activity).toLocaleString()}
                  </div>
                )}
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      {/* Queue Monitoring Dashboard */}
      <Card className="glass-card border-blue-ice/20">
        <CardHeader>
          <CardTitle className="text-white flex items-center">
            <Timer className="w-5 h-5 mr-2" />
            Redis Queue Monitoring
          </CardTitle>
          <CardDescription className="text-gray-400">
            Real-time queue sizes and processing metrics
          </CardDescription>
        </CardHeader>
        <CardContent>
          {queueLoading ? (
            <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
              {Array.from({ length: 4 }).map((_, i) => (
                <div key={i} className="text-center">
                  <Skeleton className="h-8 w-16 mx-auto mb-2" />
                  <Skeleton className="h-4 w-24 mx-auto" />
                </div>
              ))}
            </div>
          ) : (
            <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
              <div className="text-center p-4 bg-navy-deep/50 rounded-lg border border-cyan-bright/20">
                <div className="text-3xl font-bold text-cyan-bright mb-2">
                  {queueMetrics?.discovery_queue_size || 0}
                </div>
                <div className="flex items-center justify-center space-x-1 text-sm text-gray-400">
                  <Eye className="w-4 h-4" />
                  <span>Discovery Queue</span>
                </div>
              </div>
              <div className="text-center p-4 bg-navy-deep/50 rounded-lg border border-orange-warning/20">
                <div className="text-3xl font-bold text-orange-warning mb-2">
                  {queueMetrics?.pnl_queue_size || 0}
                </div>
                <div className="flex items-center justify-center space-x-1 text-sm text-gray-400">
                  <BarChart3 className="w-4 h-4" />
                  <span>P&L Queue</span>
                </div>
              </div>
              <div className="text-center p-4 bg-navy-deep/50 rounded-lg border border-blue-steel/20">
                <div className="text-3xl font-bold text-blue-steel mb-2">
                  {queueMetrics?.recent_batch_jobs || 0}
                </div>
                <div className="flex items-center justify-center space-x-1 text-sm text-gray-400">
                  <Server className="w-4 h-4" />
                  <span>Recent Batches</span>
                </div>
              </div>
              <div className="text-center p-4 bg-navy-deep/50 rounded-lg border border-green-success/20">
                <div className="text-3xl font-bold text-green-success mb-2">
                  {queueMetrics?.total_pending || 0}
                </div>
                <div className="flex items-center justify-center space-x-1 text-sm text-gray-400">
                  <Activity className="w-4 h-4" />
                  <span>Total Pending</span>
                </div>
              </div>
            </div>
          )}
        </CardContent>
      </Card>

      {/* Component Health Status */}
      <Card className="glass-card border-blue-ice/20">
        <CardHeader>
          <CardTitle className="text-white">Component Health Status</CardTitle>
          <CardDescription className="text-gray-400">
            Detailed status of all system components
          </CardDescription>
        </CardHeader>
        <CardContent>
          {healthLoading ? (
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
              {Array.from({ length: 5 }).map((_, i) => (
                <div key={i} className="p-4 bg-navy-deep/50 rounded-lg">
                  <Skeleton className="h-6 w-32 mb-2" />
                  <Skeleton className="h-4 w-24" />
                </div>
              ))}
            </div>
          ) : (
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
              {health?.components && Object.entries(health.components).map(([name, component]: [string, any]) => (
                <div key={name} className="p-4 bg-navy-deep/50 rounded-lg border border-blue-ice/10">
                  <div className="flex items-center justify-between mb-3">
                    <div className="flex items-center space-x-2">
                      {getComponentIcon(name)}
                      <h3 className="font-medium text-white capitalize">
                        {name.replace('_', ' ')}
                      </h3>
                    </div>
                    {(component.connected !== undefined ? component.connected : 
                      component.accessible !== undefined ? component.accessible : 
                      component.status === 'Running' || 
                      (typeof component === 'string' && component === 'Running')) ? (
                      <CheckCircle className="w-5 h-5 text-green-success" />
                    ) : (
                      <AlertCircle className="w-5 h-5 text-red-400" />
                    )}
                  </div>
                  
                  <div className="space-y-1 text-sm">
                    {component.latency_ms && (
                      <p className="text-gray-400">
                        Latency: <span className="text-white">{component.latency_ms}ms</span>
                      </p>
                    )}
                    {(component.status || typeof component === 'string') && (
                      <p className="text-gray-400">
                        Status: <span className={
                          (component.status || component) === 'Running' ? 'text-green-success' : 'text-red-400'
                        }>
                          {component.status || component}
                        </span>
                      </p>
                    )}
                    {component.error && (
                      <p className="text-red-400 text-xs mt-2">
                        Error: {component.error}
                      </p>
                    )}
                  </div>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      {/* Enhanced Orchestrator Metrics */}
      <Card className="glass-card border-blue-ice/20">
        <CardHeader>
          <CardTitle className="text-white flex items-center">
            <Server className="w-5 h-5 mr-2" />
            Orchestrator Performance Metrics
          </CardTitle>
          <CardDescription className="text-gray-400">
            Detailed job processing and system performance statistics
          </CardDescription>
        </CardHeader>
        <CardContent>
          {orchestratorLoading ? (
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              {Array.from({ length: 8 }).map((_, i) => (
                <div key={i} className="text-center">
                  <Skeleton className="h-8 w-16 mx-auto mb-2" />
                  <Skeleton className="h-4 w-24 mx-auto" />
                </div>
              ))}
            </div>
          ) : (
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              <div className="text-center p-4 bg-navy-deep/50 rounded-lg">
                <div className="text-2xl font-bold text-cyan-bright mb-1">
                  {orchestrator?.running_jobs || 0}
                </div>
                <p className="text-sm text-gray-400">Running Jobs</p>
              </div>
              <div className="text-center p-4 bg-navy-deep/50 rounded-lg">
                <div className="text-2xl font-bold text-orange-warning mb-1">
                  {orchestrator?.pending_jobs || 0}
                </div>
                <p className="text-sm text-gray-400">Pending Jobs</p>
              </div>
              <div className="text-center p-4 bg-navy-deep/50 rounded-lg">
                <div className="text-2xl font-bold text-green-success mb-1">
                  {orchestrator?.successful_analyses || 0}
                </div>
                <p className="text-sm text-gray-400">Successful</p>
              </div>
              <div className="text-center p-4 bg-navy-deep/50 rounded-lg">
                <div className="text-2xl font-bold text-red-400 mb-1">
                  {orchestrator?.failed_analyses || 0}
                </div>
                <p className="text-sm text-gray-400">Failed</p>
              </div>
              <div className="text-center p-4 bg-navy-deep/50 rounded-lg">
                <div className="text-2xl font-bold text-blue-steel mb-1">
                  {orchestrator?.discovery_total || 0}
                </div>
                <p className="text-sm text-gray-400">Total Discovered</p>
              </div>
              <div className="text-center p-4 bg-navy-deep/50 rounded-lg">
                <div className="text-2xl font-bold text-purple-400 mb-1">
                  {orchestrator?.cycles_completed || 0}
                </div>
                <p className="text-sm text-gray-400">Discovery Cycles</p>
              </div>
              <div className="text-center p-4 bg-navy-deep/50 rounded-lg">
                <div className="text-2xl font-bold text-yellow-400 mb-1">
                  {orchestrator?.last_cycle_wallets || 0}
                </div>
                <p className="text-sm text-gray-400">Last Cycle</p>
              </div>
              <div className="text-center p-4 bg-navy-deep/50 rounded-lg">
                <div className="text-2xl font-bold text-teal-400 mb-1">
                  {orchestrator?.queue_size || 0}
                </div>
                <p className="text-sm text-gray-400">Queue Depth</p>
              </div>
            </div>
          )}
        </CardContent>
      </Card>

      {/* Enhanced System Logs */}
      <Card className="glass-card border-blue-ice/20">
        <CardHeader>
          <div className="flex items-center justify-between">
            <div>
              <CardTitle className="text-white">Real-Time System Logs</CardTitle>
              <div className="mt-2">
                <SystemHealthIndicator />
              </div>
              <CardDescription className="text-gray-400">
                Live application logs and system events
              </CardDescription>
            </div>
            <div className="flex items-center space-x-2">
              <Filter className="w-4 h-4 text-gray-400" />
              <div className="flex space-x-1">
                {(['ALL', 'ERROR', 'WARN', 'INFO', 'DEBUG'] as LogLevel[]).map((level) => (
                  <Button
                    key={level}
                    variant={logFilter === level ? 'neon' : 'ghost'}
                    size="sm"
                    onClick={() => setLogFilter(level)}
                    className="text-xs"
                  >
                    {level}
                  </Button>
                ))}
              </div>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          <div className="bg-navy-deep/50 rounded-lg p-4 font-mono text-sm h-96 overflow-y-auto">
            {logsLoading ? (
              <div className="space-y-2">
                {Array.from({ length: 15 }).map((_, i) => (
                  <Skeleton key={i} className="h-4 w-full" />
                ))}
              </div>
            ) : (
              <div className="space-y-1">
                {filteredLogs.length > 0 ? (
                  filteredLogs.map((log: LogEntry) => (
                    <div key={log.id} className="flex items-start space-x-3 py-1 hover:bg-gray-800/30 rounded px-2">
                      <span className="text-gray-500 text-xs whitespace-nowrap">
                        {new Date(log.timestamp).toLocaleTimeString()}
                      </span>
                      <span className={`text-xs font-bold w-12 ${getLogLevelColor(log.level)}`}>
                        {log.level}
                      </span>
                      <span className="text-blue-steel text-xs w-24 truncate">
                        {log.component}
                      </span>
                      <span className="text-gray-300 text-xs flex-1">
                        {log.message}
                      </span>
                    </div>
                  ))
                ) : (
                  <div className="text-center text-gray-500 py-8">
                    No logs found for filter: {logFilter}
                  </div>
                )}
              </div>
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  )
}