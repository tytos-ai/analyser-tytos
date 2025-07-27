import { useState, useEffect, useMemo } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Progress } from '@/components/ui/progress'
import { Skeleton } from '@/components/ui/skeleton'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { useToast } from '@/hooks/use-toast'
import { formatCurrency, formatPercentage, truncateAddress } from '@/lib/utils'
import { api } from '@/lib/api'
import { 
  Plus, 
  Upload, 
  Play, 
  Clock, 
  CheckCircle, 
  AlertCircle, 
  Download,
  Eye,
  X,
  FileText,
  Settings,
  Filter,
  TrendingUp,
  BarChart3,
  Users,
  Wallet,
  ArrowRight,
  ExternalLink,
  Calendar,
  Target
} from 'lucide-react'

// Enhanced Types based on actual API structure
interface BatchJobSubmission {
  wallet_addresses: string[]
  filters?: {
    min_capital_sol?: number
    min_hold_minutes?: number
    min_trades?: number
    min_win_rate?: number
    max_signatures?: number
    max_transactions_to_fetch?: number
    timeframe_filter?: {
      start_time?: string
      end_time?: string
      mode?: string
    }
  }
}

interface BatchJob {
  id: string
  status: 'Pending' | 'Running' | 'Completed' | 'Failed'
  wallet_count: number
  created_at: string
  started_at?: string
  completed_at?: string
  success_count?: number
  failure_count?: number
}

interface BatchJobProgress {
  total_wallets: number
  completed_wallets: number
  successful_wallets: number
  failed_wallets: number
  progress_percentage: number
}

interface WalletResult {
  wallet_address: string
  status: 'success' | 'failed'
  pnl_report?: any
  error_message?: string
}

interface BatchJobResults {
  job_id: string
  status: string
  summary: {
    total_wallets: number
    successful_analyses: number
    failed_analyses: number
    total_pnl_usd: string
    average_pnl_usd: string
    profitable_wallets: number
  }
  results: Record<string, WalletResult>
}

interface WalletAnalysisDetail {
  wallet_address: string
  portfolio_result: any
  copy_trading_metrics?: any
  metadata: {
    analyzed_at: string
    tokens_processed: number
    events_processed: number
    analysis_duration_ms: number
    quality_score: string
  }
}

'use client'

