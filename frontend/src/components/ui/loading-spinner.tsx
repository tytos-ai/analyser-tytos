import { motion } from 'framer-motion'
import { cn } from '@/lib/utils'
import { useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api'

interface LoadingSpinnerProps {
  size?: 'sm' | 'md' | 'lg'
  className?: string
}

export function LoadingSpinner({ size = 'md', className }: LoadingSpinnerProps) {
  const sizeClasses = {
    sm: 'w-4 h-4',
    md: 'w-8 h-8',
    lg: 'w-12 h-12'
  }

  return (
    <div className={cn('flex items-center justify-center', className)}>
      <motion.div
        className={cn(
          'border-2 border-cyan-bright/20 border-t-cyan-bright rounded-full',
          sizeClasses[size]
        )}
        animate={{ rotate: 360 }}
        transition={{
          duration: 1,
          repeat: Infinity,
          ease: "linear"
        }}
      />
    </div>
  )
}

export function PulsingDot({ className }: { className?: string }) {
  return (
    <motion.div
      className={cn('w-2 h-2 bg-green-success rounded-full', className)}
      animate={{
        scale: [1, 1.5, 1],
        opacity: [1, 0.5, 1]
      }}
      transition={{
        duration: 2,
        repeat: Infinity,
        ease: "easeInOut"
      }}
    />
  )
}

export function SystemHealthIndicator() {
  const { data: health, isError } = useQuery({
    queryKey: ['system-health-indicator'],
    queryFn: api.monitoring.getDetailedHealth,
    refetchInterval: 10000, // Refresh every 10 seconds
    retry: 1
  })

  // Fallback to "unknown" state when API is unavailable
  const effectiveHealth = isError ? { status: 'unknown' } : health

  const getHealthColor = (status?: string) => {
    switch (status) {
      case 'healthy':
        return 'bg-green-success'
      case 'degraded':
        return 'bg-orange-warning'
      case 'down':
        return 'bg-red-500'
      default:
        return 'bg-gray-500'
    }
  }

  const getHealthText = (status?: string) => {
    switch (status) {
      case 'healthy':
        return 'System Healthy'
      case 'degraded':
        return 'System Degraded'
      case 'down':
        return 'System Down'
      case 'unknown':
        return 'System Connecting...'
      default:
        return 'System Unknown'
    }
  }

  const getHealthTextColor = (status?: string) => {
    switch (status) {
      case 'healthy':
        return 'text-green-success'
      case 'degraded':
        return 'text-orange-warning'
      case 'down':
        return 'text-red-500'
      default:
        return 'text-gray-500'
    }
  }

  const healthColor = getHealthColor(effectiveHealth?.status)
  const healthText = getHealthText(effectiveHealth?.status)
  const textColor = getHealthTextColor(effectiveHealth?.status)

  return (
    <div className="flex items-center space-x-2">
      <motion.div
        className={`w-3 h-3 rounded-full ${healthColor}`}
        animate={{
          boxShadow: effectiveHealth?.status === 'healthy' ? [
            '0 0 0 0 rgba(16, 185, 129, 0.7)',
            '0 0 0 10px rgba(16, 185, 129, 0)',
            '0 0 0 0 rgba(16, 185, 129, 0)'
          ] : effectiveHealth?.status === 'degraded' ? [
            '0 0 0 0 rgba(245, 158, 11, 0.7)',
            '0 0 0 8px rgba(245, 158, 11, 0)',
            '0 0 0 0 rgba(245, 158, 11, 0)'
          ] : [
            '0 0 0 0 rgba(156, 163, 175, 0.7)',
            '0 0 0 6px rgba(156, 163, 175, 0)',
            '0 0 0 0 rgba(156, 163, 175, 0)'
          ]
        }}
        transition={{
          duration: effectiveHealth?.status === 'healthy' ? 2 : effectiveHealth?.status === 'degraded' ? 1.5 : 1,
          repeat: Infinity,
          ease: "easeOut"
        }}
      />
      <span className={`text-sm font-medium ${textColor}`}>{healthText}</span>
    </div>
  )
}