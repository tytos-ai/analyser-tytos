'use client'

import { useState, useMemo } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Skeleton } from '@/components/ui/skeleton'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { useToast } from '@/hooks/use-toast'
import { api } from '@/lib/api'
import { formatCurrency, formatNumber, truncateAddress, formatPercentage, getPnLColorClass, getWinRateColorClass } from '@/lib/utils'
import { TokenTabs } from '@/components/ui/token-tabs'
import { LoadingSpinner } from '@/components/ui/loading-spinner'
import { ChainBadge } from '@/components/ui/chain-badge'
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
  Eye,
  Wallet,
  Copy,
  ExternalLink,
  TrendingDown,
  DollarSign,
  BarChart3,
  PieChart,
  Zap,
  Target,
  Coins,
  AlertTriangle
} from 'lucide-react'
import { DiscoveredWalletsResponse, WalletDetailResponse } from '@/types/api'
import { motion, AnimatePresence } from 'framer-motion'

export default function DEX() {
  const [sortBy, setSortBy] = useState('discovered_at')
  const [sortOrder, setSortOrder] = useState<'asc' | 'desc'>('desc')
  const [minVolume, setMinVolume] = useState<number | undefined>()
  const [searchTerm, setSearchTerm] = useState('')
  const [showDetailModal, setShowDetailModal] = useState<string | null>(null)
  
  const { toast } = useToast()
  const queryClient = useQueryClient()

  // Fetch DEX service status with auto-refresh
  const { data: dexStatus, isLoading: statusLoading } = useQuery({
    queryKey: ['dex-status'],
    queryFn: api.dex.getStatus,
    refetchInterval: 8000 // Poll every 8 seconds
  })

  // Fetch discovered wallets with auto-refresh
  const { data: walletsData, isLoading: walletsLoading } = useQuery<DiscoveredWalletsResponse>({
    queryKey: ['discovered-wallets', sortBy, sortOrder, minVolume],
    queryFn: () => api.dex.getDiscoveredWallets({
      sort_by: sortBy,
      order: sortOrder,
      min_volume: minVolume,
      limit: 50
    }),
    refetchInterval: 6000 // Poll every 6 seconds
  })

  // Get selected wallet data
  const selectedWalletData = walletsData?.wallets.find(w => w.wallet_address === showDetailModal)

  // Fetch detailed portfolio data when modal opens
  const { data: portfolioDetailData, isLoading: isLoadingPortfolio } = useQuery<WalletDetailResponse>({
    queryKey: ['walletDetail', showDetailModal, selectedWalletData?.chain],
    queryFn: () => api.results.getWalletDetail(
      showDetailModal!, 
      'portfolio',
      selectedWalletData?.chain
    ),
    enabled: !!showDetailModal && !!selectedWalletData,
    staleTime: 5 * 60 * 1000, // 5 minutes
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


  const handleServiceControl = (action: 'start' | 'stop' | 'restart') => {
    controlMutation.mutate(action)
  }

  const handleWalletClick = (walletAddress: string) => {
    setShowDetailModal(walletAddress)
  }

  const formatHoldTime = (minutes: number) => {
    if (minutes < 60) return `${Math.round(minutes)}m`
    const hours = Math.floor(minutes / 60)
    const mins = Math.round(minutes % 60)
    return mins > 0 ? `${hours}h ${mins}m` : `${hours}h`
  }

  const getExplorerUrl = (address: string, chain: string = 'solana') => {
    switch (chain) {
      case 'ethereum':
        return `https://etherscan.io/address/${address}`
      case 'bsc':
        return `https://bscscan.com/address/${address}`
      case 'base':
        return `https://basescan.org/address/${address}`
      default:
        return `https://solscan.io/account/${address}`
    }
  }

  const copyToClipboard = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text)
      toast({
        title: "Copied!",
        description: "Address copied to clipboard",
      })
    } catch (err) {
      console.error('Failed to copy text: ', err)
    }
  }

  const filteredWallets = walletsData?.wallets.filter(wallet =>
    wallet.wallet_address.toLowerCase().includes(searchTerm.toLowerCase()) ||
    wallet.chain.toLowerCase().includes(searchTerm.toLowerCase())
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
                  placeholder="Search by wallet address or chain..."
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
                  <th className="text-left py-3 px-4 font-medium text-gray-400 w-32">Chain</th>
                  <th className="text-left py-3 px-4 font-medium text-gray-400">P&L</th>
                  <th className="text-left py-3 px-4 font-medium text-gray-400">Win Rate</th>
                  <th className="text-left py-3 px-4 font-medium text-gray-400">Avg Hold Time</th>
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
                      <td className="py-3 px-4"><Skeleton className="h-4 w-20" /></td>
                      <td className="py-3 px-4"><Skeleton className="h-4 w-24" /></td>
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
                      <td className="py-3 px-4 w-32">
                        <div className="inline-flex">
                          <ChainBadge chain={wallet.chain} />
                        </div>
                      </td>
                      <td className="py-3 px-4">
                        <div className={`text-white font-medium ${wallet.pnl_usd ? getPnLColorClass(wallet.pnl_usd) : 'text-gray-400'}`}>
                          {wallet.pnl_usd ? formatCurrency(wallet.pnl_usd) : 'N/A'}
                        </div>
                      </td>
                      <td className="py-3 px-4">
                        <div className="text-gray-300 text-sm">
                          {wallet.win_rate ? `${(wallet.win_rate * 100).toFixed(1)}%` : 'N/A'}
                        </div>
                      </td>
                      <td className="py-3 px-4">
                        <div className="text-gray-300 text-sm">
                          {wallet.avg_hold_time_minutes ? formatHoldTime(wallet.avg_hold_time_minutes) : 'N/A'}
                        </div>
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

      {/* Wallet Detail Modal */}
      <AnimatePresence>
        {showDetailModal && selectedWalletData && (
          <Dialog open={!!showDetailModal} onOpenChange={() => setShowDetailModal(null)}>
            <DialogContent className="max-w-5xl max-h-[90vh] overflow-y-auto">
              <DialogHeader>
                <DialogTitle className="text-xl font-bold text-white flex items-center space-x-2">
                  <Wallet className="w-6 h-6 text-cyan-bright" />
                  <span>Wallet Analysis Details</span>
                </DialogTitle>
                <DialogDescription className="text-gray-400">
                  Comprehensive P&L analysis for {truncateAddress(selectedWalletData.wallet_address)}
                </DialogDescription>
              </DialogHeader>
              
              <div className="space-y-6">
                {isLoadingPortfolio ? (
                  <div className="flex items-center justify-center py-12">
                    <LoadingSpinner size="lg" />
                  </div>
                ) : portfolioDetailData ? (
                  <>
                    {/* Portfolio Overview Section */}
                    <motion.div
                      initial={{ opacity: 0, y: 20 }}
                      animate={{ opacity: 1, y: 0 }}
                      transition={{ duration: 0.4 }}
                    >
                      <Card className="glass-card border-blue-ice/20">
                        <CardHeader>
                          <div className="flex items-center justify-between">
                            <CardTitle className="text-lg font-bold text-white flex items-center gap-2">
                              <PieChart className="w-5 h-5 text-cyan-bright" />
                              Portfolio Overview
                            </CardTitle>
                            <div className="flex items-center gap-2">
                              <ChainBadge chain={portfolioDetailData.chain} />
                              <Badge variant="outline" className="font-normal">
                                {portfolioDetailData.portfolio_result.tokens_analyzed} tokens
                              </Badge>
                            </div>
                          </div>
                        </CardHeader>
                        <CardContent>
                          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                            {/* Left side - Wallet info and P&L */}
                            <div className="space-y-4">
                              <div className="flex items-center justify-between p-3 bg-navy-deep/50 rounded-lg">
                                <span className="text-sm text-gray-400">Wallet Address</span>
                                <div className="flex items-center gap-2">
                                  <span className="font-mono text-sm text-white">
                                    {truncateAddress(portfolioDetailData.wallet_address)}
                                  </span>
                                  <Button
                                    variant="ghost"
                                    size="sm"
                                    onClick={() => copyToClipboard(portfolioDetailData.wallet_address)}
                                    className="h-6 w-6 p-0"
                                  >
                                    <Copy className="w-3 h-3" />
                                  </Button>
                                  <Button
                                    variant="ghost"
                                    size="sm"
                                    onClick={() => window.open(getExplorerUrl(portfolioDetailData.wallet_address, portfolioDetailData.chain), '_blank')}
                                    className="h-6 w-6 p-0"
                                  >
                                    <ExternalLink className="w-3 h-3" />
                                  </Button>
                                </div>
                              </div>

                              <div className="space-y-3">
                                <div className="flex items-center justify-between">
                                  <div className="flex items-center gap-2">
                                    <DollarSign className="w-4 h-4 text-green-success" />
                                    <span className="text-sm text-gray-400">Total P&L</span>
                                  </div>
                                  <span className={`text-xl font-bold ${
                                    getPnLColorClass(portfolioDetailData.portfolio_result.total_pnl_usd.toString())
                                  }`}>
                                    {formatCurrency(parseFloat(portfolioDetailData.portfolio_result.total_pnl_usd.toString()))}
                                  </span>
                                </div>

                                <div className="flex items-center justify-between">
                                  <div className="flex items-center gap-2">
                                    <Zap className="w-4 h-4 text-cyan-bright" />
                                    <span className="text-sm text-gray-400">Realized</span>
                                  </div>
                                  <span className={`font-semibold ${
                                    getPnLColorClass(portfolioDetailData.portfolio_result.total_realized_pnl_usd.toString())
                                  }`}>
                                    {formatCurrency(parseFloat(portfolioDetailData.portfolio_result.total_realized_pnl_usd.toString()))}
                                  </span>
                                </div>

                                <div className="flex items-center justify-between">
                                  <div className="flex items-center gap-2">
                                    <Clock className="w-4 h-4 text-orange-warning" />
                                    <span className="text-sm text-gray-400">Unrealized</span>
                                  </div>
                                  <span className={`font-semibold ${
                                    getPnLColorClass(portfolioDetailData.portfolio_result.total_unrealized_pnl_usd.toString())
                                  }`}>
                                    {formatCurrency(parseFloat(portfolioDetailData.portfolio_result.total_unrealized_pnl_usd.toString()))}
                                  </span>
                                </div>
                              </div>
                            </div>

                            {/* Right side - Trading metrics */}
                            <div className="space-y-4">
                              <div className="grid grid-cols-2 gap-3">
                                <div className="p-3 bg-navy-deep/50 rounded-lg">
                                  <div className="flex items-center gap-2 mb-1">
                                    <Target className="w-4 h-4 text-purple-400" />
                                    <span className="text-sm text-gray-400">Win Rate</span>
                                  </div>
                                  <p className={`text-lg font-bold ${
                                    getWinRateColorClass(portfolioDetailData.portfolio_result.overall_win_rate_percentage.toString())
                                  }`}>
                                    {formatPercentage(parseFloat(portfolioDetailData.portfolio_result.overall_win_rate_percentage.toString()))}
                                  </p>
                                </div>

                                <div className="p-3 bg-navy-deep/50 rounded-lg">
                                  <div className="flex items-center gap-2 mb-1">
                                    <Activity className="w-4 h-4 text-blue-steel" />
                                    <span className="text-sm text-gray-400">Total Trades</span>
                                  </div>
                                  <p className="text-lg font-bold text-white">
                                    {portfolioDetailData.portfolio_result.total_trades}
                                  </p>
                                  <div className="flex gap-2 text-xs mt-1">
                                    <span className="text-green-success">
                                      W: {portfolioDetailData.portfolio_result.winning_trades ?? Math.round(portfolioDetailData.portfolio_result.total_trades * parseFloat(portfolioDetailData.portfolio_result.overall_win_rate_percentage.toString()) / 100)}
                                    </span>
                                    <span className="text-red-400">
                                      L: {portfolioDetailData.portfolio_result.losing_trades ?? (portfolioDetailData.portfolio_result.total_trades - Math.round(portfolioDetailData.portfolio_result.total_trades * parseFloat(portfolioDetailData.portfolio_result.overall_win_rate_percentage.toString()) / 100))}
                                    </span>
                                  </div>
                                </div>

                                <div className="p-3 bg-navy-deep/50 rounded-lg">
                                  <div className="flex items-center gap-2 mb-1">
                                    <Clock className="w-4 h-4 text-cyan-bright" />
                                    <span className="text-sm text-gray-400">Avg Hold</span>
                                  </div>
                                  <p className="text-lg font-bold text-white">
                                    {formatHoldTime(parseFloat(portfolioDetailData.portfolio_result.avg_hold_time_minutes.toString()))}
                                  </p>
                                </div>

                                <div className="p-3 bg-navy-deep/50 rounded-lg">
                                  <div className="flex items-center gap-2 mb-1">
                                    <Coins className="w-4 h-4 text-orange-warning" />
                                    <span className="text-sm text-gray-400">Tokens</span>
                                  </div>
                                  <p className="text-lg font-bold text-white">
                                    {portfolioDetailData.portfolio_result.tokens_analyzed}
                                  </p>
                                </div>
                              </div>

                              <div className="grid grid-cols-2 gap-3">
                                <div className="p-3 bg-navy-deep/50 rounded-lg">
                                  <p className="text-xs text-gray-400 mb-1">Total Invested</p>
                                  <p className="text-sm font-semibold text-white">
                                    {portfolioDetailData.portfolio_result.total_invested_usd && !isNaN(parseFloat(portfolioDetailData.portfolio_result.total_invested_usd.toString()))
                                      ? formatCurrency(parseFloat(portfolioDetailData.portfolio_result.total_invested_usd.toString()))
                                      : 'N/A'
                                    }
                                  </p>
                                </div>
                                <div className="p-3 bg-navy-deep/50 rounded-lg">
                                  <p className="text-xs text-gray-400 mb-1">Total Returned</p>
                                  <p className="text-sm font-semibold text-white">
                                    {portfolioDetailData.portfolio_result.total_returned_usd && !isNaN(parseFloat(portfolioDetailData.portfolio_result.total_returned_usd.toString()))
                                      ? formatCurrency(parseFloat(portfolioDetailData.portfolio_result.total_returned_usd.toString()))
                                      : 'N/A'
                                    }
                                  </p>
                                </div>
                              </div>

                              <div className="grid grid-cols-2 gap-3 mt-3">
                                <div className="p-3 bg-navy-deep/50 rounded-lg">
                                  <p className="text-xs text-gray-400 mb-1">Profit %</p>
                                  <p className={`text-sm font-semibold ${
                                    getPnLColorClass(portfolioDetailData.portfolio_result.profit_percentage || '0')
                                  }`}>
                                    {portfolioDetailData.portfolio_result.profit_percentage 
                                      ? `${parseFloat(portfolioDetailData.portfolio_result.profit_percentage.toString()) >= 0 ? '+' : ''}${portfolioDetailData.portfolio_result.profit_percentage}%`
                                      : 'N/A'
                                    }
                                  </p>
                                </div>
                                <div className="p-3 bg-navy-deep/50 rounded-lg">
                                  <p className="text-xs text-gray-400 mb-1">Streaks</p>
                                  <div className="text-xs">
                                    <span className="text-green-success">W: {portfolioDetailData.portfolio_result.current_winning_streak ?? 0}/{portfolioDetailData.portfolio_result.longest_winning_streak ?? 0}</span>
                                    <span className="text-gray-400"> | </span>
                                    <span className="text-red-400">L: {portfolioDetailData.portfolio_result.current_losing_streak ?? 0}/{portfolioDetailData.portfolio_result.longest_losing_streak ?? 0}</span>
                                  </div>
                                </div>
                              </div>
                            </div>
                          </div>
                        </CardContent>
                      </Card>
                    </motion.div>

                    <div className="w-full h-px bg-blue-ice/20" />

                    {/* Token Tabs Section */}
                    {portfolioDetailData.portfolio_result.token_results && portfolioDetailData.portfolio_result.token_results.length > 0 && (
                      <motion.div
                        initial={{ opacity: 0, y: 20 }}
                        animate={{ opacity: 1, y: 0 }}
                        transition={{ duration: 0.4, delay: 0.1 }}
                      >
                        <h3 className="text-lg font-semibold text-white mb-4 flex items-center gap-2">
                          <Coins className="w-5 h-5 text-cyan-bright" />
                          Token-by-Token Breakdown
                        </h3>
                        <TokenTabs tokenResults={portfolioDetailData.portfolio_result.token_results} />
                      </motion.div>
                    )}
                  </>
                ) : (
                  <div className="text-center py-8">
                    <AlertTriangle className="w-12 h-12 text-orange-warning mx-auto mb-4" />
                    <p className="text-gray-400">No detailed data available for this wallet</p>
                  </div>
                )}
              </div>

              <DialogFooter className="flex items-center space-x-2">
                <Button 
                  variant="outline"
                  onClick={() => window.open(getExplorerUrl(selectedWalletData.wallet_address, selectedWalletData.chain), '_blank')}
                  className="border-blue-steel/50 text-blue-steel hover:bg-blue-steel/20"
                >
                  <ExternalLink className="w-4 h-4 mr-2" />
                  View on Explorer
                </Button>
                <Button 
                  variant="outline"
                  onClick={() => window.open(`https://gmgn.ai/sol/address/${selectedWalletData.wallet_address}`, '_blank')}
                  className="border-green-success/50 text-green-success hover:bg-green-success/20"
                >
                  <BarChart3 className="w-4 h-4 mr-2" />
                  View on GMGN
                </Button>
                <Button 
                  variant="outline"
                  onClick={() => copyToClipboard(selectedWalletData.wallet_address)}
                  className="border-orange-warning/50 text-orange-warning hover:bg-orange-warning/20"
                >
                  <Copy className="w-4 h-4 mr-2" />
                  Copy Address
                </Button>
                <Button 
                  onClick={() => setShowDetailModal(null)}
                  className="bg-cyan-bright/10 border-cyan-bright/20 text-cyan-bright hover:bg-cyan-bright/20"
                >
                  Close
                </Button>
              </DialogFooter>
            </DialogContent>
          </Dialog>
        )}
      </AnimatePresence>
    </div>
  )
}