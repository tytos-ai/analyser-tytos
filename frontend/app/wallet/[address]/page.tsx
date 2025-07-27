'use client'

import { useQuery } from '@tanstack/react-query'
import { useParams } from 'next/navigation'
import { useState } from 'react'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Progress } from '@/components/ui/progress'
import { Skeleton } from '@/components/ui/skeleton'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { formatCurrency, formatPercentage, truncateAddress } from '@/lib/utils'
import { 
  Wallet, 
  TrendingUp, 
  TrendingDown, 
  Activity, 
  Clock, 
  Target,
  Shield,
  AlertTriangle,
  BarChart3,
  TrendingUp as TrendingUpIcon,
  DollarSign,
  Briefcase
} from 'lucide-react'

interface WalletAnalysis {
  wallet_address: string
  metadata: {
    quality_score: number
  }
  portfolio_result: {
    total_pnl_usd: number
    overall_win_rate_percentage: number
    total_trades: number
    avg_hold_time_minutes: number
  }
  copy_trading_metrics: {
    trading_style: string
    consistency_score: number
    risk_metrics: {
      max_position_percentage: number
      diversification_score: number
      max_consecutive_losses: number
    }
  }
}

interface WalletTrade {
  trade_id: string
  token_symbol: string
  trade_type: 'buy' | 'sell'
  amount_usd: number
  price_usd: number
  timestamp: string
  pnl_usd: number
  performance_category: 'excellent' | 'good' | 'poor' | 'loss'
}

interface WalletPosition {
  token_address: string
  token_symbol: string
  balance: number
  current_value_usd: number
  unrealized_pnl_usd: number
  risk_level: 'low' | 'medium' | 'high'
  last_updated: string
}

// Mock API function - replace with actual API call
const fetchWalletAnalysis = async (walletAddress: string): Promise<WalletAnalysis> => {
  await new Promise(resolve => setTimeout(resolve, 1500)) // Simulate API delay
  
  return {
    wallet_address: walletAddress,
    metadata: {
      quality_score: 87
    },
    portfolio_result: {
      total_pnl_usd: 234567.89,
      overall_win_rate_percentage: 68.5,
      total_trades: 1247,
      avg_hold_time_minutes: 145
    },
    copy_trading_metrics: {
      trading_style: "Swing Trader",
      consistency_score: 0.82,
      risk_metrics: {
        max_position_percentage: 15.3,
        diversification_score: 7.2,
        max_consecutive_losses: 4
      }
    }
  }
}

const fetchWalletTrades = async (walletAddress: string): Promise<WalletTrade[]> => {
  await new Promise(resolve => setTimeout(resolve, 1000))
  
  const tradeTypes = ['buy', 'sell'] as const
  const tokens = ['BONK', 'WIF', 'POPCAT', 'MYRO', 'JUP', 'ORCA']
  const categories = ['excellent', 'good', 'poor', 'loss'] as const
  
  return Array.from({ length: 20 }, (_, i) => ({
    trade_id: `trade_${i + 1}_${Math.random().toString(36).substr(2, 8)}`,
    token_symbol: tokens[Math.floor(Math.random() * tokens.length)],
    trade_type: tradeTypes[Math.floor(Math.random() * tradeTypes.length)],
    amount_usd: Math.floor(Math.random() * 50000) + 1000,
    price_usd: Math.random() * 10,
    timestamp: new Date(Date.now() - i * 3600000).toISOString(),
    pnl_usd: Math.floor(Math.random() * 20000) - 10000,
    performance_category: categories[Math.floor(Math.random() * categories.length)]
  }))
}

