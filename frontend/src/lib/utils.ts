import { type ClassValue, clsx } from "clsx"
import { twMerge } from "tailwind-merge"

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

export function formatCurrency(amount: number): string {
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format(amount)
}

export function formatPercentage(value: number): string {
  return `${value >= 0 ? '+' : ''}${value.toFixed(2)}%`
}

export function formatNumber(num: number): string {
  if (num >= 1e9) {
    return (num / 1e9).toFixed(1) + 'B'
  }
  if (num >= 1e6) {
    return (num / 1e6).toFixed(1) + 'M'
  }
  if (num >= 1e3) {
    return (num / 1e3).toFixed(1) + 'K'
  }
  return num.toString()
}

export function truncateAddress(address: string, startChars = 6, endChars = 4): string {
  if (address.length <= startChars + endChars) {
    return address
  }
  return `${address.slice(0, startChars)}...${address.slice(-endChars)}`
}

// Color utility functions for financial values
export function getPnLColorClass(value: number | string): string {
  const numValue = typeof value === 'string' ? parseFloat(value) : value
  
  if (isNaN(numValue) || numValue === 0) {
    return 'text-gray-400' // Neutral color for zero values
  }
  
  return numValue > 0 ? 'text-green-success' : 'text-red-400'
}

export function getWinRateColorClass(winRate: number | string): string {
  const numValue = typeof winRate === 'string' ? parseFloat(winRate) : winRate
  
  if (isNaN(numValue) || numValue === 0) {
    return 'text-gray-400' // Neutral color for zero values
  }
  
  if (numValue >= 70) return 'text-green-success'
  if (numValue >= 50) return 'text-orange-warning'
  return 'text-red-400'
}

export function getValueColorClass(value: number | string, positiveClass = 'text-green-success', negativeClass = 'text-red-400', zeroClass = 'text-gray-400'): string {
  const numValue = typeof value === 'string' ? parseFloat(value) : value
  
  if (isNaN(numValue) || numValue === 0) {
    return zeroClass
  }
  
  return numValue > 0 ? positiveClass : negativeClass
}