export default function Jobs() {
  const [showSubmissionForm, setShowSubmissionForm] = useState(false)
  const [showResults, setShowResults] = useState<string | null>(null)
  const [showWalletDetail, setShowWalletDetail] = useState<string | null>(null)
  const [activeJobs, setActiveJobs] = useState<string[]>([])
  const [walletInput, setWalletInput] = useState('')
  const [csvFile, setCsvFile] = useState<File | null>(null)
  const [formData, setFormData] = useState<BatchJobSubmission>({
    wallet_addresses: [],
    filters: {
      min_capital_sol: 0.0,
      min_hold_minutes: 0.0,
      min_trades: 0,
      min_win_rate: 0.0,
      max_signatures: 500,
      max_transactions_to_fetch: 1000
    }
  })

  const { toast } = useToast()
  const queryClient = useQueryClient()

  // Fetch job history with pagination
  const { data: jobHistoryData, isLoading: historyLoading } = useQuery({
    queryKey: ['job-history'],
    queryFn: () => api.batch.getJobHistory({ limit: 50, offset: 0 }),
    refetchInterval: 10000
  })

  // Fetch active job statuses
  const activeJobQueries = useQuery({
    queryKey: ['active-jobs', activeJobs],
    queryFn: async () => {
      if (activeJobs.length === 0) return []
      const results = await Promise.all(
        activeJobs.map(async (jobId) => {
          try {
            return await api.batch.getJobStatus(jobId)
          } catch (error) {
            console.error(`Failed to fetch status for job ${jobId}:`, error)
            return null
          }
        })
      )
      return results.filter(Boolean)
    },
    enabled: activeJobs.length > 0,
    refetchInterval: 3000
  })

  // Fetch job results
  const { data: jobResults, isLoading: resultsLoading } = useQuery({
    queryKey: ['job-results', showResults],
    queryFn: () => api.batch.getJobResults(showResults!),
    enabled: !!showResults
  })

  // Get wallet detail from batch job results instead of V2 API
  const walletDetail = useMemo(() => {
    if (!showWalletDetail || !jobResults?.results) return null
    
    const walletResult = jobResults.results[showWalletDetail]
    if (!walletResult || walletResult.status !== 'success' || !walletResult.pnl_report) return null
    
    const report = walletResult.pnl_report
    
    // Transform batch job data to match expected format
    return {
      wallet_address: showWalletDetail,
      portfolio_result: {
        total_pnl_usd: report.summary.total_pnl_usd || '0',
        realized_pnl_usd: report.summary.realized_pnl_usd || '0',
        unrealized_pnl_usd: report.summary.unrealized_pnl_usd || '0',
        roi_percentage: report.summary.roi_percentage || '0',
        total_trades: report.summary.total_trades || 0,
        overall_win_rate_percentage: report.summary.win_rate || '0',
        tokens_analyzed: report.token_breakdown?.length || 0,
        events_processed: report.metadata?.events_processed || 0,
        total_fees_usd: report.summary.total_fees_usd || '0',
        avg_hold_time_minutes: report.summary.avg_hold_time_minutes || '0'
      },
      token_breakdown: report.token_breakdown || [],
      current_holdings: report.current_holdings || [],
      metadata: {
        analyzed_at: report.metadata?.generated_at || new Date().toISOString(),
        data_source: 'BirdEye',
        analysis_duration_ms: Math.round((report.metadata?.analysis_duration_seconds || 0) * 1000),
        quality_score: '85',
        tokens_processed: report.token_breakdown?.length || 0,
        events_processed: report.metadata?.events_processed || 0,
        warnings: report.metadata?.warnings || []
      }
    }
  }, [showWalletDetail, jobResults])
  
  const walletDetailLoading = false // No loading since we use existing data

  // Submit job mutation
  const submitJobMutation = useMutation({
    mutationFn: api.batch.submitJob,
    onSuccess: (data) => {
      toast({
        title: "Job Submitted Successfully",
        description: `Analysis started for ${data.wallet_count || formData.wallet_addresses.length} wallets`,
      })
      setActiveJobs(prev => [...prev, data.job_id])
      setShowSubmissionForm(false)
      resetForm()
      queryClient.invalidateQueries({ queryKey: ['job-history'] })
    },
    onError: (error) => {
      toast({
        title: "Error",
        description: error instanceof Error ? error.message : "Failed to submit batch job",
        variant: "destructive",
      })
    }
  })

  // CSV export mutation
  const exportCSVMutation = useMutation({
    mutationFn: api.batch.exportJobResultsCSV,
    onSuccess: (blob, jobId) => {
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = `batch_results_${jobId}.csv`
      document.body.appendChild(a)
      a.click()
      document.body.removeChild(a)
      URL.revokeObjectURL(url)
      
      toast({
        title: "Export Successful",
        description: "CSV file has been downloaded",
      })
    },
    onError: () => {
      toast({
        title: "Export Failed",
        description: "Failed to export CSV file",
        variant: "destructive",
      })
    }
  })

  // Update active jobs when status changes
  useEffect(() => {
    if (activeJobQueries.data) {
      const completedJobs = activeJobQueries.data
        .filter(job => job && (job.status === 'Completed' || job.status === 'Failed'))
        .map(job => job!.id)
      
      if (completedJobs.length > 0) {
        setActiveJobs(prev => prev.filter(id => !completedJobs.includes(id)))
        completedJobs.forEach(jobId => {
          const job = activeJobQueries.data.find(j => j?.id === jobId)
          if (job) {
            toast({
              title: job.status === 'Completed' ? "Job Completed" : "Job Failed",
              description: `Analysis ${job.status.toLowerCase()} for job ${jobId.slice(0, 8)}...`,
              variant: job.status === 'Completed' ? "default" : "destructive"
            })
          }
        })
        queryClient.invalidateQueries({ queryKey: ['job-history'] })
      }
    }
  }, [activeJobQueries.data, queryClient, toast])

  const resetForm = () => {
    setWalletInput('')
    setCsvFile(null)
    setFormData({
      wallet_addresses: [],
      filters: {
        min_capital_sol: 0.0,
        min_hold_minutes: 0.0,
        min_trades: 0,
        min_win_rate: 0.0,
        max_signatures: 500,
        max_transactions_to_fetch: 1000
      }
    })
  }

  const handleFileUpload = (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0]
    if (file && file.type === 'text/csv') {
      setCsvFile(file)
      const reader = new FileReader()
      reader.onload = (e) => {
        const text = e.target?.result as string
        const addresses = text.split('\n')
          .map(line => line.trim())
          .filter(line => line.length > 0 && !line.startsWith('#'))
          .slice(0, 1000) // Limit to 1000 addresses
        setFormData(prev => ({ ...prev, wallet_addresses: addresses }))
      }
      reader.readAsText(file)
    } else {
      toast({
        title: "Invalid File",
        description: "Please upload a CSV file",
        variant: "destructive",
      })
    }
  }

  const handleWalletInputChange = (value: string) => {
    setWalletInput(value)
    const addresses = value.split('\n')
      .map(line => line.trim())
      .filter(line => line.length > 0)
      .slice(0, 1000) // Limit to 1000 addresses
    setFormData(prev => ({ ...prev, wallet_addresses: addresses }))
  }

  const handleSubmit = () => {
    if (formData.wallet_addresses.length === 0) {
      toast({
        title: "Error",
        description: "Please provide at least one wallet address",
        variant: "destructive",
      })
      return
    }
    
    if (formData.wallet_addresses.length > 1000) {
      toast({
        title: "Error",
        description: "Maximum 1000 wallet addresses allowed per batch",
        variant: "destructive",
      })
      return
    }
    
    submitJobMutation.mutate(formData)
  }

  const handleExportCSV = (jobId: string) => {
    exportCSVMutation.mutate(jobId)
  }

  const getStatusBadge = (status: string) => {
    switch (status) {
      case 'Completed':
        return <Badge className="bg-green-success/20 text-green-success border-green-success/30">Completed</Badge>
      case 'Running':
        return <Badge className="bg-cyan-bright/20 text-cyan-bright border-cyan-bright/30">Running</Badge>
      case 'Pending':
        return <Badge className="bg-orange-warning/20 text-orange-warning border-orange-warning/30">Pending</Badge>
      case 'Failed':
        return <Badge className="bg-red-500/20 text-red-400 border-red-500/30">Failed</Badge>
      default:
        return <Badge className="bg-gray-500/20 text-gray-400 border-gray-500/30">Unknown</Badge>
    }
  }

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'Completed':
        return <CheckCircle className="w-4 h-4 text-green-success" />
      case 'Running':
        return <Clock className="w-4 h-4 text-cyan-bright animate-spin" />
      case 'Pending':
        return <AlertCircle className="w-4 h-4 text-orange-warning" />
      case 'Failed':
        return <AlertCircle className="w-4 h-4 text-red-400" />
      default:
        return <AlertCircle className="w-4 h-4 text-gray-400" />
    }
  }

  const calculateProgress = (job: any): BatchJobProgress | null => {
    if (!job || job.status !== 'Running') return null
    
    // Try to extract progress from job data
    const total = job.wallet_count || 0
    const successful = job.success_count || 0
    const failed = job.failure_count || 0
    const completed = successful + failed
    
    return {
      total_wallets: total,
      completed_wallets: completed,
      successful_wallets: successful,
      failed_wallets: failed,
      progress_percentage: total > 0 ? Math.round((completed / total) * 100) : 0
    }
  }

  const jobHistory = jobHistoryData?.jobs || []
  const jobSummary = jobHistoryData?.summary || {}

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold text-white mb-2">Batch P&L Analysis</h1>
          <p className="text-gray-400">Submit and manage bulk wallet analysis jobs with detailed insights</p>
        </div>
        <Button variant="neon" onClick={() => setShowSubmissionForm(true)} className="space-x-2">
          <Plus className="w-4 h-4" />
          <span>New Batch Job</span>
        </Button>
      </div>

      {/* Job Statistics Dashboard */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <Card className="glass-card border-blue-ice/20">
          <CardHeader className="pb-3">
            <div className="flex items-center justify-between">
              <CardTitle className="text-sm font-medium text-gray-400">Total Jobs</CardTitle>
              <BarChart3 className="w-4 h-4 text-cyan-bright" />
            </div>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-white">{jobSummary.total_jobs || jobHistory.length}</div>
            <p className="text-xs text-gray-400">All time submissions</p>
          </CardContent>
        </Card>
        
        <Card className="glass-card border-blue-ice/20">
          <CardHeader className="pb-3">
            <div className="flex items-center justify-between">
              <CardTitle className="text-sm font-medium text-gray-400">Active Jobs</CardTitle>
              <Clock className="w-4 h-4 text-orange-warning" />
            </div>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-orange-warning">
              {(jobSummary.running_jobs || 0) + (jobSummary.pending_jobs || 0)}
            </div>
            <p className="text-xs text-gray-400">Currently processing</p>
          </CardContent>
        </Card>
        
        <Card className="glass-card border-blue-ice/20">
          <CardHeader className="pb-3">
            <div className="flex items-center justify-between">
              <CardTitle className="text-sm font-medium text-gray-400">Completed</CardTitle>
              <CheckCircle className="w-4 h-4 text-green-success" />
            </div>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-green-success">
              {jobSummary.completed_jobs || jobHistory.filter(job => job.status === 'Completed').length}
            </div>
            <p className="text-xs text-gray-400">Successful analyses</p>
          </CardContent>
        </Card>
        
        <Card className="glass-card border-blue-ice/20">
          <CardHeader className="pb-3">
            <div className="flex items-center justify-between">
              <CardTitle className="text-sm font-medium text-gray-400">Success Rate</CardTitle>
              <TrendingUp className="w-4 h-4 text-green-success" />
            </div>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-green-success">
              {jobHistory.length > 0 ? Math.round((jobHistory.filter(job => job.status === 'Completed').length / jobHistory.length) * 100) : 0}%
            </div>
            <p className="text-xs text-gray-400">Overall success</p>
          </CardContent>
        </Card>
      </div>

      {/* Active Jobs Section */}
      {activeJobs.length > 0 && (
        <Card className="glass-card border-blue-ice/20">
          <CardHeader>
            <CardTitle className="text-white flex items-center">
              <Clock className="w-5 h-5 mr-2 text-cyan-bright animate-spin" />
              Active Jobs
            </CardTitle>
            <CardDescription className="text-gray-400">
              Jobs currently being processed in real-time
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="space-y-4">
              {activeJobQueries.data?.map((job) => {
                if (!job) return null
                const progress = calculateProgress(job)
                
                return (
                  <div key={job.id} className="p-4 bg-navy-deep/50 rounded-lg border border-cyan-bright/20">
                    <div className="flex items-center justify-between mb-3">
                      <div className="flex items-center space-x-3">
                        {getStatusIcon(job.status)}
                        <div>
                          <h3 className="font-medium text-white font-mono text-sm">
                            {job.id.slice(0, 16)}...
                          </h3>
                          <div className="flex items-center space-x-4 text-sm text-gray-400">
                            <span className="flex items-center">
                              <Users className="w-3 h-3 mr-1" />
                              {job.wallet_count} wallets
                            </span>
                            <span className="flex items-center">
                              <Calendar className="w-3 h-3 mr-1" />
                              {new Date(job.created_at).toLocaleString()}
                            </span>
                          </div>
                        </div>
                      </div>
                      <div className="flex items-center space-x-2">
                        {getStatusBadge(job.status)}
                      </div>
                    </div>
                    
                    {progress && (
                      <div className="space-y-2">
                        <Progress value={progress.progress_percentage} className="h-2" />
                        <div className="flex items-center justify-between text-sm">
                          <span className="text-gray-400">
                            {progress.progress_percentage}% complete • 
                            {progress.completed_wallets} of {progress.total_wallets} analyzed
                          </span>
                          <div className="flex items-center space-x-4 text-xs">
                            <span className="text-green-success">
                              ✓ {progress.successful_wallets}
                            </span>
                            <span className="text-red-400">
                              ✗ {progress.failed_wallets}
                            </span>
                          </div>
                        </div>
                      </div>
                    )}
                  </div>
                )
              })}
            </div>
          </CardContent>
        </Card>
      )}

      {/* Job History */}
      <Card className="glass-card border-blue-ice/20">
        <CardHeader>
          <CardTitle className="text-white">Job History</CardTitle>
          <CardDescription className="text-gray-400">
            Complete history of all batch analysis jobs
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead>
                <tr className="border-b border-blue-ice/20">
                  <th className="text-left py-3 px-4 font-medium text-gray-400">Job ID</th>
                  <th className="text-left py-3 px-4 font-medium text-gray-400">Status</th>
                  <th className="text-left py-3 px-4 font-medium text-gray-400">Wallets</th>
                  <th className="text-left py-3 px-4 font-medium text-gray-400">Success/Failed</th>
                  <th className="text-left py-3 px-4 font-medium text-gray-400">Created</th>
                  <th className="text-left py-3 px-4 font-medium text-gray-400">Duration</th>
                  <th className="text-left py-3 px-4 font-medium text-gray-400">Actions</th>
                </tr>
              </thead>
              <tbody>
                {historyLoading ? (
                  Array.from({ length: 8 }).map((_, i) => (
                    <tr key={i} className="border-b border-blue-ice/10">
                      <td className="py-3 px-4"><Skeleton className="h-4 w-32" /></td>
                      <td className="py-3 px-4"><Skeleton className="h-6 w-20" /></td>
                      <td className="py-3 px-4"><Skeleton className="h-4 w-16" /></td>
                      <td className="py-3 px-4"><Skeleton className="h-4 w-20" /></td>
                      <td className="py-3 px-4"><Skeleton className="h-4 w-28" /></td>
                      <td className="py-3 px-4"><Skeleton className="h-4 w-24" /></td>
                      <td className="py-3 px-4"><Skeleton className="h-8 w-24" /></td>
                    </tr>
                  ))
                ) : (
                  jobHistory.map((job) => {
                    const duration = job.completed_at && job.started_at ? 
                      Math.round((new Date(job.completed_at).getTime() - new Date(job.started_at).getTime()) / 1000 / 60) : null
                    
                    return (
                      <tr key={job.id} className="border-b border-blue-ice/10 hover:bg-navy-deep/30 transition-colors">
                        <td className="py-3 px-4">
                          <div className="font-mono text-sm text-white">
                            {job.id.slice(0, 16)}...
                          </div>
                        </td>
                        <td className="py-3 px-4">
                          {getStatusBadge(job.status)}
                        </td>
                        <td className="py-3 px-4">
                          <div className="text-white">{job.wallet_count}</div>
                        </td>
                        <td className="py-3 px-4">
                          {job.status === 'Completed' && (
                            <div className="flex items-center space-x-2 text-sm">
                              <span className="text-green-success">✓ {job.success_count || 0}</span>
                              <span className="text-red-400">✗ {job.failure_count || 0}</span>
                            </div>
                          )}
                        </td>
                        <td className="py-3 px-4">
                          <div className="text-sm text-gray-400">
                            {new Date(job.created_at).toLocaleDateString()}
                          </div>
                        </td>
                        <td className="py-3 px-4">
                          <div className="text-sm text-gray-400">
                            {duration ? `${duration}m` : '-'}
                          </div>
                        </td>
                        <td className="py-3 px-4">
                          <div className="flex items-center space-x-2">
                            {job.status === 'Completed' && (
                              <>
                                <Button 
                                  variant="ghost" 
                                  size="sm" 
                                  onClick={() => setShowResults(job.id)}
                                  className="text-cyan-bright hover:bg-cyan-bright/20"
                                >
                                  <Eye className="w-4 h-4 mr-1" />
                                  Results
                                </Button>
                                <Button 
                                  variant="ghost" 
                                  size="sm" 
                                  onClick={() => handleExportCSV(job.id)}
                                  disabled={exportCSVMutation.isLoading}
                                  className="text-green-success hover:bg-green-success/20"
                                >
                                  <Download className="w-4 h-4 mr-1" />
                                  CSV
                                </Button>
                              </>
                            )}
                          </div>
                        </td>
                      </tr>
                    )
                  })
                )}
              </tbody>
            </table>
          </div>
        </CardContent>
      </Card>

      {/* Job Submission Form Dialog */}
      <Dialog open={showSubmissionForm} onOpenChange={setShowSubmissionForm}>
        <DialogContent className="glass-card border-blue-ice/20 max-w-3xl max-h-[90vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle className="text-white flex items-center">
              <Plus className="w-5 h-5 mr-2 text-cyan-bright" />
              New Batch P&L Analysis Job
            </DialogTitle>
            <DialogDescription className="text-gray-400">
              Submit a comprehensive P&L analysis for up to 1,000 wallet addresses
            </DialogDescription>
          </DialogHeader>
          
          <div className="space-y-6 py-4">
            {/* Wallet Input Section */}
            <div className="space-y-4">
              <h3 className="text-lg font-semibold text-white flex items-center">
                <Wallet className="w-5 h-5 mr-2" />
                Wallet Addresses
              </h3>
              
              <div className="space-y-3">
                <Label htmlFor="wallet-textarea" className="text-white">
                  Paste wallet addresses (one per line, max 1,000)
                </Label>
                <textarea
                  id="wallet-textarea"
                  value={walletInput}
                  onChange={(e) => handleWalletInputChange(e.target.value)}
                  placeholder="5ngDQ3vMT7GHeFvXP5GLX79t6fqG2t15kyQTtst9zFdw&#10;Another_wallet_address_here&#10;..."
                  className="w-full h-32 p-3 bg-navy-deep/50 border border-blue-ice/20 rounded-lg text-white placeholder-gray-400 resize-none focus:outline-none focus:ring-2 focus:ring-cyan-bright focus:border-transparent"
                />
              </div>

              <div className="flex items-center space-x-4">
                <div className="flex-1 h-px bg-blue-ice/20" />
                <span className="text-gray-400 text-sm">OR</span>
                <div className="flex-1 h-px bg-blue-ice/20" />
              </div>

              <div className="space-y-3">
                <Label htmlFor="csv-upload" className="text-white">
                  Upload CSV file with wallet addresses
                </Label>
                <div className="flex items-center space-x-3">
                  <input
                    id="csv-upload"
                    type="file"
                    accept=".csv,.txt"
                    onChange={handleFileUpload}
                    className="hidden"
                  />
                  <Button
                    variant="outline"
                    onClick={() => document.getElementById('csv-upload')?.click()}
                    className="border-blue-steel/50 text-blue-steel hover:bg-blue-steel/20"
                  >
                    <Upload className="w-4 h-4 mr-2" />
                    Choose CSV File
                  </Button>
                  {csvFile && (
                    <div className="flex items-center space-x-2 text-sm text-gray-400">
                      <FileText className="w-4 h-4" />
                      <span>{csvFile.name}</span>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => {
                          setCsvFile(null)
                          setFormData(prev => ({ 
                            ...prev, 
                            wallet_addresses: [],
                            filters: {
                              min_capital_sol: 0.0,
                              min_hold_minutes: 0.0,
                              min_trades: 0,
                              min_win_rate: 0.0,
                              max_signatures: 500,
                              max_transactions_to_fetch: 1000
                            }
                          }))
                          setWalletInput('')
                        }}
                      >
                        <X className="w-3 h-3" />
                      </Button>
                    </div>
                  )}
                </div>
              </div>

              <div className="p-3 bg-navy-deep/30 rounded-lg border border-blue-ice/10">
                <div className="flex items-center justify-between text-sm">
                  <span className="text-gray-400">Addresses loaded:</span>
                  <span className="text-white font-medium">{formData.wallet_addresses.length}</span>
                </div>
                {formData.wallet_addresses.length > 0 && (
                  <div className="mt-2 text-xs text-gray-500">
                    Estimated analysis time: ~{Math.ceil(formData.wallet_addresses.length / 10)} minutes
                  </div>
                )}
              </div>
            </div>

            {/* Advanced Filters */}
            <div className="space-y-4">
              <h3 className="text-lg font-semibold text-white flex items-center">
                <Filter className="w-5 h-5 mr-2" />
                Analysis Filters (Optional)
              </h3>
              
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div className="space-y-2">
                  <Label htmlFor="min-capital" className="text-white">
                    Min Capital (SOL)
                  </Label>
                  <Input
                    id="min-capital"
                    type="number"
                    step="0.1"
                    placeholder="1.0"
                    value={formData.filters?.min_capital_sol || ''}
                    onChange={(e) => setFormData(prev => ({
                      ...prev,
                      filters: { ...prev.filters, min_capital_sol: parseFloat(e.target.value) || undefined }
                    }))}
                    className="bg-navy-deep/50 border-blue-ice/20 text-white"
                  />
                </div>
                
                <div className="space-y-2">
                  <Label htmlFor="min-trades" className="text-white">
                    Min Trades
                  </Label>
                  <Input
                    id="min-trades"
                    type="number"
                    placeholder="5"
                    value={formData.filters?.min_trades || ''}
                    onChange={(e) => setFormData(prev => ({
                      ...prev,
                      filters: { ...prev.filters, min_trades: parseInt(e.target.value) || undefined }
                    }))}
                    className="bg-navy-deep/50 border-blue-ice/20 text-white"
                  />
                </div>
                
                <div className="space-y-2">
                  <Label htmlFor="min-win-rate" className="text-white">
                    Min Win Rate (%)
                  </Label>
                  <Input
                    id="min-win-rate"
                    type="number"
                    placeholder="50"
                    min="0"
                    max="100"
                    value={formData.filters?.min_win_rate || ''}
                    onChange={(e) => setFormData(prev => ({
                      ...prev,
                      filters: { ...prev.filters, min_win_rate: parseFloat(e.target.value) || undefined }
                    }))}
                    className="bg-navy-deep/50 border-blue-ice/20 text-white"
                  />
                </div>
                
                <div className="space-y-2">
                  <Label htmlFor="max-transactions" className="text-white">
                    Max Transactions
                  </Label>
                  <Input
                    id="max-transactions"
                    type="number"
                    placeholder="1000"
                    value={formData.filters?.max_transactions_to_fetch || ''}
                    onChange={(e) => setFormData(prev => ({
                      ...prev,
                      filters: { ...prev.filters, max_transactions_to_fetch: parseInt(e.target.value) || undefined }
                    }))}
                    className="bg-navy-deep/50 border-blue-ice/20 text-white"
                  />
                </div>
              </div>
            </div>
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setShowSubmissionForm(false)}>
              Cancel
            </Button>
            <Button 
              variant="neon" 
              onClick={handleSubmit}
              disabled={submitJobMutation.isLoading || formData.wallet_addresses.length === 0}
            >
              {submitJobMutation.isLoading ? (
                <>
                  <Clock className="w-4 h-4 mr-2 animate-spin" />
                  Submitting...
                </>
              ) : (
                <>
                  <Play className="w-4 h-4 mr-2" />
                  Run Analysis ({formData.wallet_addresses.length} wallets)
                </>
              )}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Job Results Dialog */}
      <Dialog open={!!showResults} onOpenChange={() => setShowResults(null)}>
        <DialogContent className="glass-card border-blue-ice/20 max-w-7xl max-h-[90vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle className="text-white flex items-center">
              <BarChart3 className="w-5 h-5 mr-2 text-cyan-bright" />
              Batch Job Results
            </DialogTitle>
            <DialogDescription className="text-gray-400">
              Comprehensive analysis results for job {showResults?.slice(0, 16)}...
            </DialogDescription>
          </DialogHeader>
          
          {resultsLoading ? (
            <div className="space-y-4 py-4">
              <div className="grid grid-cols-4 gap-4">
                {Array.from({ length: 4 }).map((_, i) => (
                  <Skeleton key={i} className="h-24 w-full" />
                ))}
              </div>
              <Skeleton className="h-64 w-full" />
            </div>
          ) : jobResults && (
            <div className="space-y-6 py-4">
              {/* Summary Statistics */}
              <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
                <Card className="glass-card border-blue-ice/20">
                  <CardContent className="pt-4">
                    <div className="text-2xl font-bold text-white">
                      {jobResults.summary?.total_wallets || Object.keys(jobResults.results || {}).length}
                    </div>
                    <p className="text-sm text-gray-400">Total Wallets</p>
                  </CardContent>
                </Card>
                <Card className="glass-card border-blue-ice/20">
                  <CardContent className="pt-4">
                    <div className="text-2xl font-bold text-green-success">
                      {jobResults.summary?.successful_analyses || 0}
                    </div>
                    <p className="text-sm text-gray-400">Successful</p>
                  </CardContent>
                </Card>
                <Card className="glass-card border-blue-ice/20">
                  <CardContent className="pt-4">
                    <div className="text-2xl font-bold text-cyan-bright">
                      {jobResults.summary?.total_pnl_usd ? formatCurrency(parseFloat(jobResults.summary.total_pnl_usd)) : '$0'}
                    </div>
                    <p className="text-sm text-gray-400">Total P&L</p>
                  </CardContent>
                </Card>
                <Card className="glass-card border-blue-ice/20">
                  <CardContent className="pt-4">
                    <div className="text-2xl font-bold text-white">
                      {jobResults.summary?.average_pnl_usd ? formatCurrency(parseFloat(jobResults.summary.average_pnl_usd)) : '$0'}
                    </div>
                    <p className="text-sm text-gray-400">Average P&L</p>
                  </CardContent>
                </Card>
              </div>

              {/* Results Table */}
              <Card className="glass-card border-blue-ice/20">
                <CardHeader>
                  <div className="flex items-center justify-between">
                    <CardTitle className="text-white">Wallet Analysis Results</CardTitle>
                    <div className="flex items-center space-x-2">
                      <Button 
                        variant="ghost" 
                        size="sm"
                        onClick={() => handleExportCSV(showResults!)}
                        disabled={exportCSVMutation.isLoading}
                        className="text-green-success hover:bg-green-success/20"
                      >
                        <Download className="w-4 h-4 mr-2" />
                        Export CSV
                      </Button>
                    </div>
                  </div>
                </CardHeader>
                <CardContent>
                  <div className="overflow-x-auto max-h-96">
                    <table className="w-full">
                      <thead className="sticky top-0 bg-charcoal">
                        <tr className="border-b border-blue-ice/20">
                          <th className="text-left py-3 px-4 font-medium text-gray-400">Wallet Address</th>
                          <th className="text-left py-3 px-4 font-medium text-gray-400">Status</th>
                          <th className="text-left py-3 px-4 font-medium text-gray-400">Total P&L</th>
                          <th className="text-left py-3 px-4 font-medium text-gray-400">Win Rate</th>
                          <th className="text-left py-3 px-4 font-medium text-gray-400">Trades</th>
                          <th className="text-left py-3 px-4 font-medium text-gray-400">Actions</th>
                        </tr>
                      </thead>
                      <tbody>
                        {Object.entries(jobResults.results || {}).map(([walletAddress, result]) => (
                          <tr key={walletAddress} className="border-b border-blue-ice/10 hover:bg-navy-deep/30 transition-colors">
                            <td className="py-3 px-4">
                              <span className="font-mono text-sm text-white">
                                {truncateAddress(walletAddress)}
                              </span>
                            </td>
                            <td className="py-3 px-4">
                              {result.status === 'success' ? (
                                <Badge className="bg-green-success/20 text-green-success border-green-success/30">
                                  Success
                                </Badge>
                              ) : (
                                <Badge className="bg-red-500/20 text-red-400 border-red-500/30">
                                  Failed
                                </Badge>
                              )}
                            </td>
                            <td className="py-3 px-4">
                              {result.status === 'success' && result.pnl_report ? (
                                <span className={parseFloat(result.pnl_report.summary?.total_pnl_usd || '0') >= 0 ? 'text-green-success' : 'text-red-400'}>
                                  {formatCurrency(parseFloat(result.pnl_report.summary?.total_pnl_usd || '0'))}
                                </span>
                              ) : (
                                <span className="text-gray-500">-</span>
                              )}
                            </td>
                            <td className="py-3 px-4">
                              {result.status === 'success' && result.pnl_report ? (
                                <span className="text-white">
                                  {formatPercentage(parseFloat(result.pnl_report.summary?.win_rate || '0'))}
                                </span>
                              ) : (
                                <span className="text-gray-500">-</span>
                              )}
                            </td>
                            <td className="py-3 px-4">
                              {result.status === 'success' && result.pnl_report ? (
                                <span className="text-white">{result.pnl_report.summary?.total_trades || 0}</span>
                              ) : (
                                <span className="text-gray-500">-</span>
                              )}
                            </td>
                            <td className="py-3 px-4">
                              {result.status === 'success' && (
                                <Button 
                                  variant="ghost" 
                                  size="sm" 
                                  onClick={() => setShowWalletDetail(walletAddress)}
                                  className="text-cyan-bright hover:bg-cyan-bright/20"
                                >
                                  <Eye className="w-4 h-4 mr-1" />
                                  Details
                                </Button>
                              )}
                              {result.status === 'failed' && result.error_message && (
                                <span className="text-xs text-red-400" title={result.error_message}>
                                  Error
                                </span>
                              )}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </CardContent>
              </Card>
            </div>
          )}

          <DialogFooter>
            <Button variant="outline" onClick={() => setShowResults(null)}>
              Close
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Wallet Detail Dialog (V2 Analysis) */}
      <Dialog open={!!showWalletDetail} onOpenChange={() => setShowWalletDetail(null)}>
        <DialogContent className="glass-card border-blue-ice/20 max-w-6xl max-h-[90vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle className="text-white flex items-center">
              <Target className="w-5 h-5 mr-2 text-cyan-bright" />
              Detailed Wallet Analysis
            </DialogTitle>
            <DialogDescription className="text-gray-400 font-mono">
              Enhanced analysis for {truncateAddress(showWalletDetail || '')}
            </DialogDescription>
          </DialogHeader>
          
          {walletDetailLoading ? (
            <div className="space-y-4 py-4">
              <div className="grid grid-cols-3 gap-4">
                {Array.from({ length: 3 }).map((_, i) => (
                  <Skeleton key={i} className="h-32 w-full" />
                ))}
              </div>
              <Skeleton className="h-64 w-full" />
            </div>
          ) : walletDetail && (
            <div className="space-y-6 py-4">
              {/* Portfolio Overview */}
              <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
                <Card className="glass-card border-blue-ice/20">
                  <CardContent className="pt-4">
                    <div className={`text-2xl font-bold mb-1 ${
                      parseFloat(walletDetail.portfolio_result?.total_pnl_usd || '0') >= 0 
                        ? 'text-green-success' 
                        : 'text-red-400'
                    }`}>
                      {formatCurrency(parseFloat(walletDetail.portfolio_result?.total_pnl_usd || '0'))}
                    </div>
                    <p className="text-sm text-gray-400">Total P&L</p>
                    <div className="text-xs text-gray-500 mt-1">
                      ROI: {formatPercentage(parseFloat(walletDetail.portfolio_result?.roi_percentage || '0'))}
                    </div>
                  </CardContent>
                </Card>
                
                <Card className="glass-card border-blue-ice/20">
                  <CardContent className="pt-4">
                    <div className="text-2xl font-bold text-cyan-bright mb-1">
                      {formatCurrency(parseFloat(walletDetail.portfolio_result?.realized_pnl_usd || '0'))}
                    </div>
                    <p className="text-sm text-gray-400">Realized P&L</p>
                    <div className="text-xs text-gray-500 mt-1">
                      Closed positions
                    </div>
                  </CardContent>
                </Card>
                
                <Card className="glass-card border-blue-ice/20">
                  <CardContent className="pt-4">
                    <div className={`text-2xl font-bold mb-1 ${
                      parseFloat(walletDetail.portfolio_result?.unrealized_pnl_usd || '0') >= 0 
                        ? 'text-green-success' 
                        : 'text-orange-warning'
                    }`}>
                      {formatCurrency(parseFloat(walletDetail.portfolio_result?.unrealized_pnl_usd || '0'))}
                    </div>
                    <p className="text-sm text-gray-400">Unrealized P&L</p>
                    <div className="text-xs text-gray-500 mt-1">
                      Current holdings
                    </div>
                  </CardContent>
                </Card>
                
                <Card className="glass-card border-blue-ice/20">
                  <CardContent className="pt-4">
                    <div className="text-2xl font-bold text-white mb-1">
                      {walletDetail.portfolio_result?.total_trades || 0}
                    </div>
                    <p className="text-sm text-gray-400">Total Trades</p>
                    <div className="text-xs text-cyan-bright mt-1">
                      Win Rate: {formatPercentage(parseFloat(walletDetail.portfolio_result?.overall_win_rate_percentage || '0'))}
                    </div>
                  </CardContent>
                </Card>
              </div>

              {/* Trading Statistics */}
              <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                <Card className="glass-card border-blue-ice/20">
                  <CardContent className="pt-4">
                    <div className="text-xl font-bold text-white mb-1">
                      {walletDetail.portfolio_result?.tokens_analyzed || 0}
                    </div>
                    <p className="text-sm text-gray-400">Tokens Analyzed</p>
                    <div className="text-xs text-blue-steel mt-1">
                      Events: {walletDetail.portfolio_result?.events_processed || 0}
                    </div>
                  </CardContent>
                </Card>
                
                <Card className="glass-card border-blue-ice/20">
                  <CardContent className="pt-4">
                    <div className="text-xl font-bold text-white mb-1">
                      {formatCurrency(parseFloat(walletDetail.portfolio_result?.total_fees_usd || '0'))}
                    </div>
                    <p className="text-sm text-gray-400">Total Fees Paid</p>
                    <div className="text-xs text-red-400 mt-1">
                      Transaction costs
                    </div>
                  </CardContent>
                </Card>
                
                <Card className="glass-card border-blue-ice/20">
                  <CardContent className="pt-4">
                    <div className="text-xl font-bold text-white mb-1">
                      {Math.round(parseFloat(walletDetail.portfolio_result?.avg_hold_time_minutes || '0') / 60)}h
                    </div>
                    <p className="text-sm text-gray-400">Avg Hold Time</p>
                    <div className="text-xs text-purple-400 mt-1">
                      Per position
                    </div>
                  </CardContent>
                </Card>
              </div>

              {/* Token Breakdown */}
              {walletDetail.token_breakdown && walletDetail.token_breakdown.length > 0 && (
                <Card className="glass-card border-blue-ice/20">
                  <CardHeader>
                    <CardTitle className="text-white">Token Performance Breakdown</CardTitle>
                  </CardHeader>
                  <CardContent>
                    <div className="overflow-x-auto">
                      <table className="w-full">
                        <thead>
                          <tr className="border-b border-blue-ice/20">
                            <th className="text-left py-2 px-3 font-medium text-gray-400">Token</th>
                            <th className="text-left py-2 px-3 font-medium text-gray-400">P&L</th>
                            <th className="text-left py-2 px-3 font-medium text-gray-400">Realized</th>
                            <th className="text-left py-2 px-3 font-medium text-gray-400">Unrealized</th>
                            <th className="text-left py-2 px-3 font-medium text-gray-400">Trades</th>
                            <th className="text-left py-2 px-3 font-medium text-gray-400">Avg Price</th>
                          </tr>
                        </thead>
                        <tbody>
                          {walletDetail.token_breakdown.map((token: any, index: number) => (
                            <tr key={index} className="border-b border-blue-ice/10 hover:bg-navy-deep/30 transition-colors">
                              <td className="py-2 px-3">
                                <div>
                                  <div className="text-white font-medium">{token.token_symbol || 'Unknown'}</div>
                                  <div className="text-xs text-gray-400 font-mono">{truncateAddress(token.token_mint)}</div>
                                </div>
                              </td>
                              <td className="py-2 px-3">
                                <span className={parseFloat(token.total_pnl_usd || '0') >= 0 ? 'text-green-success' : 'text-red-400'}>
                                  {formatCurrency(parseFloat(token.total_pnl_usd || '0'))}
                                </span>
                              </td>
                              <td className="py-2 px-3 text-cyan-bright">
                                {formatCurrency(parseFloat(token.realized_pnl_usd || '0'))}
                              </td>
                              <td className="py-2 px-3 text-orange-warning">
                                {formatCurrency(parseFloat(token.unrealized_pnl_usd || '0'))}
                              </td>
                              <td className="py-2 px-3 text-white">
                                {(token.buy_count || 0) + (token.sell_count || 0)}
                              </td>
                              <td className="py-2 px-3 text-gray-400">
                                {token.avg_buy_price_usd ? formatCurrency(parseFloat(token.avg_buy_price_usd)) : '-'}
                              </td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  </CardContent>
                </Card>
              )}

              {/* Current Holdings */}
              {walletDetail.current_holdings && walletDetail.current_holdings.length > 0 && (
                <Card className="glass-card border-blue-ice/20">
                  <CardHeader>
                    <CardTitle className="text-white">Current Holdings</CardTitle>
                  </CardHeader>
                  <CardContent>
                    <div className="space-y-4">
                      {walletDetail.current_holdings.map((holding: any, index: number) => (
                        <div key={index} className="p-4 bg-navy-deep/50 rounded-lg border border-blue-ice/10">
                          <div className="flex items-center justify-between mb-3">
                            <div>
                              <h4 className="text-white font-medium">{holding.token_symbol || 'Unknown Token'}</h4>
                              <p className="text-xs text-gray-400 font-mono">{truncateAddress(holding.token_mint)}</p>
                            </div>
                            <div className="text-right">
                              <div className={`text-lg font-bold ${
                                parseFloat(holding.unrealized_pnl_usd || '0') >= 0 ? 'text-green-success' : 'text-red-400'
                              }`}>
                                {formatCurrency(parseFloat(holding.unrealized_pnl_usd || '0'))}
                              </div>
                              <div className="text-xs text-gray-400">Unrealized P&L</div>
                            </div>
                          </div>
                          
                          <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
                            <div>
                              <p className="text-gray-400">Amount</p>
                              <p className="text-white font-medium">{parseFloat(holding.amount || '0').toLocaleString()}</p>
                            </div>
                            <div>
                              <p className="text-gray-400">Cost Basis</p>
                              <p className="text-cyan-bright">{formatCurrency(parseFloat(holding.total_cost_basis_usd || '0'))}</p>
                            </div>
                            <div>
                              <p className="text-gray-400">Current Value</p>
                              <p className="text-white">{formatCurrency(parseFloat(holding.current_value_usd || '0'))}</p>
                            </div>
                            <div>
                              <p className="text-gray-400">Avg Cost</p>
                              <p className="text-gray-300">{formatCurrency(parseFloat(holding.avg_cost_basis_usd || '0'))}</p>
                            </div>
                          </div>
                        </div>
                      ))}
                    </div>
                  </CardContent>
                </Card>
              )}

              {/* Analysis Metadata */}
              <Card className="glass-card border-blue-ice/20">
                <CardHeader>
                  <CardTitle className="text-white">Analysis Metadata</CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
                    <div>
                      <p className="text-gray-400">Analyzed At</p>
                      <p className="text-white">{new Date(walletDetail.metadata?.analyzed_at || '').toLocaleString()}</p>
                    </div>
                    <div>
                      <p className="text-gray-400">Data Source</p>
                      <p className="text-white">{walletDetail.metadata?.data_source || 'BirdEye'}</p>
                    </div>
                    <div>
                      <p className="text-gray-400">Analysis Duration</p>
                      <p className="text-white">{walletDetail.metadata?.analysis_duration_ms || 0}ms</p>
                    </div>
                    <div>
                      <p className="text-gray-400">Quality Score</p>
                      <p className="text-white">{walletDetail.metadata?.quality_score || 85}/100</p>
                    </div>
                  </div>
                  
                  {/* Analysis Warnings */}
                  {walletDetail.metadata?.warnings && walletDetail.metadata.warnings.length > 0 && (
                    <div className="mt-4 p-3 bg-orange-warning/10 border border-orange-warning/20 rounded-lg">
                      <h4 className="text-orange-warning font-medium mb-2">Analysis Notes</h4>
                      <ul className="space-y-1 text-sm">
                        {walletDetail.metadata.warnings.map((warning: string, index: number) => (
                          <li key={index} className="text-gray-300 flex items-start">
                            <span className="text-orange-warning mr-2">•</span>
                            {warning}
                          </li>
                        ))}
                      </ul>
                    </div>
                  )}
                </CardContent>
              </Card>
            </div>
          )}

          <DialogFooter>
            <Button variant="outline" onClick={() => setShowWalletDetail(null)}>
              Close
            </Button>
            {showWalletDetail && (
              <Button 
                variant="neon"
                onClick={() => {
                  window.open(`https://solscan.io/account/${showWalletDetail}`, '_blank')
                }}
              >
                <ExternalLink className="w-4 h-4 mr-2" />
                View on Solscan
              </Button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}