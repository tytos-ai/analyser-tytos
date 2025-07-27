'use client'

import { useRouter } from 'next/navigation'
import { motion } from 'framer-motion'
import { Button } from '@/components/ui/button'
import { ArrowRight, Zap, Shield, TrendingUp } from 'lucide-react'

export default function Landing() {
  const router = useRouter()

  const handleEnterDemo = () => {
    router.push('/dashboard')
  }

  return (
    <div className="min-h-screen bg-navy-deep relative overflow-hidden">
      {/* Video Background */}
      <div className="absolute inset-0 z-0">
        <video
          autoPlay
          loop
          muted
          playsInline
          className="w-full h-full object-cover"
        >
          <source src="/video-placeholder.mp4" type="video/mp4" />
        </video>
        <div className="absolute inset-0 video-overlay" />
      </div>

      {/* Content */}
      <div className="relative z-10 min-h-screen flex flex-col items-center justify-center px-4">
        <motion.div
          initial={{ opacity: 0, y: 30 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.8 }}
          className="text-center max-w-4xl mx-auto"
        >
          {/* Logo */}
          <motion.div
            initial={{ scale: 0.8, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            transition={{ duration: 0.6, delay: 0.2 }}
            className="mb-8"
          >
            <div className="w-20 h-20 bg-gradient-to-br from-cyan-bright to-blue-steel rounded-2xl flex items-center justify-center mx-auto mb-6 animate-glow">
              <Zap className="w-10 h-10 text-white" />
            </div>
          </motion.div>

          {/* Title */}
          <motion.h1
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.8, delay: 0.4 }}
            className="text-5xl md:text-7xl font-bold mb-6 gradient-text leading-tight"
          >
            Wallet Analyzer
          </motion.h1>

          {/* Subtitle */}
          <motion.p
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.8, delay: 0.6 }}
            className="text-xl md:text-2xl text-gray-300 mb-12 leading-relaxed"
          >
            Track, analyze, and follow the best traders on Solana.
            <br />
            <span className="text-cyan-bright">Discover profitable strategies</span> with real-time insights.
          </motion.p>

          {/* CTA Button */}
          <motion.div
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.8, delay: 0.8 }}
            className="mb-16"
          >
            <Button
              size="lg"
              variant="neon"
              onClick={handleEnterDemo}
              className="text-lg px-8 py-4 group"
            >
              Enter Demo Mode
              <ArrowRight className="ml-2 w-5 h-5 group-hover:translate-x-1 transition-transform" />
            </Button>
          </motion.div>

          {/* Features */}
          <motion.div
            initial={{ opacity: 0, y: 30 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.8, delay: 1 }}
            className="grid grid-cols-1 md:grid-cols-3 gap-8 max-w-3xl mx-auto"
          >
            <div className="glass-card p-6 rounded-lg">
              <Shield className="w-8 h-8 text-cyan-bright mb-4 mx-auto" />
              <h3 className="text-lg font-semibold text-white mb-2">Secure Analysis</h3>
              <p className="text-gray-400 text-sm">Enterprise-grade security for your wallet tracking</p>
            </div>
            <div className="glass-card p-6 rounded-lg">
              <TrendingUp className="w-8 h-8 text-cyan-bright mb-4 mx-auto" />
              <h3 className="text-lg font-semibold text-white mb-2">Real-time Data</h3>
              <p className="text-gray-400 text-sm">Live market data and instant trade notifications</p>
            </div>
            <div className="glass-card p-6 rounded-lg">
              <Zap className="w-8 h-8 text-cyan-bright mb-4 mx-auto" />
              <h3 className="text-lg font-semibold text-white mb-2">AI Insights</h3>
              <p className="text-gray-400 text-sm">Advanced analytics powered by machine learning</p>
            </div>
          </motion.div>
        </motion.div>

        {/* Floating particles */}
        <div className="absolute inset-0 pointer-events-none">
          {Array.from({ length: 20 }).map((_, i) => (
            <motion.div
              key={i}
              className="absolute w-2 h-2 bg-cyan-bright rounded-full opacity-20"
              animate={{
                y: [0, -100, 0],
                x: [0, Math.random() * 100 - 50, 0],
                opacity: [0.2, 0.8, 0.2]
              }}
              transition={{
                duration: 3 + Math.random() * 2,
                repeat: Infinity,
                delay: Math.random() * 2
              }}
              style={{
                left: `${Math.random() * 100}%`,
                top: `${Math.random() * 100}%`
              }}
            />
          ))}
        </div>
      </div>
    </div>
  )
}