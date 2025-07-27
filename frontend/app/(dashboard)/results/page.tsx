'use client'

import { useState, useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { LoadingSpinner } from '@/components/ui/loading-spinner'
import { api } from '@/lib/api'
import { formatCurrency, formatPercentage, truncateAddress } from '@/lib/utils'
import { 
  Download, Filter, Search, Eye, TrendingUp, TrendingDown, 
  ChevronLeft, ChevronRight, ArrowUpDown, ExternalLink,
  BarChart3, Wallet, Target, Calendar, DollarSign, Copy,
  Activity, Clock, Award, Zap, TrendingUpIcon, AlertTriangle
} from 'lucide-react'
import { motion, AnimatePresence } from 'framer-motion'

interface WalletResult {
  wallet_address: string
  token_address: string
  token_symbol: string
  total_pnl_usd: string
  realized_pnl_usd: string
  unrealized_pnl_usd: string
  roi_percentage: string
  total_trades: number
  win_rate: string
  avg_hold_time_minutes: string
  analyzed_at: string
}

interface ResultsResponse {
  results: WalletResult[]
  pagination: {
    total_count: number
    limit: number
    offset: number
    has_more: boolean
  }
  summary: {
    total_wallets: number
    profitable_wallets: number
    total_pnl_usd: string
    average_pnl_usd: string
    total_trades: number
    profitability_rate: number
    last_updated: string
  }
}

interface Trade {
  id: string
  type: 'buy' | 'sell' | 'transfer_in' | 'transfer_out'
  token_symbol: string
  token_address: string
  amount: string
  price_usd: string
  value_usd: string
  timestamp: string
  signature: string
  realized_pnl?: string
  fees_usd: string
}

interface TokenHolding {
  token_symbol: string
  token_address: string
  amount: string
  current_price_usd: string
  current_value_usd: string
  avg_buy_price: string
  unrealized_pnl_usd: string
  unrealized_pnl_percentage: string
}

export default function Results() {
  const [currentPage, setCurrentPage] = useState(1)
  const [sortBy, setSortBy] = useState<'pnl' | 'win_rate' | 'trades' | 'analyzed_at' | 'hold_time' | 'composite'>('composite')
  const [sortOrder, setSortOrder] = useState<'asc' | 'desc'>('desc')
  const [showDetailModal, setShowDetailModal] = useState<string | null>(null)
  const [showTradeHistoryModal, setShowTradeHistoryModal] = useState<string | null>(null)
  
  // Advanced filters
  const [minPnl, setMinPnl] = useState<string>('')
  const [maxPnl, setMaxPnl] = useState<string>('')
  const [minWinRate, setMinWinRate] = useState<string>('')
  const [maxWinRate, setMaxWinRate] = useState<string>('')
  const [minHoldTime, setMinHoldTime] = useState<string>('')
  const [maxHoldTime, setMaxHoldTime] = useState<string>('')
  const [showAdvancedFilters, setShowAdvancedFilters] = useState(false)
  const pageSize = 25

  const { data: resultsData, isLoading } = useQuery<ResultsResponse>({
    queryKey: ['results'],
    queryFn: () => api.results.getResults({
      limit: 500,
      offset: 0
    }),
    refetchInterval: 30000 // Refresh every 30 seconds
  })

  // Get detailed data for the selected wallet
  const selectedWalletData = useMemo(() => {
    if (!showDetailModal || !resultsData?.results) return null
    return resultsData.results.find(r => r.wallet_address === showDetailModal)
  }, [showDetailModal, resultsData?.results])

  // Calculate composite score for ranking best traders
  const calculateCompositeScore = (result: WalletResult, allResults: WalletResult[]) => {
    const pnl = parseFloat(result.total_pnl_usd)
    const winRate = parseFloat(result.win_rate)
    const holdTime = parseFloat(result.avg_hold_time_minutes)
    const trades = result.total_trades

    // Normalize P&L (0-100 scale)
    const pnlValues = allResults.map(r => parseFloat(r.total_pnl_usd))
    const maxPnl = Math.max(...pnlValues)
    const minPnl = Math.min(...pnlValues)
    const pnlNormalized = maxPnl > minPnl ? ((pnl - minPnl) / (maxPnl - minPnl)) * 100 : 50

    // Hold time efficiency (optimal range: 30min - 24hrs)
    let holdTimeEfficiency = 0
    if (holdTime >= 30 && holdTime <= 1440) {
      holdTimeEfficiency = 100 // Optimal range
    } else if (holdTime < 30) {
      holdTimeEfficiency = Math.max(0, (holdTime / 30) * 70) // Scalping penalty
    } else {
      holdTimeEfficiency = Math.max(0, 100 - ((holdTime - 1440) / 1440) * 50) // Long hold penalty
    }

    // Normalize trade volume (0-100 scale)
    const tradeValues = allResults.map(r => r.total_trades)
    const maxTrades = Math.max(...tradeValues)
    const minTrades = Math.min(...tradeValues)
    const tradesNormalized = maxTrades > minTrades ? ((trades - minTrades) / (maxTrades - minTrades)) * 100 : 50

    // Composite score with weights
    return (
      (pnlNormalized * 0.4) +           // 40% P&L
      (winRate * 0.35) +                // 35% Win Rate
      (holdTimeEfficiency * 0.15) +     // 15% Hold Time Efficiency
      (tradesNormalized * 0.1)          // 10% Trade Volume
    )
  }

  // Sort and filter results locally for responsiveness
  const processedResults = useMemo(() => {
    if (!resultsData?.results) return {
      results: [],
      totalFiltered: 0,
      totalOriginal: 0
    }
    
    // Apply ALL filters consistently (no conditional logic)
    let filtered = resultsData.results.filter(result => {
      const pnl = parseFloat(result.total_pnl_usd)
      const winRate = parseFloat(result.win_rate)
      const holdTime = parseFloat(result.avg_hold_time_minutes)
      
      // P&L Range Filters
      if (minPnl && pnl < parseFloat(minPnl)) return false
      if (maxPnl && pnl > parseFloat(maxPnl)) return false
      
      // Win Rate Range Filters (THIS WAS THE MISSING PART!)
      if (minWinRate && winRate < parseFloat(minWinRate)) return false
      if (maxWinRate && winRate > parseFloat(maxWinRate)) return false
      
      // Hold Time Range Filters
      if (minHoldTime && holdTime < parseFloat(minHoldTime)) return false
      if (maxHoldTime && holdTime > parseFloat(maxHoldTime)) return false
      
      return true
    })
    
    // Apply local sorting
    filtered.sort((a, b) => {
      let aVal: number, bVal: number
      
      switch (sortBy) {
        case 'composite':
          aVal = calculateCompositeScore(a, filtered)
          bVal = calculateCompositeScore(b, filtered)
          break
        case 'pnl':
          aVal = parseFloat(a.total_pnl_usd)
          bVal = parseFloat(b.total_pnl_usd)
          break
        case 'win_rate':
          aVal = parseFloat(a.win_rate)
          bVal = parseFloat(b.win_rate)
          break
        case 'trades':
          aVal = a.total_trades
          bVal = b.total_trades
          break
        case 'analyzed_at':
          aVal = new Date(a.analyzed_at).getTime()
          bVal = new Date(b.analyzed_at).getTime()
          break
        case 'hold_time':
          aVal = parseFloat(a.avg_hold_time_minutes)
          bVal = parseFloat(b.avg_hold_time_minutes)
          break
        default:
          return 0
      }
      
      return sortOrder === 'desc' ? bVal - aVal : aVal - bVal
    })
    
    // Apply client-side pagination
    const startIndex = (currentPage - 1) * pageSize
    const endIndex = startIndex + pageSize
    const paginatedResults = filtered.slice(startIndex, endIndex)
    
    return {
      results: paginatedResults,
      totalFiltered: filtered.length,
      totalOriginal: resultsData?.results.length || 0
    }
  }, [resultsData?.results, sortBy, sortOrder, minPnl, maxPnl, minWinRate, maxWinRate, minHoldTime, maxHoldTime, currentPage])

  const totalPages = Math.ceil((processedResults.totalFiltered || 0) / pageSize)

  const getPnLBadge = (pnl: number) => {
    if (pnl > 0) {
      return <Badge className="bg-green-success/20 text-green-success border-green-success/30">
        <TrendingUp className="w-3 h-3 mr-1" />
        +{formatCurrency(pnl)}
      </Badge>
    }
    return <Badge className="bg-red-500/20 text-red-400 border-red-500/30">
      <TrendingDown className="w-3 h-3 mr-1" />
      {formatCurrency(pnl)}
    </Badge>
  }

  const getWinRateColor = (winRate: number) => {
    if (winRate >= 70) return 'text-green-success'
    if (winRate >= 50) return 'text-orange-warning'
    return 'text-red-400'
  }

  const handleSortChange = (field: typeof sortBy) => {
    if (sortBy === field) {
      setSortOrder(sortOrder === 'desc' ? 'asc' : 'desc')
    } else {
      setSortBy(field)
      setSortOrder('desc')
    }
  }

  const clearAllFilters = () => {
    setMinPnl('')
    setMaxPnl('')
    setMinWinRate('')
    setMaxWinRate('')
    setMinHoldTime('')
    setMaxHoldTime('')
    setCurrentPage(1)
  }

  const applyFilters = () => {
    setCurrentPage(1)
    // Trigger re-fetch with new filters
  }

  const exportToCSV = () => {
    if (!resultsData?.results) return
    
    const headers = ['Wallet Address', 'Token', 'Total P&L (USD)', 'Realized P&L (USD)', 'Unrealized P&L (USD)', 'Win Rate (%)', 'Total Trades', 'Avg Hold Time (mins)', 'Analyzed At']
    const csvData = resultsData.results.map(result => [
      result.wallet_address,
      result.token_symbol,
      result.total_pnl_usd,
      result.realized_pnl_usd,
      result.unrealized_pnl_usd,
      result.win_rate,
      result.total_trades.toString(),
      result.avg_hold_time_minutes,
      result.analyzed_at
    ])
    
    const csvContent = [headers, ...csvData]
      .map(row => row.map(cell => `"${cell}"`).join(','))
      .join('\n')
    
    const blob = new Blob([csvContent], { type: 'text/csv;charset=utf-8;' })
    const link = document.createElement('a')
    const url = URL.createObjectURL(blob)
    link.setAttribute('href', url)
    link.setAttribute('download', `wallet_results_${new Date().toISOString().split('T')[0]}.csv`)
    document.body.appendChild(link)
    link.click()
    document.body.removeChild(link)
    URL.revokeObjectURL(url)
  }

  const copyToClipboard = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text)
    } catch (err) {
      console.error('Failed to copy text: ', err)
    }
  }

  // Generate mock trade data for demonstration (in real app, this would come from API)
  const generateMockTradeData = (walletAddress: string): { trades: Trade[], holdings: TokenHolding[] } => {
    const tokens = ['SOLANA', 'USDC', 'BONK', 'WIF', 'POPCAT', 'RAYDOG']
    const trades: Trade[] = []
    const holdings: TokenHolding[] = []

    // Generate trades
    for (let i = 0; i < 25; i++) {
      const token = tokens[Math.floor(Math.random() * tokens.length)]
      const type = Math.random() > 0.5 ? 'buy' : 'sell'
      const amount = (Math.random() * 10000).toFixed(2)
      const price = (Math.random() * 100).toFixed(4)
      const value = (parseFloat(amount) * parseFloat(price)).toFixed(2)
      
      trades.push({
        id: `trade_${i}`,
        type,
        token_symbol: token,
        token_address: `${token.toLowerCase()}_address_${i}`,
        amount,
        price_usd: price,
        value_usd: value,
        timestamp: new Date(Date.now() - i * 86400000 * Math.random()).toISOString(),
        signature: `${walletAddress.slice(0, 8)}...${Math.random().toString(36).substring(2, 8)}`,
        realized_pnl: type === 'sell' ? ((Math.random() - 0.4) * 1000).toFixed(2) : undefined,
        fees_usd: (Math.random() * 5).toFixed(2)
      })
    }

    // Generate current holdings
    for (let i = 0; i < 5; i++) {
      const token = tokens[i]
      const amount = (Math.random() * 1000).toFixed(2)
      const currentPrice = (Math.random() * 50).toFixed(4)
      const avgBuyPrice = (Math.random() * 50).toFixed(4)
      const currentValue = (parseFloat(amount) * parseFloat(currentPrice)).toFixed(2)
      const unrealizedPnl = ((parseFloat(currentPrice) - parseFloat(avgBuyPrice)) * parseFloat(amount)).toFixed(2)
      
      holdings.push({
        token_symbol: token,
        token_address: `${token.toLowerCase()}_address`,
        amount,
        current_price_usd: currentPrice,
        current_value_usd: currentValue,
        avg_buy_price: avgBuyPrice,
        unrealized_pnl_usd: unrealizedPnl,
        unrealized_pnl_percentage: ((parseFloat(unrealizedPnl) / (parseFloat(avgBuyPrice) * parseFloat(amount))) * 100).toFixed(2)
      })
    }

    return { trades: trades.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime()), holdings }
  }

  const tradeHistoryData = useMemo(() => {
    if (!showTradeHistoryModal) return null
    return generateMockTradeData(showTradeHistoryModal)
  }, [showTradeHistoryModal])

  return (
    <div className="space-y-6">
      {/* Header */}
      <motion.div 
        className="flex flex-col md:flex-row md:items-center justify-between gap-4"
        initial={{ opacity: 0, y: -20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.6 }}
      >
        <div>
          <h1 className="text-3xl font-bold text-white mb-2">P&L Analysis Results</h1>
          <p className="text-gray-400">
            {resultsData?.summary.total_wallets || 0} wallets analyzed • 
            {resultsData?.summary.profitable_wallets || 0} profitable •
            {resultsData?.summary.profitability_rate ? ` ${resultsData.summary.profitability_rate.toFixed(1)}% success rate` : ''}
          </p>
        </div>
        <div className="flex items-center space-x-2">
          <Button 
            variant="outline" 
            size="sm" 
            onClick={exportToCSV}
            disabled={!resultsData?.results.length}
            className="border-green-success/50 text-green-success hover:bg-green-success/20"
          >
            <Download className="w-4 h-4 mr-2" />
            Export CSV
          </Button>
        </div>
      </motion.div>

      {/* Summary Statistics */}
      <motion.div
        className="grid grid-cols-1 md:grid-cols-4 gap-4"
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.6, delay: 0.1 }}
      >
        <Card className="glass-card border-blue-ice/20">
          <CardContent className="pt-4">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-gray-400">Total P&L</p>
                <p className={`text-2xl font-bold ${
                  parseFloat(resultsData?.summary.total_pnl_usd || '0') >= 0 
                    ? 'text-green-success' 
                    : 'text-red-400'
                }`}>
                  {formatCurrency(parseFloat(resultsData?.summary.total_pnl_usd || '0'))}
                </p>
              </div>
              <DollarSign className="w-8 h-8 text-cyan-bright" />
            </div>
          </CardContent>
        </Card>
        
        <Card className="glass-card border-blue-ice/20">
          <CardContent className="pt-4">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-gray-400">Average P&L</p>
                <p className={`text-2xl font-bold ${
                  parseFloat(resultsData?.summary.average_pnl_usd || '0') >= 0 
                    ? 'text-green-success' 
                    : 'text-red-400'
                }`}>
                  {formatCurrency(parseFloat(resultsData?.summary.average_pnl_usd || '0'))}
                </p>
              </div>
              <BarChart3 className="w-8 h-8 text-orange-warning" />
            </div>
          </CardContent>
        </Card>
        
        <Card className="glass-card border-blue-ice/20">
          <CardContent className="pt-4">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-gray-400">Total Wallets</p>
                <p className="text-2xl font-bold text-white">
                  {resultsData?.summary.total_wallets || 0}
                </p>
                <p className="text-xs text-green-success">
                  {resultsData?.summary.profitable_wallets || 0} profitable
                </p>
              </div>
              <Wallet className="w-8 h-8 text-blue-soft" />
            </div>
          </CardContent>
        </Card>
        
        <Card className="glass-card border-blue-ice/20">
          <CardContent className="pt-4">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-gray-400">Total Trades</p>
                <p className="text-2xl font-bold text-white">
                  {(resultsData?.summary.total_trades || 0).toLocaleString()}
                </p>
                <p className="text-xs text-cyan-bright">
                  Success Rate: {resultsData?.summary.profitability_rate?.toFixed(1) || 0}%
                </p>
              </div>
              <Target className="w-8 h-8 text-purple-400" />
            </div>
          </CardContent>
        </Card>
      </motion.div>

      {/* Search and Filters */}
      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.6, delay: 0.2 }}
      >
        <Card className="glass-card border-blue-ice/20">
          <CardContent className="pt-6 space-y-4">
            {/* Basic Filters and Sorting */}
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div className="flex space-x-2">
                <Button 
                  variant="outline" 
                  size="sm"
                  onClick={() => setShowAdvancedFilters(!showAdvancedFilters)}
                  className="border-cyan-bright/50 text-cyan-bright hover:bg-cyan-bright/20"
                >
                  <Filter className="w-4 h-4 mr-2" />
                  Advanced Filters
                </Button>
                <Button 
                  variant="ghost" 
                  size="sm"
                  onClick={clearAllFilters}
                >
                  Clear All
                </Button>
              </div>
              
              <div className="flex items-center space-x-2 text-sm text-gray-400">
                <Calendar className="w-4 h-4" />
                <span>
                  Last updated: {resultsData?.summary.last_updated 
                    ? new Date(resultsData.summary.last_updated).toLocaleString()
                    : 'Loading...'}
                </span>
              </div>
            </div>

            {/* Advanced Filters */}
            <AnimatePresence>
              {showAdvancedFilters && (
                <motion.div
                  initial={{ opacity: 0, height: 0 }}
                  animate={{ opacity: 1, height: 'auto' }}
                  exit={{ opacity: 0, height: 0 }}
                  transition={{ duration: 0.3 }}
                  className="border-t border-blue-ice/20 pt-4"
                >
                  <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
                    {/* P&L Range */}
                    <div className="space-y-3">
                      <h4 className="text-sm font-medium text-white flex items-center">
                        <DollarSign className="w-4 h-4 mr-2 text-green-success" />
                        P&L Range (USD)
                      </h4>
                      <div className="grid grid-cols-2 gap-2">
                        <Input
                          type="number"
                          placeholder="Min P&L"
                          value={minPnl}
                          onChange={(e) => setMinPnl(e.target.value)}
                          className="bg-navy-deep/50 border-blue-ice/20 text-white placeholder-gray-400"
                        />
                        <Input
                          type="number"
                          placeholder="Max P&L"
                          value={maxPnl}
                          onChange={(e) => setMaxPnl(e.target.value)}
                          className="bg-navy-deep/50 border-blue-ice/20 text-white placeholder-gray-400"
                        />
                      </div>
                    </div>

                    {/* Win Rate Range */}
                    <div className="space-y-3">
                      <h4 className="text-sm font-medium text-white flex items-center">
                        <Target className="w-4 h-4 mr-2 text-orange-warning" />
                        Win Rate Range (%)
                      </h4>
                      <div className="grid grid-cols-2 gap-2">
                        <Input
                          type="number"
                          placeholder="Min %"
                          value={minWinRate}
                          onChange={(e) => setMinWinRate(e.target.value)}
                          min="0"
                          max="100"
                          className="bg-navy-deep/50 border-blue-ice/20 text-white placeholder-gray-400"
                        />
                        <Input
                          type="number"
                          placeholder="Max %"
                          value={maxWinRate}
                          onChange={(e) => setMaxWinRate(e.target.value)}
                          min="0"
                          max="100"
                          className="bg-navy-deep/50 border-blue-ice/20 text-white placeholder-gray-400"
                        />
                      </div>
                    </div>

                    {/* Hold Time Range */}
                    <div className="space-y-3">
                      <h4 className="text-sm font-medium text-white flex items-center">
                        <Clock className="w-4 h-4 mr-2 text-purple-400" />
                        Hold Time Range (minutes)
                      </h4>
                      <div className="grid grid-cols-2 gap-2">
                        <Input
                          type="number"
                          placeholder="Min minutes"
                          value={minHoldTime}
                          onChange={(e) => setMinHoldTime(e.target.value)}
                          className="bg-navy-deep/50 border-blue-ice/20 text-white placeholder-gray-400"
                        />
                        <Input
                          type="number"
                          placeholder="Max minutes"
                          value={maxHoldTime}
                          onChange={(e) => setMaxHoldTime(e.target.value)}
                          className="bg-navy-deep/50 border-blue-ice/20 text-white placeholder-gray-400"
                        />
                      </div>
                    </div>
                  </div>

                  <div className="flex items-center justify-between mt-4 pt-4 border-t border-blue-ice/20">
                    <div className="text-sm text-gray-400">
                      Showing {processedResults.results.length} of {processedResults.totalFiltered} filtered wallets
                      (Total: {processedResults.totalOriginal} wallets)
                    </div>
                    <div className="flex space-x-2">
                      <Button 
                        variant="outline" 
                        size="sm"
                        onClick={applyFilters}
                        className="border-green-success/50 text-green-success hover:bg-green-success/20"
                      >
                        Apply Filters
                      </Button>
                      <Button 
                        variant="ghost" 
                        size="sm"
                        onClick={clearAllFilters}
                      >
                        Reset All
                      </Button>
                    </div>
                  </div>
                </motion.div>
              )}
            </AnimatePresence>
          </CardContent>
        </Card>
      </motion.div>

      {/* Results Table */}
      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.6, delay: 0.3 }}
      >
        <Card className="glass-card border-blue-ice/20">
          <CardHeader>
            <div className="flex items-center justify-between">
              <div>
                <CardTitle className="text-white">Analysis Results</CardTitle>
                <CardDescription className="text-gray-400">
                  Showing {processedResults.results.length} of {processedResults.totalFiltered} filtered results
                  (Total: {processedResults.totalOriginal} wallets)
                </CardDescription>
              </div>
              <div className="flex items-center space-x-3">
                <span className="text-sm text-gray-400">Sort by:</span>
                <Select value={sortBy} onValueChange={(value) => setSortBy(value as typeof sortBy)}>
                  <SelectTrigger className="w-48 bg-navy-deep/50 border-blue-ice/20 text-white">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="composite">🏆 Best Overall Traders</SelectItem>
                    <SelectItem value="pnl">💰 Highest P&L</SelectItem>
                    <SelectItem value="win_rate">🎯 Best Win Rate</SelectItem>
                    <SelectItem value="hold_time">⏱️ Hold Time</SelectItem>
                    <SelectItem value="trades">📈 Most Active</SelectItem>
                    <SelectItem value="analyzed_at">📅 Recent Analysis</SelectItem>
                  </SelectContent>
                </Select>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setSortOrder(sortOrder === 'desc' ? 'asc' : 'desc')}
                  className="text-cyan-bright hover:bg-cyan-bright/20"
                >
                  {sortOrder === 'desc' ? '↓' : '↑'}
                </Button>
              </div>
            </div>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <div className="flex items-center justify-center py-12">
                <LoadingSpinner size="lg" />
              </div>
            ) : (
              <div className="overflow-x-auto">
                <table className="w-full">
                  <thead>
                    <tr className="border-b border-blue-ice/20">
                      <th className="text-left py-3 px-4 font-medium text-gray-400">
                        Wallet Address
                      </th>
                      <th className="text-left py-3 px-4 font-medium text-gray-400">
                        Token
                      </th>
                      <th 
                        className="text-left py-3 px-4 font-medium text-gray-400 cursor-pointer hover:text-white transition-colors"
                        onClick={() => handleSortChange('pnl')}
                      >
                        <div className="flex items-center space-x-1">
                          <span>Total P&L</span>
                          <ArrowUpDown className="w-3 h-3" />
                          {sortBy === 'pnl' && (
                            <span className="text-cyan-bright">{sortOrder === 'desc' ? '↓' : '↑'}</span>
                          )}
                        </div>
                      </th>
                      <th className="text-left py-3 px-4 font-medium text-gray-400">
                        Realized / Unrealized
                      </th>
                      <th 
                        className="text-left py-3 px-4 font-medium text-gray-400 cursor-pointer hover:text-white transition-colors"
                        onClick={() => handleSortChange('win_rate')}
                      >
                        <div className="flex items-center space-x-1">
                          <span>Win Rate</span>
                          <ArrowUpDown className="w-3 h-3" />
                          {sortBy === 'win_rate' && (
                            <span className="text-cyan-bright">{sortOrder === 'desc' ? '↓' : '↑'}</span>
                          )}
                        </div>
                      </th>
                      <th 
                        className="text-left py-3 px-4 font-medium text-gray-400 cursor-pointer hover:text-white transition-colors"
                        onClick={() => handleSortChange('hold_time')}
                      >
                        <div className="flex items-center space-x-1">
                          <span>Hold Time</span>
                          <ArrowUpDown className="w-3 h-3" />
                          {sortBy === 'hold_time' && (
                            <span className="text-cyan-bright">{sortOrder === 'desc' ? '↓' : '↑'}</span>
                          )}
                        </div>
                      </th>
                      <th 
                        className="text-left py-3 px-4 font-medium text-gray-400 cursor-pointer hover:text-white transition-colors"
                        onClick={() => handleSortChange('analyzed_at')}
                      >
                        <div className="flex items-center space-x-1">
                          <span>Analyzed</span>
                          <ArrowUpDown className="w-3 h-3" />
                          {sortBy === 'analyzed_at' && (
                            <span className="text-cyan-bright">{sortOrder === 'desc' ? '↓' : '↑'}</span>
                          )}
                        </div>
                      </th>
                      <th className="text-left py-3 px-4 font-medium text-gray-400">Actions</th>
                    </tr>
                  </thead>
                  <tbody>
                    {processedResults.results.map((result, index) => (
                      <motion.tr 
                        key={`${result.wallet_address}-${result.token_address}`}
                        className="border-b border-blue-ice/10 hover:bg-navy-deep/30 transition-colors"
                        initial={{ opacity: 0, x: -20 }}
                        animate={{ opacity: 1, x: 0 }}
                        transition={{ duration: 0.3, delay: index * 0.05 }}
                      >
                        <td className="py-3 px-4">
                          <button
                            onClick={() => setShowTradeHistoryModal(result.wallet_address)}
                            className="font-mono text-sm text-cyan-bright hover:text-cyan-bright/80 hover:underline cursor-pointer transition-colors"
                            title="Click to view full P&L report and trade history"
                          >
                            {truncateAddress(result.wallet_address)}
                          </button>
                        </td>
                        <td className="py-3 px-4">
                          <Badge className="bg-blue-steel/20 text-blue-steel border-blue-steel/30">
                            {result.token_symbol}
                          </Badge>
                        </td>
                        <td className="py-3 px-4">
                          {getPnLBadge(parseFloat(result.total_pnl_usd))}
                        </td>
                        <td className="py-3 px-4">
                          <div className="text-xs space-y-1">
                            <div className="text-cyan-bright">
                              R: {formatCurrency(parseFloat(result.realized_pnl_usd))}
                            </div>
                            <div className="text-orange-warning">
                              U: {formatCurrency(parseFloat(result.unrealized_pnl_usd))}
                            </div>
                          </div>
                        </td>
                        <td className="py-3 px-4">
                          <div className={`font-medium ${getWinRateColor(parseFloat(result.win_rate))}`}>
                            {formatPercentage(parseFloat(result.win_rate))}
                          </div>
                        </td>
                        <td className="py-3 px-4">
                          <div className="text-white font-medium">
                            {Math.round(parseFloat(result.avg_hold_time_minutes))}m
                          </div>
                          <div className="text-xs text-gray-400">
                            {Math.round(parseFloat(result.avg_hold_time_minutes)) < 60 
                              ? "Scalper" 
                              : Math.round(parseFloat(result.avg_hold_time_minutes)) < 1440 
                              ? "Day Trader" 
                              : "Swing Trader"}
                          </div>
                        </td>
                        <td className="py-3 px-4">
                          <div className="text-sm text-gray-400">
                            {new Date(result.analyzed_at).toLocaleDateString()}
                          </div>
                        </td>
                        <td className="py-3 px-4">
                          <div className="flex items-center space-x-1">
                            <Button 
                              variant="ghost" 
                              size="sm" 
                              onClick={() => setShowDetailModal(result.wallet_address)}
                              className="text-cyan-bright hover:bg-cyan-bright/20"
                              title="View P&L Analysis"
                            >
                              <Eye className="w-4 h-4" />
                            </Button>
                            <Button 
                              variant="ghost" 
                              size="sm" 
                              onClick={() => setShowTradeHistoryModal(result.wallet_address)}
                              className="text-green-success hover:bg-green-success/20"
                              title="View Trade History"
                            >
                              <Activity className="w-4 h-4" />
                            </Button>
                            <Button 
                              variant="ghost" 
                              size="sm"
                              onClick={() => window.open(`https://solscan.io/account/${result.wallet_address}`, '_blank')}
                              className="text-blue-steel hover:bg-blue-steel/20"
                              title="View on Solscan"
                            >
                              <ExternalLink className="w-3 h-3" />
                            </Button>
                            <Button 
                              variant="ghost" 
                              size="sm"
                              onClick={() => window.open(`https://gmgn.ai/sol/address/${result.wallet_address}`, '_blank')}
                              className="text-green-success hover:bg-green-success/20"
                              title="View on GMGN"
                            >
                              <ExternalLink className="w-3 h-3" />
                            </Button>
                          </div>
                        </td>
                      </motion.tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </CardContent>
        </Card>
      </motion.div>

      {/* Pagination */}
      {totalPages > 1 && (
        <motion.div
          className="flex items-center justify-center space-x-2"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ duration: 0.6, delay: 0.4 }}
        >
          <Button
            variant="outline"
            size="sm"
            onClick={() => setCurrentPage(Math.max(1, currentPage - 1))}
            disabled={currentPage === 1}
            className="border-blue-ice/20 text-white hover:bg-blue-ice/20"
          >
            <ChevronLeft className="w-4 h-4" />
          </Button>
          
          <div className="flex items-center space-x-1">
            {Array.from({ length: Math.min(5, totalPages) }, (_, i) => {
              const pageNum = i + 1
              return (
                <Button
                  key={pageNum}
                  variant={currentPage === pageNum ? "neon" : "ghost"}
                  size="sm"
                  onClick={() => setCurrentPage(pageNum)}
                  className="w-8 h-8 p-0"
                >
                  {pageNum}
                </Button>
              )
            })}
            
            {totalPages > 5 && (
              <>
                <span className="text-gray-400">...</span>
                <Button
                  variant={currentPage === totalPages ? "neon" : "ghost"}
                  size="sm"
                  onClick={() => setCurrentPage(totalPages)}
                  className="w-8 h-8 p-0"
                >
                  {totalPages}
                </Button>
              </>
            )}
          </div>
          
          <Button
            variant="outline"
            size="sm"
            onClick={() => setCurrentPage(Math.min(totalPages, currentPage + 1))}
            disabled={currentPage === totalPages}
            className="border-blue-ice/20 text-white hover:bg-blue-ice/20"
          >
            <ChevronRight className="w-4 h-4" />
          </Button>
        </motion.div>
      )}

      {/* Detailed Wallet Analysis Modal */}
      <AnimatePresence>
        {showDetailModal && selectedWalletData && (
          <Dialog open={!!showDetailModal} onOpenChange={() => setShowDetailModal(null)}>
            <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto bg-navy-deep border-blue-ice/20">
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
                {/* Wallet Overview Section */}
                <motion.div
                  initial={{ opacity: 0, y: 20 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ duration: 0.4 }}
                  className="grid grid-cols-1 md:grid-cols-2 gap-4"
                >
                  <Card className="glass-card border-blue-ice/20">
                    <CardHeader className="pb-3">
                      <CardTitle className="text-sm font-medium text-gray-400 flex items-center">
                        <Activity className="w-4 h-4 mr-2" />
                        Wallet Information
                      </CardTitle>
                    </CardHeader>
                    <CardContent className="space-y-3">
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-gray-400">Address:</span>
                        <div className="flex items-center space-x-2">
                          <span className="font-mono text-sm text-white">
                            {truncateAddress(selectedWalletData.wallet_address)}
                          </span>
                          <Button 
                            variant="ghost" 
                            size="sm" 
                            onClick={() => copyToClipboard(selectedWalletData.wallet_address)}
                            className="h-6 w-6 p-0 text-gray-400 hover:text-white"
                          >
                            <Copy className="w-3 h-3" />
                          </Button>
                        </div>
                      </div>
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-gray-400">Token:</span>
                        <Badge className="bg-blue-steel/20 text-blue-steel border-blue-steel/30">
                          {selectedWalletData.token_symbol}
                        </Badge>
                      </div>
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-gray-400">Analyzed:</span>
                        <span className="text-sm text-white">
                          {new Date(selectedWalletData.analyzed_at).toLocaleString()}
                        </span>
                      </div>
                    </CardContent>
                  </Card>

                  <Card className="glass-card border-blue-ice/20">
                    <CardHeader className="pb-3">
                      <CardTitle className="text-sm font-medium text-gray-400 flex items-center">
                        <TrendingUpIcon className="w-4 h-4 mr-2" />
                        Performance Summary
                      </CardTitle>
                    </CardHeader>
                    <CardContent className="space-y-3">
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-gray-400">Total P&L:</span>
                        {getPnLBadge(parseFloat(selectedWalletData.total_pnl_usd))}
                      </div>
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-gray-400">Win Rate:</span>
                        <span className={`text-sm font-medium ${getWinRateColor(parseFloat(selectedWalletData.win_rate))}`}>
                          {formatPercentage(parseFloat(selectedWalletData.win_rate))}
                        </span>
                      </div>
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-gray-400">Total Trades:</span>
                        <span className="text-sm font-medium text-white">
                          {selectedWalletData.total_trades}
                        </span>
                      </div>
                    </CardContent>
                  </Card>
                </motion.div>

                <div className="w-full h-px bg-blue-ice/20 my-4" />

                {/* P&L Breakdown Section */}
                <motion.div
                  initial={{ opacity: 0, y: 20 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ duration: 0.4, delay: 0.1 }}
                >
                  <Card className="glass-card border-blue-ice/20">
                    <CardHeader>
                      <CardTitle className="text-lg font-bold text-white flex items-center">
                        <BarChart3 className="w-5 h-5 mr-2 text-cyan-bright" />
                        Profit & Loss Breakdown
                      </CardTitle>
                    </CardHeader>
                    <CardContent>
                      <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
                        <div className="space-y-2">
                          <div className="flex items-center space-x-2">
                            <DollarSign className="w-4 h-4 text-green-success" />
                            <span className="text-sm font-medium text-gray-400">Total P&L</span>
                          </div>
                          <div className={`text-2xl font-bold ${
                            parseFloat(selectedWalletData.total_pnl_usd) >= 0 
                              ? 'text-green-success' 
                              : 'text-red-400'
                          }`}>
                            {formatCurrency(parseFloat(selectedWalletData.total_pnl_usd))}
                          </div>
                          <div className="text-xs text-gray-500">
                            Overall portfolio performance
                          </div>
                        </div>

                        <div className="space-y-2">
                          <div className="flex items-center space-x-2">
                            <Zap className="w-4 h-4 text-cyan-bright" />
                            <span className="text-sm font-medium text-gray-400">Realized P&L</span>
                          </div>
                          <div className={`text-2xl font-bold ${
                            parseFloat(selectedWalletData.realized_pnl_usd) >= 0 
                              ? 'text-cyan-bright' 
                              : 'text-red-400'
                          }`}>
                            {formatCurrency(parseFloat(selectedWalletData.realized_pnl_usd))}
                          </div>
                          <div className="text-xs text-gray-500">
                            Profits/losses from closed positions
                          </div>
                        </div>

                        <div className="space-y-2">
                          <div className="flex items-center space-x-2">
                            <Clock className="w-4 h-4 text-orange-warning" />
                            <span className="text-sm font-medium text-gray-400">Unrealized P&L</span>
                          </div>
                          <div className={`text-2xl font-bold ${
                            parseFloat(selectedWalletData.unrealized_pnl_usd) >= 0 
                              ? 'text-orange-warning' 
                              : 'text-red-400'
                          }`}>
                            {formatCurrency(parseFloat(selectedWalletData.unrealized_pnl_usd))}
                          </div>
                          <div className="text-xs text-gray-500">
                            Current open position value
                          </div>
                        </div>
                      </div>
                    </CardContent>
                  </Card>
                </motion.div>

                {/* Trading Metrics Section */}
                <motion.div
                  initial={{ opacity: 0, y: 20 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ duration: 0.4, delay: 0.2 }}
                  className="grid grid-cols-1 md:grid-cols-2 gap-4"
                >
                  <Card className="glass-card border-blue-ice/20">
                    <CardHeader className="pb-3">
                      <CardTitle className="text-sm font-medium text-gray-400 flex items-center">
                        <Target className="w-4 h-4 mr-2" />
                        Trading Performance
                      </CardTitle>
                    </CardHeader>
                    <CardContent className="space-y-4">
                      <div className="space-y-2">
                        <div className="flex justify-between items-center">
                          <span className="text-sm text-gray-400">Win Rate</span>
                          <span className={`font-bold ${getWinRateColor(parseFloat(selectedWalletData.win_rate))}`}>
                            {formatPercentage(parseFloat(selectedWalletData.win_rate))}
                          </span>
                        </div>
                        <div className="w-full bg-navy-deep rounded-full h-2">
                          <div 
                            className={`h-2 rounded-full transition-all duration-300 ${
                              parseFloat(selectedWalletData.win_rate) >= 70 
                                ? 'bg-green-success' 
                                : parseFloat(selectedWalletData.win_rate) >= 50 
                                ? 'bg-orange-warning' 
                                : 'bg-red-400'
                            }`}
                            style={{ width: `${Math.min(parseFloat(selectedWalletData.win_rate), 100)}%` }}
                          />
                        </div>
                      </div>

                      <div className="flex justify-between items-center">
                        <span className="text-sm text-gray-400">Total Trades</span>
                        <div className="flex items-center space-x-2">
                          <Activity className="w-4 h-4 text-blue-steel" />
                          <span className="font-bold text-white">{selectedWalletData.total_trades}</span>
                        </div>
                      </div>

                      <div className="flex justify-between items-center">
                        <span className="text-sm text-gray-400">Avg Hold Time</span>
                        <div className="flex items-center space-x-2">
                          <Clock className="w-4 h-4 text-purple-400" />
                          <span className="font-bold text-white">
                            {Math.round(parseFloat(selectedWalletData.avg_hold_time_minutes))}m
                          </span>
                        </div>
                      </div>
                    </CardContent>
                  </Card>

                  <Card className="glass-card border-blue-ice/20">
                    <CardHeader className="pb-3">
                      <CardTitle className="text-sm font-medium text-gray-400 flex items-center">
                        <Award className="w-4 h-4 mr-2" />
                        Trading Style Analysis
                      </CardTitle>
                    </CardHeader>
                    <CardContent className="space-y-4">
                      <div className="space-y-3">
                        <div className="flex items-center justify-between">
                          <span className="text-sm text-gray-400">Style:</span>
                          <Badge className={
                            Math.round(parseFloat(selectedWalletData.avg_hold_time_minutes)) < 60
                              ? "bg-red-500/20 text-red-400 border-red-500/30"
                              : Math.round(parseFloat(selectedWalletData.avg_hold_time_minutes)) < 1440
                              ? "bg-orange-warning/20 text-orange-warning border-orange-warning/30"
                              : "bg-green-success/20 text-green-success border-green-success/30"
                          }>
                            {Math.round(parseFloat(selectedWalletData.avg_hold_time_minutes)) < 60 
                              ? "Scalper" 
                              : Math.round(parseFloat(selectedWalletData.avg_hold_time_minutes)) < 1440 
                              ? "Day Trader" 
                              : "Swing Trader"}
                          </Badge>
                        </div>

                        <div className="flex items-center justify-between">
                          <span className="text-sm text-gray-400">Performance:</span>
                          <Badge className={
                            parseFloat(selectedWalletData.win_rate) >= 70 && parseFloat(selectedWalletData.total_pnl_usd) > 0
                              ? "bg-green-success/20 text-green-success border-green-success/30"
                              : parseFloat(selectedWalletData.win_rate) >= 50 || parseFloat(selectedWalletData.total_pnl_usd) > 0
                              ? "bg-orange-warning/20 text-orange-warning border-orange-warning/30"
                              : "bg-red-500/20 text-red-400 border-red-500/30"
                          }>
                            {parseFloat(selectedWalletData.win_rate) >= 70 && parseFloat(selectedWalletData.total_pnl_usd) > 0
                              ? "Elite Trader"
                              : parseFloat(selectedWalletData.win_rate) >= 50 || parseFloat(selectedWalletData.total_pnl_usd) > 0
                              ? "Profitable"
                              : "Needs Improvement"}
                          </Badge>
                        </div>

                        <div className="flex items-center justify-between">
                          <span className="text-sm text-gray-400">Activity:</span>
                          <Badge className={
                            selectedWalletData.total_trades >= 100
                              ? "bg-cyan-bright/20 text-cyan-bright border-cyan-bright/30"
                              : selectedWalletData.total_trades >= 20
                              ? "bg-blue-steel/20 text-blue-steel border-blue-steel/30"
                              : "bg-gray-500/20 text-gray-400 border-gray-500/30"
                          }>
                            {selectedWalletData.total_trades >= 100 
                              ? "High Volume" 
                              : selectedWalletData.total_trades >= 20 
                              ? "Active" 
                              : "Casual"}
                          </Badge>
                        </div>
                      </div>
                    </CardContent>
                  </Card>
                </motion.div>

                {/* ROI and Risk Assessment */}
                <motion.div
                  initial={{ opacity: 0, y: 20 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ duration: 0.4, delay: 0.3 }}
                >
                  <Card className="glass-card border-blue-ice/20">
                    <CardHeader>
                      <CardTitle className="text-lg font-bold text-white flex items-center">
                        <AlertTriangle className="w-5 h-5 mr-2 text-yellow-500" />
                        ROI & Risk Assessment
                      </CardTitle>
                    </CardHeader>
                    <CardContent>
                      <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
                        <div className="space-y-2">
                          <div className="flex items-center space-x-2">
                            <TrendingUp className="w-4 h-4 text-green-success" />
                            <span className="text-sm font-medium text-gray-400">ROI Percentage</span>
                          </div>
                          <div className={`text-2xl font-bold ${
                            parseFloat(selectedWalletData.roi_percentage) >= 0 
                              ? 'text-green-success' 
                              : 'text-red-400'
                          }`}>
                            {formatPercentage(parseFloat(selectedWalletData.roi_percentage))}
                          </div>
                          <div className="text-xs text-gray-500">
                            Return on investment ratio
                          </div>
                        </div>

                        <div className="space-y-2">
                          <div className="flex items-center space-x-2">
                            <BarChart3 className="w-4 h-4 text-blue-steel" />
                            <span className="text-sm font-medium text-gray-400">Avg P&L per Trade</span>
                          </div>
                          <div className={`text-2xl font-bold ${
                            parseFloat(selectedWalletData.total_pnl_usd) / selectedWalletData.total_trades >= 0 
                              ? 'text-blue-steel' 
                              : 'text-red-400'
                          }`}>
                            {formatCurrency(parseFloat(selectedWalletData.total_pnl_usd) / selectedWalletData.total_trades)}
                          </div>
                          <div className="text-xs text-gray-500">
                            Average profit per trade
                          </div>
                        </div>

                        <div className="space-y-2">
                          <div className="flex items-center space-x-2">
                            <Target className="w-4 h-4 text-purple-400" />
                            <span className="text-sm font-medium text-gray-400">Copy Trading Score</span>
                          </div>
                          <div className={`text-2xl font-bold ${
                            parseFloat(selectedWalletData.win_rate) >= 70 && parseFloat(selectedWalletData.total_pnl_usd) > 5000
                              ? 'text-green-success'
                              : parseFloat(selectedWalletData.win_rate) >= 50 && parseFloat(selectedWalletData.total_pnl_usd) > 1000
                              ? 'text-orange-warning'
                              : 'text-red-400'
                          }`}>
                            {Math.round(
                              (parseFloat(selectedWalletData.win_rate) / 100) * 50 +
                              Math.min(Math.log10(Math.abs(parseFloat(selectedWalletData.total_pnl_usd)) + 1) * 10, 30) +
                              Math.min(selectedWalletData.total_trades / 10, 20)
                            )}/100
                          </div>
                          <div className="text-xs text-gray-500">
                            Algorithmic copy-worthiness rating
                          </div>
                        </div>
                      </div>
                    </CardContent>
                  </Card>
                </motion.div>
              </div>

              <DialogFooter className="flex items-center space-x-2">
                <Button 
                  variant="outline"
                  onClick={() => window.open(`https://solscan.io/account/${selectedWalletData.wallet_address}`, '_blank')}
                  className="border-blue-steel/50 text-blue-steel hover:bg-blue-steel/20"
                >
                  <ExternalLink className="w-4 h-4 mr-2" />
                  View on Solscan
                </Button>
                <Button 
                  variant="outline"
                  onClick={() => window.open(`https://gmgn.ai/sol/address/${selectedWalletData.wallet_address}`, '_blank')}
                  className="border-green-success/50 text-green-success hover:bg-green-success/20"
                >
                  <ExternalLink className="w-4 h-4 mr-2" />
                  View on GMGN
                </Button>
                <Button 
                  variant="outline"
                  onClick={() => copyToClipboard(selectedWalletData.wallet_address)}
                  className="border-cyan-bright/50 text-cyan-bright hover:bg-cyan-bright/20"
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

      {/* Trade History Modal */}
      <AnimatePresence>
        {showTradeHistoryModal && tradeHistoryData && (
          <Dialog open={!!showTradeHistoryModal} onOpenChange={() => setShowTradeHistoryModal(null)}>
            <DialogContent className="max-w-6xl max-h-[90vh] overflow-y-auto bg-navy-deep border-blue-ice/20">
              <DialogHeader>
                <DialogTitle className="text-xl font-bold text-white flex items-center space-x-2">
                  <Activity className="w-6 h-6 text-green-success" />
                  <span>Complete Trading History & Holdings</span>
                </DialogTitle>
                <DialogDescription className="text-gray-400">
                  Full P&L report with all trades and current positions for {truncateAddress(showTradeHistoryModal)}
                </DialogDescription>
              </DialogHeader>

              <div className="space-y-6">
                {/* Current Holdings Section */}
                <motion.div
                  initial={{ opacity: 0, y: 20 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ duration: 0.4 }}
                >
                  <Card className="glass-card border-blue-ice/20">
                    <CardHeader>
                      <CardTitle className="text-lg font-bold text-white flex items-center">
                        <Wallet className="w-5 h-5 mr-2 text-cyan-bright" />
                        Current Holdings
                      </CardTitle>
                    </CardHeader>
                    <CardContent>
                      <div className="overflow-x-auto">
                        <table className="w-full">
                          <thead>
                            <tr className="border-b border-blue-ice/20">
                              <th className="text-left py-2 px-3 font-medium text-gray-400">Token</th>
                              <th className="text-left py-2 px-3 font-medium text-gray-400">Amount</th>
                              <th className="text-left py-2 px-3 font-medium text-gray-400">Current Price</th>
                              <th className="text-left py-2 px-3 font-medium text-gray-400">Current Value</th>
                              <th className="text-left py-2 px-3 font-medium text-gray-400">Avg Buy Price</th>
                              <th className="text-left py-2 px-3 font-medium text-gray-400">Unrealized P&L</th>
                            </tr>
                          </thead>
                          <tbody>
                            {tradeHistoryData.holdings.map((holding, index) => (
                              <tr key={holding.token_address} className="border-b border-blue-ice/10 hover:bg-navy-deep/30">
                                <td className="py-2 px-3">
                                  <Badge className="bg-blue-steel/20 text-blue-steel border-blue-steel/30">
                                    {holding.token_symbol}
                                  </Badge>
                                </td>
                                <td className="py-2 px-3 text-white font-medium">
                                  {parseFloat(holding.amount).toLocaleString()}
                                </td>
                                <td className="py-2 px-3 text-white">
                                  ${holding.current_price_usd}
                                </td>
                                <td className="py-2 px-3 text-white font-medium">
                                  ${parseFloat(holding.current_value_usd).toLocaleString()}
                                </td>
                                <td className="py-2 px-3 text-gray-400">
                                  ${holding.avg_buy_price}
                                </td>
                                <td className="py-2 px-3">
                                  <div className={`font-medium ${
                                    parseFloat(holding.unrealized_pnl_usd) >= 0 ? 'text-green-success' : 'text-red-400'
                                  }`}>
                                    ${holding.unrealized_pnl_usd}
                                  </div>
                                  <div className={`text-xs ${
                                    parseFloat(holding.unrealized_pnl_percentage) >= 0 ? 'text-green-success' : 'text-red-400'
                                  }`}>
                                    ({holding.unrealized_pnl_percentage}%)
                                  </div>
                                </td>
                              </tr>
                            ))}
                          </tbody>
                        </table>
                      </div>
                    </CardContent>
                  </Card>
                </motion.div>

                <div className="w-full h-px bg-blue-ice/20 my-4" />

                {/* Trade History Section */}
                <motion.div
                  initial={{ opacity: 0, y: 20 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ duration: 0.4, delay: 0.1 }}
                >
                  <Card className="glass-card border-blue-ice/20">
                    <CardHeader>
                      <CardTitle className="text-lg font-bold text-white flex items-center justify-between">
                        <div className="flex items-center">
                          <BarChart3 className="w-5 h-5 mr-2 text-orange-warning" />
                          All Trades & Transactions
                        </div>
                        <div className="text-sm text-gray-400">
                          {tradeHistoryData?.trades?.length || 0} transactions
                        </div>
                      </CardTitle>
                    </CardHeader>
                    <CardContent>
                      <div className="overflow-x-auto">
                        <table className="w-full">
                          <thead>
                            <tr className="border-b border-blue-ice/20">
                              <th className="text-left py-2 px-3 font-medium text-gray-400">Date</th>
                              <th className="text-left py-2 px-3 font-medium text-gray-400">Type</th>
                              <th className="text-left py-2 px-3 font-medium text-gray-400">Token</th>
                              <th className="text-left py-2 px-3 font-medium text-gray-400">Amount</th>
                              <th className="text-left py-2 px-3 font-medium text-gray-400">Price</th>
                              <th className="text-left py-2 px-3 font-medium text-gray-400">Value</th>
                              <th className="text-left py-2 px-3 font-medium text-gray-400">P&L</th>
                              <th className="text-left py-2 px-3 font-medium text-gray-400">Fees</th>
                              <th className="text-left py-2 px-3 font-medium text-gray-400">Signature</th>
                            </tr>
                          </thead>
                          <tbody>
                            {tradeHistoryData.trades.map((trade, index) => (
                              <motion.tr 
                                key={trade.id}
                                className="border-b border-blue-ice/10 hover:bg-navy-deep/30"
                                initial={{ opacity: 0, x: -20 }}
                                animate={{ opacity: 1, x: 0 }}
                                transition={{ duration: 0.3, delay: index * 0.02 }}
                              >
                                <td className="py-2 px-3 text-sm text-gray-400">
                                  {new Date(trade.timestamp).toLocaleDateString()}
                                  <div className="text-xs text-gray-500">
                                    {new Date(trade.timestamp).toLocaleTimeString()}
                                  </div>
                                </td>
                                <td className="py-2 px-3">
                                  <Badge className={
                                    trade.type === 'buy' 
                                      ? "bg-green-success/20 text-green-success border-green-success/30"
                                      : trade.type === 'sell'
                                      ? "bg-red-500/20 text-red-400 border-red-500/30"
                                      : trade.type === 'transfer_in'
                                      ? "bg-blue-steel/20 text-blue-steel border-blue-steel/30"
                                      : "bg-orange-warning/20 text-orange-warning border-orange-warning/30"
                                  }>
                                    {trade.type.replace('_', ' ').toUpperCase()}
                                  </Badge>
                                </td>
                                <td className="py-2 px-3">
                                  <Badge className="bg-blue-steel/20 text-blue-steel border-blue-steel/30">
                                    {trade.token_symbol}
                                  </Badge>
                                </td>
                                <td className="py-2 px-3 text-white font-medium">
                                  {parseFloat(trade.amount).toLocaleString()}
                                </td>
                                <td className="py-2 px-3 text-white">
                                  ${trade.price_usd}
                                </td>
                                <td className="py-2 px-3 text-white font-medium">
                                  ${parseFloat(trade.value_usd).toLocaleString()}
                                </td>
                                <td className="py-2 px-3">
                                  {trade.realized_pnl ? (
                                    <div className={`font-medium ${
                                      parseFloat(trade.realized_pnl) >= 0 ? 'text-green-success' : 'text-red-400'
                                    }`}>
                                      ${trade.realized_pnl}
                                    </div>
                                  ) : (
                                    <span className="text-gray-500">-</span>
                                  )}
                                </td>
                                <td className="py-2 px-3 text-gray-400">
                                  ${trade.fees_usd}
                                </td>
                                <td className="py-2 px-3">
                                  <button
                                    onClick={() => window.open(`https://solscan.io/tx/${trade.signature}`, '_blank')}
                                    className="font-mono text-xs text-cyan-bright hover:text-cyan-bright/80 hover:underline"
                                  >
                                    {trade.signature}
                                  </button>
                                </td>
                              </motion.tr>
                            ))}
                          </tbody>
                        </table>
                      </div>
                    </CardContent>
                  </Card>
                </motion.div>
              </div>

              <DialogFooter className="flex items-center space-x-2">
                <Button 
                  variant="outline"
                  onClick={() => window.open(`https://solscan.io/account/${showTradeHistoryModal}`, '_blank')}
                  className="border-blue-steel/50 text-blue-steel hover:bg-blue-steel/20"
                >
                  <ExternalLink className="w-4 h-4 mr-2" />
                  View on Solscan
                </Button>
                <Button 
                  variant="outline"
                  onClick={() => window.open(`https://gmgn.ai/sol/address/${showTradeHistoryModal}`, '_blank')}
                  className="border-green-success/50 text-green-success hover:bg-green-success/20"
                >
                  <ExternalLink className="w-4 h-4 mr-2" />
                  View on GMGN
                </Button>
                <Button 
                  variant="outline"
                  onClick={() => copyToClipboard(showTradeHistoryModal)}
                  className="border-cyan-bright/50 text-cyan-bright hover:bg-cyan-bright/20"
                >
                  <Copy className="w-4 h-4 mr-2" />
                  Copy Address
                </Button>
                <Button 
                  onClick={() => setShowTradeHistoryModal(null)}
                  className="bg-green-success/10 border-green-success/20 text-green-success hover:bg-green-success/20"
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