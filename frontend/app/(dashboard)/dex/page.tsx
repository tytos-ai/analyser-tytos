'use client'

import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useRouter } from 'next/navigation'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Skeleton } from '@/components/ui/skeleton'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { useToast } from '@/hooks/use-toast'
import { api } from '@/lib/api'
import { formatCurrency, formatNumber, truncateAddress } from '@/lib/utils'
import { 
  Play, 
  Square, 
  RotateCcw, 
  Activity, 
  Wifi, 
  WifiOff, 
  TrendingUp, 
  Users, 
  Clock,
  Search,
  Filter,
  RefreshCw,
  AlertCircle,
  CheckCircle,
  Eye
} from 'lucide-react'

interface DiscoveredWallet {
  wallet_address: string
  token_symbol: string
  trader_volume_usd: number
  discovery_reason: string
  discovered_at: string
  analysis_status: 'pending' | 'processing' | 'completed' | 'failed'
}

export default function DEX() {
  const [sortBy, setSortBy] = useState('discovered_at')
  const [sortOrder, setSortOrder] = useState<'asc' | 'desc'>('desc')
  const [minVolume, setMinVolume] = useState<number | undefined>()
  const [searchTerm, setSearchTerm] = useState('')
  
  const { toast } = useToast()
  const queryClient = useQueryClient()
  const router = useRouter()

  // Fetch DEX service status with auto-refresh
  const { data: dexStatus, isLoading: statusLoading } = useQuery({
    queryKey: ['dex-status'],
    queryFn: api.dex.getStatus,
    refetchInterval: 8000 // Poll every 8 seconds
  })

  // Fetch discovered wallets with auto-refresh
  const { data: walletsData, isLoading: walletsLoading } = useQuery({
    queryKey: ['discovered-wallets', sortBy, sortOrder, minVolume],
    queryFn: () => api.dex.getDiscoveredWallets({
      sort_by: sortBy,
      order: sortOrder,
      min_volume: minVolume,
      limit: 50
    }),
    refetchInterval: 6000 // Poll every 6 seconds
  })

  // Service control mutation
  const controlMutation = useMutation({
    mutationFn: api.dex.control,
    onSuccess: (data, variables) => {
      if (data.success) {
        toast({
          title: "Success",
          description: data.message,
        })
        queryClient.invalidateQueries({ queryKey: ['dex-status'] })
      } else {
        toast({
          title: "Error",
          description: data.message,
          variant: "destructive",
        })
      }
    },
    onError: () => {
      toast({
        title: "Error",
        description: "Failed to control DEX service",
        variant: "destructive",
      })
    }
  })

  const getServiceStatusColor = (status: string) => {
    switch (status) {
      case 'Running':
        return 'bg-green-success/20 text-green-success border-green-success/30'
      case 'Stopped':
        return 'bg-gray-500/20 text-gray-400 border-gray-500/30'
      case 'Error':
        return 'bg-red-500/20 text-red-400 border-red-500/30'
      default:
        return 'bg-gray-500/20 text-gray-400 border-gray-500/30'
    }
  }

  const getConnectionIcon = (status: string) => {
    return status === 'Connected' ? (
      <Wifi className="w-4 h-4 text-green-success" />
    ) : (
      <WifiOff className="w-4 h-4 text-red-400" />
    )
  }

  const getAnalysisStatusBadge = (status: string) => {
    switch (status) {
      case 'completed':
        return <Badge className="bg-green-success/20 text-green-success border-green-success/30">Completed</Badge>
      case 'processing':
        return <Badge className="bg-cyan-bright/20 text-cyan-bright border-cyan-bright/30">Processing</Badge>
      case 'pending':
        return <Badge className="bg-orange-warning/20 text-orange-warning border-orange-warning/30">Pending</Badge>
      case 'failed':
        return <Badge className="bg-red-500/20 text-red-400 border-red-500/30">Failed</Badge>
      default:
        return <Badge className="bg-gray-500/20 text-gray-400 border-gray-500/30">Unknown</Badge>
    }
  }

  const handleServiceControl = (action: 'start' | 'stop' | 'restart') => {
    controlMutation.mutate(action)
  }

  const handleWalletClick = (walletAddress: string) => {
    router.push(`/wallet/${walletAddress}`)
  }

  const filteredWallets = walletsData?.discovered_wallets.filter(wallet =>
    wallet.wallet_address.toLowerCase().includes(searchTerm.toLowerCase()) ||
    wallet.token_symbol.toLowerCase().includes(searchTerm.toLowerCase()) ||
    wallet.discovery_reason.toLowerCase().includes(searchTerm.toLowerCase())
  ) || []

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold text-white mb-2">DEX Discovery Monitor</h1>
          <p className="text-gray-400">Real-time wallet discovery and token monitoring</p>
        </div>
        <div className="flex items-center space-x-2 text-sm text-gray-400">
          <RefreshCw className="w-4 h-4 animate-spin" />
          <span>Live Feed</span>
        </div>
      </div>

      {/* Component 1: DEX Service Status & Control Panel */}
      <Card className="glass-card border-blue-ice/20">
        <CardHeader>
          <CardTitle className="text-white flex items-center">
            <Activity className="w-5 h-5 mr-2 text-cyan-bright" />
            DEX Discovery Service Control Panel
          </CardTitle>
          <CardDescription className="text-gray-400">
            Monitor and control the wallet discovery service
          </CardDescription>
        </CardHeader>
        <CardContent>
          {statusLoading ? (
            <div className="space-y-4">
              <Skeleton className="h-16 w-full" />
              <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
                {Array.from({ length: 4 }).map((_, i) => (
                  <Skeleton key={i} className="h-20" />
                ))}
              </div>
            </div>
          ) : (
            <div className="space-y-6">
              {/* Service Status and Controls */}
              <div className="flex items-center justify-between p-4 bg-navy-deep/50 rounded-lg">
                <div className="flex items-center space-x-4">
                  <div className="flex items-center space-x-2">
                    {dexStatus?.service_status === 'Running' ? (
                      <CheckCircle className="w-6 h-6 text-green-success" />
                    ) : (
                      <AlertCircle className="w-6 h-6 text-red-400" />
                    )}
                    <span className="text-lg font-semibold text-white">Service Status:</span>
                  </div>
                  <Badge className={`text-lg px-4 py-2 ${getServiceStatusColor(dexStatus?.service_status || 'Unknown')}`}>
                    {dexStatus?.service_status || 'Unknown'}
                  </Badge>
                </div>
                
                <div className="flex items-center space-x-2">
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={dexStatus?.service_status === 'Running' || controlMutation.isLoading}
                    onClick={() => handleServiceControl('start')}
                    className="border-green-success/50 text-green-success hover:bg-green-success/20"
                  >
                    <Play className="w-4 h-4 mr-2" />
                    Start
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={dexStatus?.service_status === 'Stopped' || controlMutation.isLoading}
                    onClick={() => handleServiceControl('stop')}
                    className="border-red-500/50 text-red-400 hover:bg-red-500/20"
                  >
                    <Square className="w-4 h-4 mr-2" />
                    Stop
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={dexStatus?.service_status === 'Stopped' || controlMutation.isLoading}
                    onClick={() => handleServiceControl('restart')}
                    className="border-orange-warning/50 text-orange-warning hover:bg-orange-warning/20"
                  >
                    <RotateCcw className="w-4 h-4 mr-2" />
                    Restart
                  </Button>
                </div>
              </div>

              {/* Connection Health */}
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div className="p-4 bg-navy-deep/50 rounded-lg">
                  <div className="flex items-center justify-between mb-2">
                    <h3 className="font-medium text-white">DexScreener Connection</h3>
                    {getConnectionIcon(dexStatus?.connections.dexscreener.status || 'Disconnected')}
                  </div>
                  <div className="text-sm text-gray-400 space-y-1">
                    <p>Status: <span className="text-white">{dexStatus?.connections.dexscreener.status}</span></p>
                    <p>Latency: <span className="text-white">{dexStatus?.connections.dexscreener.latency_ms}ms</span></p>
                    <p>Last Update: <span className="text-white">
                      {dexStatus?.connections.dexscreener.last_update ? 
                        new Date(dexStatus.connections.dexscreener.last_update).toLocaleTimeString() : 'N/A'}
                    </span></p>
                  </div>
                </div>

                <div className="p-4 bg-navy-deep/50 rounded-lg">
                  <div className="flex items-center justify-between mb-2">
                    <h3 className="font-medium text-white">BirdEye Connection</h3>
                    {getConnectionIcon(dexStatus?.connections.birdeye.status || 'Disconnected')}
                  </div>
                  <div className="text-sm text-gray-400 space-y-1">
                    <p>Status: <span className="text-white">{dexStatus?.connections.birdeye.status}</span></p>
                    <p>Latency: <span className="text-white">{dexStatus?.connections.birdeye.latency_ms}ms</span></p>
                    <p>Last Update: <span className="text-white">
                      {dexStatus?.connections.birdeye.last_update ? 
                        new Date(dexStatus.connections.birdeye.last_update).toLocaleTimeString() : 'N/A'}
                    </span></p>
                  </div>
                </div>
              </div>

              {/* Discovery Statistics */}
              <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
                <div className="text-center p-4 bg-navy-deep/50 rounded-lg">
                  <div className="text-2xl font-bold text-cyan-bright mb-1">
                    {dexStatus?.discovery_stats.trending_tokens_found || 0}
                  </div>
                  <p className="text-sm text-gray-400">Trending Tokens Found</p>
                </div>
                <div className="text-center p-4 bg-navy-deep/50 rounded-lg">
                  <div className="text-2xl font-bold text-green-success mb-1">
                    {dexStatus?.discovery_stats.wallets_discovered || 0}
                  </div>
                  <p className="text-sm text-gray-400">Wallets Discovered</p>
                </div>
                <div className="text-center p-4 bg-navy-deep/50 rounded-lg">
                  <div className="text-2xl font-bold text-orange-warning mb-1">
                    {dexStatus?.discovery_stats.queue_size || 0}
                  </div>
                  <p className="text-sm text-gray-400">Queue Size</p>
                </div>
                <div className="text-center p-4 bg-navy-deep/50 rounded-lg">
                  <div className="text-2xl font-bold text-blue-steel mb-1">
                    {dexStatus?.discovery_stats.discovery_rate_per_hour || 0}
                  </div>
                  <p className="text-sm text-gray-400">Discovery Rate/Hour</p>
                </div>
              </div>
            </div>
          )}
        </CardContent>
      </Card>

      {/* Component 2: Discovered Wallets Feed */}
      <Card className="glass-card border-blue-ice/20">
        <CardHeader>
          <div className="flex items-center justify-between">
            <div>
              <CardTitle className="text-white flex items-center">
                <Users className="w-5 h-5 mr-2 text-cyan-bright" />
                Discovered Wallets Feed
              </CardTitle>
              <CardDescription className="text-gray-400">
                Real-time wallet discoveries from DEX monitoring
              </CardDescription>
            </div>
            <div className="flex items-center space-x-2">
              <div className="w-2 h-2 bg-green-success rounded-full animate-pulse" />
              <span className="text-sm text-gray-400">Live</span>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          {/* Filters and Search */}
          <div className="flex flex-col md:flex-row gap-4 mb-6">
            <div className="flex-1">
              <div className="relative">
                <Search className="absolute left-3 top-1/2 transform -translate-y-1/2 text-gray-400 w-4 h-4" />
                <Input
                  placeholder="Search wallets, tokens, or discovery reasons..."
                  value={searchTerm}
                  onChange={(e) => setSearchTerm(e.target.value)}
                  className="pl-10 bg-navy-deep/50 border-blue-ice/20 text-white"
                />
              </div>
            </div>
            
            <div className="flex items-center space-x-4">
              <div className="flex items-center space-x-2">
                <Label htmlFor="sort-by" className="text-white text-sm">Sort by:</Label>
                <Select value={sortBy} onValueChange={setSortBy}>
                  <SelectTrigger className="w-32 bg-navy-deep/50 border-blue-ice/20 text-white">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="discovered_at">Time</SelectItem>
                    <SelectItem value="volume">Volume</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              
              <div className="flex items-center space-x-2">
                <Label htmlFor="sort-order" className="text-white text-sm">Order:</Label>
                <Select value={sortOrder} onValueChange={(value: 'asc' | 'desc') => setSortOrder(value)}>
                  <SelectTrigger className="w-24 bg-navy-deep/50 border-blue-ice/20 text-white">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="desc">Desc</SelectItem>
                    <SelectItem value="asc">Asc</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              
              <div className="flex items-center space-x-2">
                <Label htmlFor="min-volume" className="text-white text-sm">Min Volume:</Label>
                <Input
                  id="min-volume"
                  type="number"
                  placeholder="10000"
                  value={minVolume || ''}
                  onChange={(e) => setMinVolume(e.target.value ? parseInt(e.target.value) : undefined)}
                  className="w-24 bg-navy-deep/50 border-blue-ice/20 text-white"
                />
              </div>
            </div>
          </div>

          {/* Wallets Table */}
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead>
                <tr className="border-b border-blue-ice/20">
                  <th className="text-left py-3 px-4 font-medium text-gray-400">Wallet Address</th>
                  <th className="text-left py-3 px-4 font-medium text-gray-400">Token</th>
                  <th className="text-left py-3 px-4 font-medium text-gray-400">Volume</th>
                  <th className="text-left py-3 px-4 font-medium text-gray-400">Discovery Reason</th>
                  <th className="text-left py-3 px-4 font-medium text-gray-400">Discovered</th>
                  <th className="text-left py-3 px-4 font-medium text-gray-400">Analysis Status</th>
                  <th className="text-left py-3 px-4 font-medium text-gray-400">Actions</th>
                </tr>
              </thead>
              <tbody>
                {walletsLoading ? (
                  Array.from({ length: 10 }).map((_, i) => (
                    <tr key={i} className="border-b border-blue-ice/10">
                      <td className="py-3 px-4"><Skeleton className="h-4 w-32" /></td>
                      <td className="py-3 px-4"><Skeleton className="h-4 w-16" /></td>
                      <td className="py-3 px-4"><Skeleton className="h-4 w-24" /></td>
                      <td className="py-3 px-4"><Skeleton className="h-4 w-28" /></td>
                      <td className="py-3 px-4"><Skeleton className="h-4 w-20" /></td>
                      <td className="py-3 px-4"><Skeleton className="h-6 w-20" /></td>
                      <td className="py-3 px-4"><Skeleton className="h-8 w-16" /></td>
                    </tr>
                  ))
                ) : (
                  filteredWallets.map((wallet, i) => (
                    <tr key={i} className="border-b border-blue-ice/10 hover:bg-navy-deep/30 transition-colors">
                      <td className="py-3 px-4">
                        <button
                          onClick={() => handleWalletClick(wallet.wallet_address)}
                          className="font-mono text-sm text-cyan-bright hover:text-cyan-bright/80 transition-colors"
                        >
                          {truncateAddress(wallet.wallet_address)}
                        </button>
                      </td>
                      <td className="py-3 px-4">
                        <Badge className="bg-blue-steel/20 text-blue-steel border-blue-steel/30">
                          {wallet.token_symbol}
                        </Badge>
                      </td>
                      <td className="py-3 px-4">
                        <div className="text-white font-medium">
                          {formatCurrency(wallet.trader_volume_usd)}
                        </div>
                      </td>
                      <td className="py-3 px-4">
                        <div className="text-gray-300 text-sm">
                          {wallet.discovery_reason}
                        </div>
                      </td>
                      <td className="py-3 px-4">
                        <div className="text-gray-400 text-sm">
                          {new Date(wallet.discovered_at).toLocaleTimeString()}
                        </div>
                      </td>
                      <td className="py-3 px-4">
                        {getAnalysisStatusBadge(wallet.analysis_status)}
                      </td>
                      <td className="py-3 px-4">
                        <Button 
                          variant="ghost" 
                          size="sm" 
                          onClick={() => handleWalletClick(wallet.wallet_address)}
                          className="text-cyan-bright hover:bg-cyan-bright/20"
                        >
                          <Eye className="w-4 h-4" />
                        </Button>
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>

          {filteredWallets.length === 0 && !walletsLoading && (
            <div className="text-center py-8">
              <Users className="w-12 h-12 text-gray-400 mx-auto mb-4" />
              <h3 className="text-lg font-medium text-white mb-2">No Wallets Found</h3>
              <p className="text-gray-400">
                {searchTerm ? 'No wallets match your search criteria.' : 'No wallets have been discovered yet.'}
              </p>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  )
}