const fetchWalletPositions = async (walletAddress: string): Promise<WalletPosition[]> => {
  await new Promise(resolve => setTimeout(resolve, 800))
  
  const tokens = ['BONK', 'WIF', 'POPCAT', 'MYRO', 'JUP', 'ORCA', 'RAY', 'BOME']
  const riskLevels = ['low', 'medium', 'high'] as const
  
  return Array.from({ length: 8 }, (_, i) => ({
    token_address: `${Math.random().toString(36).substr(2, 8)}...${Math.random().toString(36).substr(2, 4)}`,
    token_symbol: tokens[i],
    balance: Math.floor(Math.random() * 1000000) + 1000,
    current_value_usd: Math.floor(Math.random() * 100000) + 5000,
    unrealized_pnl_usd: Math.floor(Math.random() * 50000) - 25000,
    risk_level: riskLevels[Math.floor(Math.random() * riskLevels.length)],
    last_updated: new Date(Date.now() - Math.random() * 3600000).toISOString()
  }))
}

const formatHoldTime = (minutes: number): string => {
  if (minutes < 60) {
    return `${minutes}m`
  }
  const hours = Math.floor(minutes / 60)
  const remainingMinutes = minutes % 60
  if (hours < 24) {
    return remainingMinutes > 0 ? `${hours}h ${remainingMinutes}m` : `${hours}h`
  }
  const days = Math.floor(hours / 24)
  const remainingHours = hours % 24
  return remainingHours > 0 ? `${days}d ${remainingHours}h` : `${days}d`
}

const getQualityScoreColor = (score: number): string => {
  if (score >= 80) return 'text-green-success bg-green-success/20 border-green-success/30'
  if (score >= 60) return 'text-orange-warning bg-orange-warning/20 border-orange-warning/30'
  return 'text-red-400 bg-red-500/20 border-red-500/30'
}

const getTradingStyleIcon = (style: string) => {
  switch (style.toLowerCase()) {
    case 'scalper':
      return <Activity className="w-5 h-5" />
    case 'swing trader':
      return <TrendingUp className="w-5 h-5" />
    case 'day trader':
      return <BarChart3 className="w-5 h-5" />
    default:
      return <Target className="w-5 h-5" />
  }
}

