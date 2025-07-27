import { useState } from 'react'
import { Search, X, Menu, Wallet } from 'lucide-react'
import { WalletIcon } from '@/components/icons/WalletIcon'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { useRouter } from 'next/navigation'
import { motion, AnimatePresence } from 'framer-motion'
import { useUI } from '@/store/ui'

export function Header() {
  const [searchOpen, setSearchOpen] = useState(false)
  const [walletAddress, setWalletAddress] = useState('')
  const router = useRouter()
  const { setSidebarOpen } = useUI()

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (walletAddress.trim()) {
      router.push(`/wallet/${walletAddress.trim()}`)
      setWalletAddress('')
      setSearchOpen(false)
    }
  }

  return (
    <>
      <motion.header 
        initial={{ y: -100, opacity: 0 }}
        animate={{ y: 0, opacity: 1 }}
        transition={{ duration: 0.6, ease: "easeOut" }}
        className="sticky top-0 z-40 bg-gradient-to-r from-navy-deep via-charcoal to-navy-deep backdrop-blur-xl border-b border-cyan-bright/20 shadow-2xl"
      >
        <div className="px-6 py-4">
          <div className="flex items-center justify-between">
            <motion.div 
              className="flex items-center space-x-4"
              initial={{ x: -50, opacity: 0 }}
              animate={{ x: 0, opacity: 1 }}
              transition={{ duration: 0.8, delay: 0.2 }}
            >
              {/* Mobile hamburger menu */}
              <Button
                variant="ghost"
                size="icon"
                onClick={() => setSidebarOpen(true)}
                className="lg:hidden text-white hover:bg-cyan-bright/20"
              >
                <Menu className="w-6 h-6" />
              </Button>
              
              <div className="w-12 h-12 bg-gradient-to-br from-cyan-bright via-blue-steel to-blue-soft rounded-2xl flex items-center justify-center shadow-lg">
                <WalletIcon size={24} className="text-white" />
              </div>
              <div>
                <motion.h1 
                  className="text-2xl md:text-3xl font-black bg-gradient-to-r from-cyan-bright via-blue-steel to-blue-soft bg-clip-text text-transparent tracking-tight"
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1 }}
                  transition={{ duration: 1, delay: 0.4 }}
                >
                  Wallet Analyser
                </motion.h1>
                <motion.p 
                  className="text-sm text-gray-300/80 font-medium tracking-wide"
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1 }}
                  transition={{ duration: 1, delay: 0.6 }}
                >
                  Real-time Trading Intelligence
                </motion.p>
              </div>
            </motion.div>

            <motion.div 
              className="flex items-center space-x-4"
              initial={{ x: 50, opacity: 0 }}
              animate={{ x: 0, opacity: 1 }}
              transition={{ duration: 0.8, delay: 0.3 }}
            >
              <Button
                variant="neon"
                size="sm"
                onClick={() => setSearchOpen(true)}
                className="relative overflow-hidden group"
              >
                <motion.div
                  className="absolute inset-0 bg-gradient-to-r from-cyan-bright/20 to-blue-steel/20 opacity-0 group-hover:opacity-100 transition-opacity duration-300"
                />
                <Search className="w-4 h-4 mr-2" />
                <span className="hidden sm:inline">Search Wallet</span>
                <span className="sm:hidden">Search</span>
              </Button>
            </motion.div>
          </div>
        </div>
      </motion.header>

      {/* Search Dialog */}
      <Dialog open={searchOpen} onOpenChange={setSearchOpen}>
        <DialogContent className="glass-card border-cyan-bright/30 max-w-md">
          <DialogHeader>
            <DialogTitle className="text-white flex items-center">
              <Search className="w-5 h-5 mr-2 text-cyan-bright" />
              Analyze Wallet
            </DialogTitle>
          </DialogHeader>
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="relative">
              <Input
                type="text"
                placeholder="Enter wallet address..."
                value={walletAddress}
                onChange={(e) => setWalletAddress(e.target.value)}
                className="bg-navy-deep/50 border-cyan-bright/20 text-white placeholder-gray-400 focus:border-cyan-bright focus:ring-cyan-bright/20"
                autoFocus
              />
            </div>
            <div className="flex space-x-2">
              <Button 
                type="submit" 
                variant="neon" 
                className="flex-1"
                disabled={!walletAddress.trim()}
              >
                Analyze
              </Button>
              <Button 
                type="button" 
                variant="outline" 
                onClick={() => setSearchOpen(false)}
                className="border-gray-500/50 text-gray-400 hover:bg-gray-500/20"
              >
                Cancel
              </Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>
    </>
  )
}