'use client'

import { useQuery } from '@tanstack/react-query'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { MetricsChart } from '@/components/charts/MetricsChart'
import { LoadingSpinner, SystemHealthIndicator } from '@/components/ui/loading-spinner'
import { api } from '@/lib/api'
import { formatCurrency, formatNumber } from '@/lib/utils'
import { Activity, DollarSign, TrendingUp, Users, Server, Zap } from 'lucide-react'
import { motion } from 'framer-motion'

export default function Dashboard() {
  const { data: metrics, isLoading: metricsLoading } = useQuery({
    queryKey: ['dashboard-metrics'],
    queryFn: api.dashboard.getMetrics
  })

  const { data: chartData, isLoading: chartLoading } = useQuery({
    queryKey: ['dashboard-charts'],
    queryFn: api.dashboard.getChartData
  })

  const metricCards = [
    {
      title: 'Total Wallets',
      value: metrics?.totalWallets || 0,
      change: '+12.5%',
      icon: Users,
      color: 'text-cyan-bright',
      bgGradient: 'from-cyan-bright/20 to-cyan-bright/5'
    },
    {
      title: 'Volume (24h)',
      value: formatCurrency(metrics?.totalVolume || 0),
      change: '+8.2%',
      icon: DollarSign,
      color: 'text-green-success',
      bgGradient: 'from-green-success/20 to-green-success/5'
    },
    {
      title: 'Success Rate',
      value: `${metrics?.successRate || 0}%`,
      change: '+2.1%',
      icon: TrendingUp,
      color: 'text-blue-steel',
      bgGradient: 'from-blue-steel/20 to-blue-steel/5'
    },
    {
      title: 'Total P&L',
      value: formatCurrency(metrics?.profitLoss || 0),
      change: '+15.3%',
      icon: Activity,
      color: 'text-orange-warning',
      bgGradient: 'from-orange-warning/20 to-orange-warning/5'
    },
    {
      title: 'Active Services',
      value: metrics?.activeServices || 0,
      change: '100%',
      icon: Server,
      color: 'text-blue-soft',
      bgGradient: 'from-blue-soft/20 to-blue-soft/5'
    },
    {
      title: 'System Health',
      value: `${metrics?.systemHealth || 0}%`,
      change: '+0.5%',
      icon: Zap,
      color: 'text-green-success',
      bgGradient: 'from-green-success/20 to-green-success/5'
    }
  ]

  const containerVariants = {
    hidden: { opacity: 0 },
    visible: {
      opacity: 1,
      transition: {
        staggerChildren: 0.1
      }
    }
  }

  const cardVariants = {
    hidden: { opacity: 0, y: 20 },
    visible: {
      opacity: 1,
      y: 0,
      transition: {
        duration: 0.5,
        ease: "easeOut"
      }
    }
  }

  return (
    <div className="space-y-6">
      <motion.div
        initial={{ opacity: 0, y: -20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.6 }}
        className="flex flex-col md:flex-row md:items-center justify-between"
      >
        <div>
          <h1 className="text-3xl font-bold text-white mb-2">Dashboard</h1>
          <p className="text-gray-400">Overview of your wallet analysis system</p>
        </div>
        <SystemHealthIndicator />
      </motion.div>

      {/* Metrics Grid */}
      <motion.div 
        className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-6 gap-4"
        variants={containerVariants}
        initial="hidden"
        animate="visible"
      >
        {metricCards.map((metric, index) => {
          const Icon = metric.icon
          return (
            <motion.div
              key={index}
              variants={cardVariants}
              whileHover={{ 
                scale: 1.05,
                transition: { duration: 0.2 }
              }}
              whileTap={{ scale: 0.95 }}
            >
              <Card className={`glass-card border-blue-ice/20 hover:border-cyan-bright/50 transition-all duration-300 bg-gradient-to-br ${metric.bgGradient} relative overflow-hidden`}>
                <div className="absolute inset-0 bg-gradient-to-br from-white/5 to-transparent opacity-0 hover:opacity-100 transition-opacity duration-300" />
                <CardHeader className="pb-3 relative z-10">
                  <div className="flex items-center justify-between">
                    <CardTitle className="text-sm font-medium text-gray-400">
                      {metric.title}
                    </CardTitle>
                    <motion.div
                      whileHover={{ rotate: 360 }}
                      transition={{ duration: 0.5 }}
                    >
                      <Icon className={`w-4 h-4 ${metric.color}`} />
                    </motion.div>
                  </div>
                </CardHeader>
                <CardContent className="relative z-10">
                  {metricsLoading ? (
                    <LoadingSpinner size="sm" className="mb-2" />
                  ) : (
                    <motion.div 
                      className="text-2xl font-bold text-white mb-1"
                      initial={{ scale: 0 }}
                      animate={{ scale: 1 }}
                      transition={{ duration: 0.5, delay: index * 0.1 }}
                    >
                      {typeof metric.value === 'string' ? metric.value : formatNumber(metric.value)}
                    </motion.div>
                  )}
                  <p className="text-xs text-green-success">
                    {metric.change} from last period
                  </p>
                </CardContent>
              </Card>
            </motion.div>
          )
        })}
      </motion.div>

      {/* Charts */}
      <div className="grid grid-cols-1 xl:grid-cols-2 gap-6">
        <motion.div
          initial={{ opacity: 0, x: -50 }}
          animate={{ opacity: 1, x: 0 }}
          transition={{ duration: 0.6, delay: 0.3 }}
        >
          <Card className="glass-card border-blue-ice/20 hover:border-cyan-bright/30 transition-all duration-300">
            <CardHeader>
              <CardTitle className="text-white">Volume & Profit Trends</CardTitle>
              <CardDescription className="text-gray-400">
                30-day performance overview
              </CardDescription>
            </CardHeader>
            <CardContent>
              {chartLoading ? (
                <div className="h-80 flex items-center justify-center">
                  <LoadingSpinner size="lg" />
                </div>
              ) : (
                <MetricsChart data={chartData || []} />
              )}
            </CardContent>
          </Card>
        </motion.div>

        <motion.div
          initial={{ opacity: 0, x: 50 }}
          animate={{ opacity: 1, x: 0 }}
          transition={{ duration: 0.6, delay: 0.4 }}
        >
          <Card className="glass-card border-blue-ice/20 hover:border-cyan-bright/30 transition-all duration-300">
            <CardHeader>
              <CardTitle className="text-white">Recent Activity</CardTitle>
              <CardDescription className="text-gray-400">
                Latest system events
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="space-y-4">
                {Array.from({ length: 5 }).map((_, i) => (
                  <motion.div 
                    key={i} 
                    className="flex items-center space-x-3 p-3 bg-navy-deep/50 rounded-lg hover:bg-navy-deep/70 transition-colors duration-200"
                    initial={{ opacity: 0, x: -20 }}
                    animate={{ opacity: 1, x: 0 }}
                    transition={{ duration: 0.4, delay: 0.5 + i * 0.1 }}
                    whileHover={{ scale: 1.02 }}
                  >
                    <motion.div 
                      className="w-2 h-2 bg-cyan-bright rounded-full"
                      animate={{ 
                        scale: [1, 1.5, 1],
                        opacity: [1, 0.5, 1]
                      }}
                      transition={{ 
                        duration: 2,
                        repeat: Infinity,
                        delay: i * 0.2
                      }}
                    />
                    <div className="flex-1">
                      <p className="text-sm text-white">Wallet analysis completed</p>
                      <p className="text-xs text-gray-400">{i + 2} minutes ago</p>
                    </div>
                    <div className="text-xs text-green-success font-medium">+$2,340</div>
                  </motion.div>
                ))}
              </div>
            </CardContent>
          </Card>
        </motion.div>
      </div>
    </div>
  )
}