export default function WalletProfile() {
  const [activeTab, setActiveTab] = useState<'analysis' | 'trades' | 'positions'>('analysis')
  const params = useParams()
  const walletAddress = params.address as string
  
  const { data: analysis, isLoading, error } = useQuery({
    queryKey: ['wallet-analysis', walletAddress],
    queryFn: () => fetchWalletAnalysis(walletAddress!),
    enabled: !!walletAddress
  })

  const { data: trades, isLoading: tradesLoading } = useQuery({
    queryKey: ['wallet-trades', walletAddress],
    queryFn: () => fetchWalletTrades(walletAddress!),
    enabled: !!walletAddress && activeTab === 'trades'
  })

  const { data: positions, isLoading: positionsLoading } = useQuery({
    queryKey: ['wallet-positions', walletAddress],
    queryFn: () => fetchWalletPositions(walletAddress!),
    enabled: !!walletAddress && activeTab === 'positions'
  })

  if (error) {
    return (
      <div className="space-y-6">
        <div className="text-center py-12">
          <AlertTriangle className="w-12 h-12 text-red-400 mx-auto mb-4" />
          <h2 className="text-xl font-semibold text-white mb-2">Failed to Load Wallet Analysis</h2>
          <p className="text-gray-400">Unable to fetch data for this wallet. Please try again later.</p>
        </div>
      </div>
    )
  }

  const getPerformanceBadge = (category: string) => {
    switch (category) {
      case 'excellent':
        return <Badge className="bg-green-success/20 text-green-success border-green-success/30">Excellent</Badge>
      case 'good':
        return <Badge className="bg-blue-steel/20 text-blue-steel border-blue-steel/30">Good</Badge>
      case 'poor':
        return <Badge className="bg-orange-warning/20 text-orange-warning border-orange-warning/30">Poor</Badge>
      case 'loss':
        return <Badge className="bg-red-500/20 text-red-400 border-red-500/30">Loss</Badge>
      default:
        return <Badge className="bg-gray-500/20 text-gray-400 border-gray-500/30">Unknown</Badge>
    }
  }

  const getRiskBadge = (risk: string) => {
    switch (risk) {
      case 'low':
        return <Badge className="bg-green-success/20 text-green-success border-green-success/30">Low Risk</Badge>
      case 'medium':
        return <Badge className="bg-orange-warning/20 text-orange-warning border-orange-warning/30">Medium Risk</Badge>
      case 'high':
        return <Badge className="bg-red-500/20 text-red-400 border-red-500/30">High Risk</Badge>
      default:
        return <Badge className="bg-gray-500/20 text-gray-400 border-gray-500/30">Unknown</Badge>
    }
  }

  return (
    <div className="space-y-6">
      {/* Header Section */}
      <div className="space-y-4">
        {isLoading ? (
          <div className="space-y-3">
            <Skeleton className="h-8 w-96" />
            <Skeleton className="h-6 w-32" />
          </div>
        ) : (
          <>
            <div className="flex items-center space-x-4">
              <div className="w-12 h-12 bg-gradient-to-br from-cyan-bright to-blue-steel rounded-xl flex items-center justify-center">
                <Wallet className="w-6 h-6 text-white" />
              </div>
              <div>
                <h1 className="text-2xl font-bold text-white font-mono">
                  {analysis?.wallet_address}
                </h1>
                <p className="text-gray-400">Wallet Performance Analysis</p>
              </div>
            </div>
            <div className="flex items-center space-x-2">
              <span className="text-sm text-gray-400">Quality Score:</span>
              <Badge className={`text-lg font-bold px-4 py-2 ${getQualityScoreColor(analysis?.metadata.quality_score || 0)}`}>
                {analysis?.metadata.quality_score}/100
              </Badge>
            </div>
          </>
        )}
      </div>

      {/* Key Performance Indicators Row */}
      <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-4">
        {/* Total P&L */}
        <Card className="glass-card border-blue-ice/20">
          <CardHeader className="pb-3">
            <div className="flex items-center justify-between">
              <CardTitle className="text-sm font-medium text-gray-400">Total P&L</CardTitle>
              {(analysis?.portfolio_result.total_pnl_usd || 0) >= 0 ? (
                <TrendingUp className="w-4 h-4 text-green-success" />
              ) : (
                <TrendingDown className="w-4 h-4 text-red-400" />
              )}
            </div>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <Skeleton className="h-8 w-full" />
            ) : (
              <div className={`text-2xl font-bold ${
                (analysis?.portfolio_result.total_pnl_usd || 0) >= 0 ? 'text-green-success' : 'text-red-400'
              }`}>
                {formatCurrency(analysis?.portfolio_result.total_pnl_usd || 0)}
              </div>
            )}
          </CardContent>
        </Card>

        {/* Win Rate */}
        <Card className="glass-card border-blue-ice/20">
          <CardHeader className="pb-3">
            <div className="flex items-center justify-between">
              <CardTitle className="text-sm font-medium text-gray-400">Win Rate</CardTitle>
              <Target className="w-4 h-4 text-cyan-bright" />
            </div>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <Skeleton className="h-8 w-full" />
            ) : (
              <div className="text-2xl font-bold text-white">
                {analysis?.portfolio_result.overall_win_rate_percentage.toFixed(1)}%
              </div>
            )}
          </CardContent>
        </Card>

        {/* Total Trades */}
        <Card className="glass-card border-blue-ice/20">
          <CardHeader className="pb-3">
            <div className="flex items-center justify-between">
              <CardTitle className="text-sm font-medium text-gray-400">Total Trades</CardTitle>
              <BarChart3 className="w-4 h-4 text-blue-steel" />
            </div>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <Skeleton className="h-8 w-full" />
            ) : (
              <div className="text-2xl font-bold text-white">
                {analysis?.portfolio_result.total_trades.toLocaleString()}
              </div>
            )}
          </CardContent>
        </Card>

        {/* Average Hold Time */}
        <Card className="glass-card border-blue-ice/20">
          <CardHeader className="pb-3">
            <div className="flex items-center justify-between">
              <CardTitle className="text-sm font-medium text-gray-400">Avg Hold Time</CardTitle>
              <Clock className="w-4 h-4 text-orange-warning" />
            </div>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <Skeleton className="h-8 w-full" />
            ) : (
              <div className="text-2xl font-bold text-white">
                {formatHoldTime(analysis?.portfolio_result.avg_hold_time_minutes || 0)}
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      {/* Navigation Tabs */}
      <div className="flex space-x-1 bg-navy-deep/50 p-1 rounded-lg">
        <Button
          variant={activeTab === 'analysis' ? 'neon' : 'ghost'}
          onClick={() => setActiveTab('analysis')}
          className="flex-1"
        >
          <BarChart3 className="w-4 h-4 mr-2" />
          Analysis
        </Button>
        <Button
          variant={activeTab === 'trades' ? 'neon' : 'ghost'}
          onClick={() => setActiveTab('trades')}
          className="flex-1"
        >
          <TrendingUpIcon className="w-4 h-4 mr-2" />
          Trade History
        </Button>
        <Button
          variant={activeTab === 'positions' ? 'neon' : 'ghost'}
          onClick={() => setActiveTab('positions')}
          className="flex-1"
        >
          <Briefcase className="w-4 h-4 mr-2" />
          Current Holdings
        </Button>
      </div>

      {/* Tab Content */}
      {activeTab === 'analysis' && (
        <>
      {/* Trading Style and Consistency Section */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <Card className="glass-card border-blue-ice/20">
          <CardHeader>
            <CardTitle className="text-white flex items-center">
              {isLoading ? (
                <Skeleton className="w-6 h-6 mr-2" />
              ) : (
                <>
                  {getTradingStyleIcon(analysis?.copy_trading_metrics.trading_style || '')}
                  <span className="ml-2">Trading Style</span>
                </>
              )}
            </CardTitle>
            <CardDescription className="text-gray-400">
              Identified trading pattern and approach
            </CardDescription>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <div className="space-y-3">
                <Skeleton className="h-8 w-32" />
                <Skeleton className="h-4 w-48" />
              </div>
            ) : (
              <div className="space-y-3">
                <div className="text-2xl font-bold text-cyan-bright">
                  {analysis?.copy_trading_metrics.trading_style}
                </div>
                <p className="text-sm text-gray-400">
                  Average position duration: {formatHoldTime(analysis?.portfolio_result.avg_hold_time_minutes || 0)}
                </p>
              </div>
            )}
          </CardContent>
        </Card>

        <Card className="glass-card border-blue-ice/20">
          <CardHeader>
            <CardTitle className="text-white flex items-center">
              <Shield className="w-5 h-5 mr-2 text-blue-steel" />
              Consistency Score
            </CardTitle>
            <CardDescription className="text-gray-400">
              Trading performance reliability metric
            </CardDescription>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <div className="space-y-3">
                <Skeleton className="h-8 w-16" />
                <Skeleton className="h-2 w-full" />
              </div>
            ) : (
              <div className="space-y-3">
                <div className="text-2xl font-bold text-white">
                  {((analysis?.copy_trading_metrics.consistency_score || 0) * 100).toFixed(0)}%
                </div>
                <Progress 
                  value={(analysis?.copy_trading_metrics.consistency_score || 0) * 100} 
                  className="h-3"
                />
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      {/* Risk Assessment Summary */}
      <Card className="glass-card border-blue-ice/20">
        <CardHeader>
          <CardTitle className="text-white flex items-center">
            <AlertTriangle className="w-5 h-5 mr-2 text-orange-warning" />
            Risk Assessment
          </CardTitle>
          <CardDescription className="text-gray-400">
            Portfolio risk metrics and position management analysis
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
            {/* Max Position Percentage */}
            <div className="space-y-2">
              <h4 className="text-sm font-medium text-gray-400">Max Position Size</h4>
              {isLoading ? (
                <Skeleton className="h-6 w-16" />
              ) : (
                <div className="text-xl font-bold text-white">
                  {analysis?.copy_trading_metrics.risk_metrics.max_position_percentage.toFixed(1)}%
                </div>
              )}
              <p className="text-xs text-gray-500">Largest single position</p>
            </div>

            {/* Diversification Score */}
            <div className="space-y-2">
              <h4 className="text-sm font-medium text-gray-400">Diversification Score</h4>
              {isLoading ? (
                <Skeleton className="h-6 w-16" />
              ) : (
                <div className="text-xl font-bold text-white">
                  {analysis?.copy_trading_metrics.risk_metrics.diversification_score.toFixed(1)}/10
                </div>
              )}
              <p className="text-xs text-gray-500">Portfolio spread rating</p>
            </div>

            {/* Max Consecutive Losses */}
            <div className="space-y-2">
              <h4 className="text-sm font-medium text-gray-400">Max Consecutive Losses</h4>
              {isLoading ? (
                <Skeleton className="h-6 w-16" />
              ) : (
                <div className="text-xl font-bold text-red-400">
                  {analysis?.copy_trading_metrics.risk_metrics.max_consecutive_losses}
                </div>
              )}
              <p className="text-xs text-gray-500">Worst losing streak</p>
            </div>
          </div>
        </CardContent>
      </Card>
        </>
      )}

      {activeTab === 'trades' && (
        <Card className="glass-card border-blue-ice/20">
          <CardHeader>
            <CardTitle className="text-white flex items-center">
              <TrendingUpIcon className="w-5 h-5 mr-2 text-cyan-bright" />
              Trade History
            </CardTitle>
            <CardDescription className="text-gray-400">
              Detailed trade-by-trade breakdown with performance analysis
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="overflow-x-auto">
              <table className="w-full">
                <thead>
                  <tr className="border-b border-blue-ice/20">
                    <th className="text-left py-3 px-4 font-medium text-gray-400">Trade ID</th>
                    <th className="text-left py-3 px-4 font-medium text-gray-400">Token</th>
                    <th className="text-left py-3 px-4 font-medium text-gray-400">Type</th>
                    <th className="text-left py-3 px-4 font-medium text-gray-400">Amount</th>
                    <th className="text-left py-3 px-4 font-medium text-gray-400">P&L</th>
                    <th className="text-left py-3 px-4 font-medium text-gray-400">Performance</th>
                    <th className="text-left py-3 px-4 font-medium text-gray-400">Date</th>
                  </tr>
                </thead>
                <tbody>
                  {tradesLoading ? (
                    Array.from({ length: 10 }).map((_, i) => (
                      <tr key={i} className="border-b border-blue-ice/10">
                        <td className="py-3 px-4"><Skeleton className="h-4 w-24" /></td>
                        <td className="py-3 px-4"><Skeleton className="h-4 w-16" /></td>
                        <td className="py-3 px-4"><Skeleton className="h-4 w-12" /></td>
                        <td className="py-3 px-4"><Skeleton className="h-4 w-20" /></td>
                        <td className="py-3 px-4"><Skeleton className="h-4 w-20" /></td>
                        <td className="py-3 px-4"><Skeleton className="h-6 w-20" /></td>
                        <td className="py-3 px-4"><Skeleton className="h-4 w-24" /></td>
                      </tr>
                    ))
                  ) : (
                    trades?.map((trade) => (
                      <tr key={trade.trade_id} className="border-b border-blue-ice/10 hover:bg-navy-deep/30 transition-colors">
                        <td className="py-3 px-4">
                          <span className="font-mono text-xs text-gray-400">
                            {trade.trade_id.slice(0, 12)}...
                          </span>
                        </td>
                        <td className="py-3 px-4">
                          <Badge className="bg-blue-steel/20 text-blue-steel border-blue-steel/30">
                            {trade.token_symbol}
                          </Badge>
                        </td>
                        <td className="py-3 px-4">
                          <span className={`text-sm font-medium ${
                            trade.trade_type === 'buy' ? 'text-green-success' : 'text-red-400'
                          }`}>
                            {trade.trade_type.toUpperCase()}
                          </span>
                        </td>
                        <td className="py-3 px-4">
                          <span className="text-white">{formatCurrency(trade.amount_usd)}</span>
                        </td>
                        <td className="py-3 px-4">
                          <span className={trade.pnl_usd >= 0 ? 'text-green-success' : 'text-red-400'}>
                            {formatCurrency(trade.pnl_usd)}
                          </span>
                        </td>
                        <td className="py-3 px-4">
                          {getPerformanceBadge(trade.performance_category)}
                        </td>
                        <td className="py-3 px-4">
                          <span className="text-sm text-gray-400">
                            {new Date(trade.timestamp).toLocaleDateString()}
                          </span>
                        </td>
                      </tr>
                    ))
                  )}
                </tbody>
              </table>
            </div>
          </CardContent>
        </Card>
      )}

      {activeTab === 'positions' && (
        <Card className="glass-card border-blue-ice/20">
          <CardHeader>
            <CardTitle className="text-white flex items-center">
              <Briefcase className="w-5 h-5 mr-2 text-cyan-bright" />
              Current Holdings
            </CardTitle>
            <CardDescription className="text-gray-400">
              Current token positions with unrealized P&L and risk assessment
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="overflow-x-auto">
              <table className="w-full">
                <thead>
                  <tr className="border-b border-blue-ice/20">
                    <th className="text-left py-3 px-4 font-medium text-gray-400">Token</th>
                    <th className="text-left py-3 px-4 font-medium text-gray-400">Balance</th>
                    <th className="text-left py-3 px-4 font-medium text-gray-400">Current Value</th>
                    <th className="text-left py-3 px-4 font-medium text-gray-400">Unrealized P&L</th>
                    <th className="text-left py-3 px-4 font-medium text-gray-400">Risk Level</th>
                    <th className="text-left py-3 px-4 font-medium text-gray-400">Last Updated</th>
                  </tr>
                </thead>
                <tbody>
                  {positionsLoading ? (
                    Array.from({ length: 8 }).map((_, i) => (
                      <tr key={i} className="border-b border-blue-ice/10">
                        <td className="py-3 px-4"><Skeleton className="h-4 w-16" /></td>
                        <td className="py-3 px-4"><Skeleton className="h-4 w-24" /></td>
                        <td className="py-3 px-4"><Skeleton className="h-4 w-20" /></td>
                        <td className="py-3 px-4"><Skeleton className="h-4 w-20" /></td>
                        <td className="py-3 px-4"><Skeleton className="h-6 w-20" /></td>
                        <td className="py-3 px-4"><Skeleton className="h-4 w-24" /></td>
                      </tr>
                    ))
                  ) : (
                    positions?.map((position, i) => (
                      <tr key={i} className="border-b border-blue-ice/10 hover:bg-navy-deep/30 transition-colors">
                        <td className="py-3 px-4">
                          <div>
                            <Badge className="bg-blue-steel/20 text-blue-steel border-blue-steel/30 mb-1">
                              {position.token_symbol}
                            </Badge>
                            <div className="text-xs text-gray-400 font-mono">
                              {truncateAddress(position.token_address)}
                            </div>
                          </div>
                        </td>
                        <td className="py-3 px-4">
                          <span className="text-white">{position.balance.toLocaleString()}</span>
                        </td>
                        <td className="py-3 px-4">
                          <span className="text-white">{formatCurrency(position.current_value_usd)}</span>
                        </td>
                        <td className="py-3 px-4">
                          <span className={position.unrealized_pnl_usd >= 0 ? 'text-green-success' : 'text-red-400'}>
                            {formatCurrency(position.unrealized_pnl_usd)}
                          </span>
                        </td>
                        <td className="py-3 px-4">
                          {getRiskBadge(position.risk_level)}
                        </td>
                        <td className="py-3 px-4">
                          <span className="text-sm text-gray-400">
                            {new Date(position.last_updated).toLocaleString()}
                          </span>
                        </td>
                      </tr>
                    ))
                  )}
                </tbody>
              </table>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  )
}