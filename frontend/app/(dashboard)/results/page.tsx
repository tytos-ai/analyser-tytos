'use client'

import { useState, useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { ChainBadge } from '@/components/ui/chain-badge'
import { ChainSelectItem, AllChainsSelectItem } from '@/components/ui/chain-select-item'
import { SortSelectItem, sortOptions } from '@/components/ui/sort-select-item'
import { TokenTabs } from '@/components/ui/token-tabs'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { LoadingSpinner } from '@/components/ui/loading-spinner'
import { api } from '@/lib/api'
import { formatCurrency, formatPercentage, truncateAddress, getPnLColorClass, getWinRateColorClass } from '@/lib/utils'
import { 
  Download, Filter, Search, Eye, TrendingUp, TrendingDown, 
  ChevronLeft, ChevronRight, ArrowUpDown, ExternalLink,
  BarChart3, Wallet, Target, Calendar, DollarSign, Copy,
  Activity, Clock, Award, Zap, TrendingUpIcon, AlertTriangle,
  ChevronDown, ChevronUp, Coins, PieChart
} from 'lucide-react'
import { motion, AnimatePresence } from 'framer-motion'
import type { WalletDetailResponse } from '@/types/api'

interface WalletResult {
  wallet_address: string
  chain: string
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
  const [selectedChain, setSelectedChain] = useState<string>('all')
  const [showAdvancedFilters, setShowAdvancedFilters] = useState(false)
  const itemsPerPage = 15

  // Fetch results data
  const { data: resultsData, isLoading, error } = useQuery<ResultsResponse>({
    queryKey: ['results', currentPage, sortBy, sortOrder, minPnl, maxPnl, minWinRate, selectedChain],
    queryFn: () => api.results.getResults({
      limit: itemsPerPage,
      offset: (currentPage - 1) * itemsPerPage,
      min_pnl: minPnl ? parseFloat(minPnl) : undefined,
      max_pnl: maxPnl ? parseFloat(maxPnl) : undefined,
      min_win_rate: minWinRate ? parseFloat(minWinRate) : undefined,
    }),
    refetchInterval: 30000 // Refresh every 30 seconds
  })

  // Get detailed data for the selected wallet
  const selectedWalletData = useMemo(() => {
    if (!showDetailModal || !resultsData?.results) return null
    return resultsData.results.find(r => r.wallet_address === showDetailModal)
  }, [showDetailModal, resultsData?.results])

  // Fetch detailed portfolio data when modal opens
  const { data: portfolioDetailData, isLoading: isLoadingPortfolio } = useQuery<WalletDetailResponse>({
    queryKey: ['walletDetail', showDetailModal, selectedWalletData?.chain],
    queryFn: () => api.results.getWalletDetail(
      showDetailModal!, 
      selectedWalletData?.token_address || 'portfolio',
      selectedWalletData?.chain
    ),
    enabled: !!showDetailModal && !!selectedWalletData,
    staleTime: 5 * 60 * 1000, // 5 minutes
  })

  // Get wallet data for trade history modal
  const tradeHistoryWalletData = useMemo(() => {
    if (!showTradeHistoryModal || !resultsData?.results) return null
    return resultsData.results.find(r => r.wallet_address === showTradeHistoryModal)
  }, [showTradeHistoryModal, resultsData?.results])

  // Apply client-side filtering and sorting
  const filteredAndSortedResults = useMemo(() => {
    if (!resultsData?.results) return []
    
    let filtered = [...resultsData.results]
    
    // Apply chain filter
    if (selectedChain !== 'all') {
      filtered = filtered.filter(r => r.chain.toLowerCase() === selectedChain.toLowerCase())
    }

    // Apply hold time filters
    if (minHoldTime) {
      filtered = filtered.filter(r => parseFloat(r.avg_hold_time_minutes) >= parseFloat(minHoldTime))
    }
    if (maxHoldTime) {
      filtered = filtered.filter(r => parseFloat(r.avg_hold_time_minutes) <= parseFloat(maxHoldTime))
    }

    // Apply win rate filters (convert from percentage)
    if (maxWinRate) {
      filtered = filtered.filter(r => parseFloat(r.win_rate) <= parseFloat(maxWinRate) / 100)
    }
    
    // Sort results
    filtered.sort((a, b) => {
      let comparison = 0
      
      switch (sortBy) {
        case 'pnl':
          comparison = parseFloat(a.total_pnl_usd) - parseFloat(b.total_pnl_usd)
          break
        case 'win_rate':
          comparison = parseFloat(a.win_rate) - parseFloat(b.win_rate)
          break
        case 'trades':
          comparison = a.total_trades - b.total_trades
          break
        case 'analyzed_at':
          comparison = new Date(a.analyzed_at).getTime() - new Date(b.analyzed_at).getTime()
          break
        case 'hold_time':
          comparison = parseFloat(a.avg_hold_time_minutes) - parseFloat(b.avg_hold_time_minutes)
          break
        case 'composite':
          // Composite score: 40% P&L, 30% win rate, 20% trades, 10% hold time
          const scoreA = (
            parseFloat(a.total_pnl_usd) * 0.4 +
            parseFloat(a.win_rate) * 10000 * 0.3 +
            a.total_trades * 100 * 0.2 +
            (1 / (parseFloat(a.avg_hold_time_minutes) + 1)) * 1000 * 0.1
          )
          const scoreB = (
            parseFloat(b.total_pnl_usd) * 0.4 +
            parseFloat(b.win_rate) * 10000 * 0.3 +
            b.total_trades * 100 * 0.2 +
            (1 / (parseFloat(b.avg_hold_time_minutes) + 1)) * 1000 * 0.1
          )
          comparison = scoreA - scoreB
          break
      }
      
      return sortOrder === 'desc' ? -comparison : comparison
    })
    
    return filtered
  }, [resultsData?.results, sortBy, sortOrder, minHoldTime, maxHoldTime, maxWinRate, selectedChain])

  const totalPages = Math.ceil((resultsData?.pagination.total_count || 0) / itemsPerPage)

  const handleSort = (newSortBy: typeof sortBy) => {
    if (sortBy === newSortBy) {
      setSortOrder(sortOrder === 'desc' ? 'asc' : 'desc')
    } else {
      setSortBy(newSortBy)
      setSortOrder('desc')
    }
  }

  const clearFilters = () => {
    setMinPnl('')
    setMaxPnl('')
    setMinWinRate('')
    setMaxWinRate('')
    setMinHoldTime('')
    setMaxHoldTime('')
    setSelectedChain('all')
  }

  const hasActiveFilters = minPnl || maxPnl || minWinRate || maxWinRate || minHoldTime || maxHoldTime || selectedChain !== 'all'

  const getWinRateColor = (winRate: number) => {
    return getWinRateColorClass(winRate * 100) // Convert to percentage
  }

  const getPnLBadge = (pnl: number) => {
    const colorClass = getPnLColorClass(pnl)
    return (
      <div className={`flex items-center space-x-1 ${colorClass}`}>
        {pnl > 0 ? <TrendingUp className="w-4 h-4" /> : pnl < 0 ? <TrendingDown className="w-4 h-4" /> : null}
        <span className="font-semibold">{formatCurrency(pnl)}</span>
      </div>
    )
  }

  const exportToCSV = () => {
    if (!filteredAndSortedResults.length) return

    const headers = [
      'Wallet Address',
      'Chain',
      'Token',
      'Total P&L',
      'Realized P&L',
      'Unrealized P&L',
      'ROI %',
      'Win Rate',
      'Total Trades',
      'Avg Hold Time (min)',
      'Analyzed At'
    ]
    
    const csvData = filteredAndSortedResults.map(result => [
      result.wallet_address,
      result.chain,
      result.token_symbol,
      result.total_pnl_usd,
      result.realized_pnl_usd,
      result.unrealized_pnl_usd,
      result.roi_percentage,
      (parseFloat(result.win_rate) * 100).toFixed(2) + '%',
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

  const getExplorerUrl = (address: string, chain: string) => {
    switch (chain.toLowerCase()) {
      case 'solana':
        return `https://solscan.io/account/${address}`
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
                <p className={`text-2xl font-bold ${getPnLColorClass(resultsData?.summary.total_pnl_usd || '0')}`}>
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
                <p className={`text-2xl font-bold ${getPnLColorClass(resultsData?.summary.average_pnl_usd || '0')}`}>
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
                <p className="text-xs text-gray-500">
                  {resultsData?.summary.last_updated 
                    ? `Updated ${new Date(resultsData.summary.last_updated).toLocaleDateString()}` 
                    : 'Real-time'}
                </p>
              </div>
              <Activity className="w-8 h-8 text-purple-400" />
            </div>
          </CardContent>
        </Card>
      </motion.div>

      {/* Filters and Controls */}
      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.6, delay: 0.2 }}
      >
        <Card className="glass-card border-blue-ice/20">
          <CardContent className="p-4">
            {/* Main Filters Row */}
            <div className="flex flex-col lg:flex-row gap-4 items-end">
              <div className="flex-1 grid grid-cols-1 md:grid-cols-3 gap-4">
                {/* Chain Filter */}
                <div>
                  <label className="text-sm text-gray-400 mb-1 block">Chain</label>
                  <Select value={selectedChain} onValueChange={setSelectedChain}>
                    <SelectTrigger className="bg-navy-deep border-blue-ice/20 text-white">
                      <SelectValue placeholder="Select chain" />
                    </SelectTrigger>
                    <SelectContent className="bg-navy-deep border-blue-ice/20">
                      <SelectItem value="all">
                        <AllChainsSelectItem />
                      </SelectItem>
                      <SelectItem value="solana">
                        <ChainSelectItem chain="solana" label="Solana" />
                      </SelectItem>
                      <SelectItem value="ethereum">
                        <ChainSelectItem chain="ethereum" label="Ethereum" />
                      </SelectItem>
                      <SelectItem value="bsc">
                        <ChainSelectItem chain="bsc" label="BSC" />
                      </SelectItem>
                      <SelectItem value="base">
                        <ChainSelectItem chain="base" label="Base" />
                      </SelectItem>
                    </SelectContent>
                  </Select>
                </div>

                {/* Sort By */}
                <div>
                  <label className="text-sm text-gray-400 mb-1 block">Sort By</label>
                  <Select value={sortBy} onValueChange={(value: any) => setSortBy(value)}>
                    <SelectTrigger className="bg-navy-deep border-blue-ice/20 text-white">
                      <SelectValue placeholder="Sort by" />
                    </SelectTrigger>
                    <SelectContent className="bg-navy-deep border-blue-ice/20">
                      {Object.entries(sortOptions).map(([key, { icon, label }]) => (
                        <SelectItem key={key} value={key}>
                          <SortSelectItem icon={icon} label={label} />
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>

                {/* Sort Order */}
                <div>
                  <label className="text-sm text-gray-400 mb-1 block">Order</label>
                  <Button
                    variant="outline"
                    onClick={() => setSortOrder(sortOrder === 'desc' ? 'asc' : 'desc')}
                    className="w-full bg-navy-deep border-blue-ice/20 text-white hover:bg-blue-ice/20"
                  >
                    {sortOrder === 'desc' ? (
                      <>
                        <ChevronDown className="w-4 h-4 mr-2" />
                        Descending
                      </>
                    ) : (
                      <>
                        <ChevronUp className="w-4 h-4 mr-2" />
                        Ascending
                      </>
                    )}
                  </Button>
                </div>
              </div>

              <div className="flex gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => setShowAdvancedFilters(!showAdvancedFilters)}
                  className={`border-blue-ice/20 ${
                    showAdvancedFilters ? 'bg-blue-ice/20 text-white' : 'text-gray-400 hover:text-white'
                  }`}
                >
                  <Filter className="w-4 h-4 mr-2" />
                  Advanced
                  {hasActiveFilters && (
                    <Badge variant="destructive" className="ml-2 px-1 py-0 text-xs">
                      {[minPnl, maxPnl, minWinRate, maxWinRate, minHoldTime, maxHoldTime, selectedChain !== 'all'].filter(Boolean).length}
                    </Badge>
                  )}
                </Button>
                {hasActiveFilters && (
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={clearFilters}
                    className="border-red-400/20 text-red-400 hover:bg-red-400/20"
                  >
                    Clear
                  </Button>
                )}
              </div>
            </div>

            {/* Advanced Filters */}
            <AnimatePresence>
              {showAdvancedFilters && (
                <motion.div
                  initial={{ height: 0, opacity: 0 }}
                  animate={{ height: 'auto', opacity: 1 }}
                  exit={{ height: 0, opacity: 0 }}
                  transition={{ duration: 0.3 }}
                  className="overflow-hidden"
                >
                  <div className="grid grid-cols-1 md:grid-cols-3 gap-4 mt-4 pt-4 border-t border-blue-ice/20">
                    <div>
                      <label className="text-sm text-gray-400 mb-1 block">Min P&L</label>
                      <Input
                        type="number"
                        value={minPnl}
                        onChange={(e) => setMinPnl(e.target.value)}
                        placeholder="e.g. 1000"
                        className="bg-navy-deep border-blue-ice/20 text-white"
                      />
                    </div>
                    <div>
                      <label className="text-sm text-gray-400 mb-1 block">Max P&L</label>
                      <Input
                        type="number"
                        value={maxPnl}
                        onChange={(e) => setMaxPnl(e.target.value)}
                        placeholder="e.g. 50000"
                        className="bg-navy-deep border-blue-ice/20 text-white"
                      />
                    </div>
                    <div>
                      <label className="text-sm text-gray-400 mb-1 block">Min Win Rate (%)</label>
                      <Input
                        type="number"
                        value={minWinRate}
                        onChange={(e) => setMinWinRate(e.target.value)}
                        placeholder="e.g. 60"
                        min="0"
                        max="100"
                        className="bg-navy-deep border-blue-ice/20 text-white"
                      />
                    </div>
                    <div>
                      <label className="text-sm text-gray-400 mb-1 block">Max Win Rate (%)</label>
                      <Input
                        type="number"
                        value={maxWinRate}
                        onChange={(e) => setMaxWinRate(e.target.value)}
                        placeholder="e.g. 90"
                        min="0"
                        max="100"
                        className="bg-navy-deep border-blue-ice/20 text-white"
                      />
                    </div>
                    <div>
                      <label className="text-sm text-gray-400 mb-1 block">Min Hold Time (min)</label>
                      <Input
                        type="number"
                        value={minHoldTime}
                        onChange={(e) => setMinHoldTime(e.target.value)}
                        placeholder="e.g. 5"
                        min="0"
                        className="bg-navy-deep border-blue-ice/20 text-white"
                      />
                    </div>
                    <div>
                      <label className="text-sm text-gray-400 mb-1 block">Max Hold Time (min)</label>
                      <Input
                        type="number"
                        value={maxHoldTime}
                        onChange={(e) => setMaxHoldTime(e.target.value)}
                        placeholder="e.g. 1440"
                        min="0"
                        className="bg-navy-deep border-blue-ice/20 text-white"
                      />
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
          <CardHeader className="pb-4">
            <CardTitle className="text-lg font-semibold text-white">
              Analysis Results
              {filteredAndSortedResults.length > 0 && (
                <span className="ml-2 text-sm text-gray-400">
                  ({filteredAndSortedResults.length} wallets)
                </span>
              )}
            </CardTitle>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <div className="flex items-center justify-center py-12">
                <LoadingSpinner size="lg" />
              </div>
            ) : error ? (
              <div className="text-center py-12">
                <AlertTriangle className="w-12 h-12 text-red-400 mx-auto mb-4" />
                <p className="text-red-400">Failed to load results</p>
                <p className="text-sm text-gray-500 mt-2">Please try again later</p>
              </div>
            ) : filteredAndSortedResults.length === 0 ? (
              <div className="text-center py-12">
                <Search className="w-12 h-12 text-gray-500 mx-auto mb-4" />
                <p className="text-gray-400">No results found</p>
                <p className="text-sm text-gray-500 mt-2">Try adjusting your filters</p>
              </div>
            ) : (
              <div className="space-y-4">
                {filteredAndSortedResults.map((result, index) => (
                  <motion.div
                    key={`${result.wallet_address}-${result.token_address}`}
                    initial={{ opacity: 0, x: -20 }}
                    animate={{ opacity: 1, x: 0 }}
                    transition={{ duration: 0.3, delay: index * 0.02 }}
                    className="group"
                  >
                    <div className="p-4 bg-navy-deep/50 rounded-lg border border-blue-ice/10 hover:border-blue-ice/30 transition-all duration-200">
                      <div className="flex items-center justify-between">
                        <div className="flex items-center space-x-4">
                          <div className="flex-shrink-0">
                            <div className={`w-2 h-2 rounded-full ${
                              parseFloat(result.total_pnl_usd) > 0 ? 'bg-green-success' : 
                              parseFloat(result.total_pnl_usd) < 0 ? 'bg-red-400' : 'bg-gray-400'
                            }`} />
                          </div>
                          <div>
                            <div className="flex items-center space-x-3">
                              <p className="font-mono text-sm text-white">
                                {truncateAddress(result.wallet_address)}
                              </p>
                              <ChainBadge chain={result.chain} />
                              <Badge variant="outline" className="text-xs">
                                {result.token_symbol}
                              </Badge>
                            </div>
                            <div className="flex items-center space-x-4 mt-2 text-sm text-gray-400">
                              <span className="flex items-center">
                                <Target className="w-3 h-3 mr-1" />
                                {formatPercentage(parseFloat(result.win_rate))} win
                              </span>
                              <span className="flex items-center">
                                <Activity className="w-3 h-3 mr-1" />
                                {result.total_trades} trades
                              </span>
                              <span className="flex items-center">
                                <Clock className="w-3 h-3 mr-1" />
                                {Math.round(parseFloat(result.avg_hold_time_minutes))}m avg
                              </span>
                              <span className="flex items-center">
                                <Calendar className="w-3 h-3 mr-1" />
                                {new Date(result.analyzed_at).toLocaleDateString()}
                              </span>
                            </div>
                          </div>
                        </div>
                        
                        <div className="flex items-center space-x-4">
                          <div className="text-right">
                            {getPnLBadge(parseFloat(result.total_pnl_usd))}
                            <p className="text-xs text-gray-500 mt-1">
                              ROI: {formatPercentage(parseFloat(result.roi_percentage))}
                            </p>
                          </div>
                          
                          <div className="flex items-center space-x-2 opacity-0 group-hover:opacity-100 transition-opacity">
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => setShowDetailModal(result.wallet_address)}
                              className="text-cyan-bright hover:bg-cyan-bright/20"
                            >
                              <Eye className="w-4 h-4" />
                            </Button>
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => window.open(getExplorerUrl(result.wallet_address, result.chain), '_blank')}
                              className="text-blue-soft hover:bg-blue-soft/20"
                            >
                              <ExternalLink className="w-4 h-4" />
                            </Button>
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => copyToClipboard(result.wallet_address)}
                              className="text-gray-400 hover:bg-gray-400/20"
                            >
                              <Copy className="w-4 h-4" />
                            </Button>
                          </div>
                        </div>
                      </div>
                    </div>
                  </motion.div>
                ))}
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
              let pageNum
              if (totalPages <= 5) {
                pageNum = i + 1
              } else if (currentPage <= 3) {
                pageNum = i + 1
              } else if (currentPage >= totalPages - 2) {
                pageNum = totalPages - 4 + i
              } else {
                pageNum = currentPage - 2 + i
              }
              
              return (
                <Button
                  key={pageNum}
                  variant={currentPage === pageNum ? "default" : "outline"}
                  size="sm"
                  onClick={() => setCurrentPage(pageNum)}
                  className={currentPage === pageNum 
                    ? "bg-cyan-bright/20 border-cyan-bright text-cyan-bright" 
                    : "border-blue-ice/20 text-gray-400 hover:text-white hover:bg-blue-ice/20"
                  }
                >
                  {pageNum}
                </Button>
              )
            })}
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
                                    getPnLColorClass(portfolioDetailData.portfolio_result.total_pnl_usd)
                                  }`}>
                                    {formatCurrency(parseFloat(portfolioDetailData.portfolio_result.total_pnl_usd))}
                                  </span>
                                </div>

                                <div className="flex items-center justify-between">
                                  <div className="flex items-center gap-2">
                                    <Zap className="w-4 h-4 text-cyan-bright" />
                                    <span className="text-sm text-gray-400">Realized</span>
                                  </div>
                                  <span className={`font-semibold ${
                                    getPnLColorClass(portfolioDetailData.portfolio_result.total_realized_pnl_usd)
                                  }`}>
                                    {formatCurrency(parseFloat(portfolioDetailData.portfolio_result.total_realized_pnl_usd))}
                                  </span>
                                </div>

                                <div className="flex items-center justify-between">
                                  <div className="flex items-center gap-2">
                                    <Clock className="w-4 h-4 text-orange-warning" />
                                    <span className="text-sm text-gray-400">Unrealized</span>
                                  </div>
                                  <span className={`font-semibold ${
                                    getPnLColorClass(portfolioDetailData.portfolio_result.total_unrealized_pnl_usd)
                                  }`}>
                                    {formatCurrency(parseFloat(portfolioDetailData.portfolio_result.total_unrealized_pnl_usd))}
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
                                    getWinRateColorClass(portfolioDetailData.portfolio_result.overall_win_rate_percentage)
                                  }`}>
                                    {formatPercentage(parseFloat(portfolioDetailData.portfolio_result.overall_win_rate_percentage))}
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
                                      W: {Math.round(portfolioDetailData.portfolio_result.total_trades * parseFloat(portfolioDetailData.portfolio_result.overall_win_rate_percentage) / 100)}
                                    </span>
                                    <span className="text-red-400">
                                      L: {portfolioDetailData.portfolio_result.total_trades - Math.round(portfolioDetailData.portfolio_result.total_trades * parseFloat(portfolioDetailData.portfolio_result.overall_win_rate_percentage) / 100)}
                                    </span>
                                  </div>
                                </div>

                                <div className="p-3 bg-navy-deep/50 rounded-lg">
                                  <div className="flex items-center gap-2 mb-1">
                                    <Clock className="w-4 h-4 text-cyan-bright" />
                                    <span className="text-sm text-gray-400">Avg Hold</span>
                                  </div>
                                  <p className="text-lg font-bold text-white">
                                    {Math.round(parseFloat(portfolioDetailData.portfolio_result.avg_hold_time_minutes))}m
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
                                    {portfolioDetailData.portfolio_result.total_invested_usd && !isNaN(parseFloat(portfolioDetailData.portfolio_result.total_invested_usd))
                                      ? formatCurrency(parseFloat(portfolioDetailData.portfolio_result.total_invested_usd))
                                      : 'N/A'
                                    }
                                  </p>
                                </div>
                                <div className="p-3 bg-navy-deep/50 rounded-lg">
                                  <p className="text-xs text-gray-400 mb-1">Total Returned</p>
                                  <p className="text-sm font-semibold text-white">
                                    {portfolioDetailData.portfolio_result.total_returned_usd && !isNaN(parseFloat(portfolioDetailData.portfolio_result.total_returned_usd))
                                      ? formatCurrency(parseFloat(portfolioDetailData.portfolio_result.total_returned_usd))
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
                                      ? `${parseFloat(portfolioDetailData.portfolio_result.profit_percentage) >= 0 ? '+' : ''}${portfolioDetailData.portfolio_result.profit_percentage}%`
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
                  </>
                ) : (
                  /* Fallback to original view if no portfolio data */
                  <>
                    <motion.div
                      initial={{ opacity: 0, y: 20 }}
                      animate={{ opacity: 1, y: 0 }}
                      transition={{ duration: 0.4 }}
                      className="grid grid-cols-1 lg:grid-cols-2 gap-4"
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
                            <span className="text-sm text-gray-400">Chain:</span>
                            <ChainBadge chain={selectedWalletData.chain} />
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
                  </>   
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

      {/* Trade History Modal */}
      <AnimatePresence>
        {showTradeHistoryModal && tradeHistoryData && (
          <Dialog open={!!showTradeHistoryModal} onOpenChange={() => setShowTradeHistoryModal(null)}>
            <DialogContent className="max-w-7xl max-h-[90vh] overflow-y-auto">
              <DialogHeader>
                <DialogTitle className="text-xl font-bold text-white flex items-center space-x-2">
                  <BarChart3 className="w-6 h-6 text-orange-warning" />
                  <span>Trade History</span>
                </DialogTitle>
                <DialogDescription className="text-gray-400">
                  Detailed trading activity for {truncateAddress(showTradeHistoryModal)}
                </DialogDescription>
              </DialogHeader>

              <div className="space-y-6">
                {/* Current Holdings */}
                {tradeHistoryData.holdings.length > 0 && (
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
                        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                          {tradeHistoryData.holdings.map((holding, index) => (
                            <motion.div
                              key={holding.token_address}
                              initial={{ opacity: 0, scale: 0.9 }}
                              animate={{ opacity: 1, scale: 1 }}
                              transition={{ duration: 0.3, delay: index * 0.05 }}
                              className="p-4 bg-navy-deep/50 rounded-lg border border-blue-ice/10"
                            >
                              <div className="flex items-center justify-between mb-3">
                                <h4 className="font-semibold text-white">{holding.token_symbol}</h4>
                                <Badge className={parseFloat(holding.unrealized_pnl_usd) > 0 
                                  ? "bg-green-success/20 text-green-success border-green-success/30" 
                                  : parseFloat(holding.unrealized_pnl_usd) < 0
                                  ? "bg-red-500/20 text-red-400 border-red-500/30"
                                  : "bg-gray-500/20 text-gray-400 border-gray-500/30"
                                }>
                                  {parseFloat(holding.unrealized_pnl_percentage) >= 0 ? '+' : ''}
                                  {holding.unrealized_pnl_percentage}%
                                </Badge>
                              </div>
                              <div className="space-y-2 text-sm">
                                <div className="flex justify-between">
                                  <span className="text-gray-400">Amount:</span>
                                  <span className="text-white">{parseFloat(holding.amount).toLocaleString()}</span>
                                </div>
                                <div className="flex justify-between">
                                  <span className="text-gray-400">Avg Cost:</span>
                                  <span className="text-white">{formatCurrency(parseFloat(holding.avg_buy_price))}</span>
                                </div>
                                <div className="flex justify-between">
                                  <span className="text-gray-400">Current:</span>
                                  <span className="text-white">{formatCurrency(parseFloat(holding.current_price_usd))}</span>
                                </div>
                                <div className="flex justify-between pt-2 border-t border-blue-ice/10">
                                  <span className="text-gray-400">Value:</span>
                                  <span className="font-semibold text-white">{formatCurrency(parseFloat(holding.current_value_usd))}</span>
                                </div>
                                <div className="flex justify-between">
                                  <span className="text-gray-400">P&L:</span>
                                  <span className={`font-semibold ${
                                    getPnLColorClass(holding.unrealized_pnl_usd)
                                  }`}>
                                    {formatCurrency(parseFloat(holding.unrealized_pnl_usd))}
                                  </span>
                                </div>
                              </div>
                            </motion.div>
                          ))}
                        </div>
                      </CardContent>
                    </Card>
                  </motion.div>
                )}

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
                                  <Badge className={trade.type === 'buy' 
                                    ? "bg-green-success/20 text-green-success border-green-success/30" 
                                    : "bg-red-500/20 text-red-400 border-red-500/30"
                                  }>
                                    {trade.type.toUpperCase()}
                                  </Badge>
                                </td>
                                <td className="py-2 px-3 text-sm text-white">{trade.token_symbol}</td>
                                <td className="py-2 px-3 text-sm text-white">{parseFloat(trade.amount).toLocaleString()}</td>
                                <td className="py-2 px-3 text-sm text-white">{formatCurrency(parseFloat(trade.price_usd))}</td>
                                <td className="py-2 px-3 text-sm text-white">{formatCurrency(parseFloat(trade.value_usd))}</td>
                                <td className="py-2 px-3 text-sm">
                                  {trade.realized_pnl ? (
                                    <span className={getPnLColorClass(trade.realized_pnl)}>
                                      {formatCurrency(parseFloat(trade.realized_pnl))}
                                    </span>
                                  ) : (
                                    <span className="text-gray-500">-</span>
                                  )}
                                </td>
                                <td className="py-2 px-3 text-sm text-gray-400">{formatCurrency(parseFloat(trade.fees_usd))}</td>
                                <td className="py-2 px-3">
                                  <Button
                                    variant="ghost"
                                    size="sm"
                                    onClick={() => window.open(`https://solscan.io/tx/${trade.signature}`, '_blank')}
                                    className="text-blue-soft hover:text-cyan-bright text-xs"
                                  >
                                    {trade.signature}
                                    <ExternalLink className="w-3 h-3 ml-1" />
                                  </Button>
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
                  onClick={() => window.open(getExplorerUrl(showTradeHistoryModal, tradeHistoryWalletData?.chain || 'solana'), '_blank')}
                  className="border-blue-steel/50 text-blue-steel hover:bg-blue-steel/20"
                >
                  <ExternalLink className="w-4 h-4 mr-2" />
                  View on Explorer
                </Button>
                <Button 
                  variant="outline"
                  onClick={() => window.open(`https://gmgn.ai/sol/address/${showTradeHistoryModal}`, '_blank')}
                  className="border-green-success/50 text-green-success hover:bg-green-success/20"
                >
                  <BarChart3 className="w-4 h-4 mr-2" />
                  View on GMGN
                </Button>
                <Button 
                  variant="outline"
                  onClick={() => copyToClipboard(showTradeHistoryModal)}
                  className="border-orange-warning/50 text-orange-warning hover:bg-orange-warning/20"
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