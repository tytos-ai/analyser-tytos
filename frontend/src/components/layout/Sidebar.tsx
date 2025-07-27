import { useState } from 'react'
import Link from 'next/link'
import { usePathname, useRouter } from 'next/navigation'
import { WalletIcon } from '@/components/icons/WalletIcon'
import { 
  LayoutDashboard, 
  Settings, 
  Activity, 
  Briefcase, 
  Wallet,
  TrendingUp,
  Menu,
  X,
  LogOut
} from 'lucide-react'
import { cn } from '@/lib/utils'
import { useUI } from '@/store/ui'
import { Button } from '@/components/ui/button'

const navigation = [
  { name: 'Dashboard', href: '/dashboard', icon: LayoutDashboard },
  { name: 'Services', href: '/services', icon: Settings },
  { name: 'Monitoring', href: '/monitoring', icon: Activity },
  { name: 'Batch Jobs', href: '/jobs', icon: Briefcase },
  { name: 'Wallet Results', href: '/results', icon: Wallet },
  { name: 'DEX Monitor', href: '/dex', icon: TrendingUp },
  { name: 'Wallet Profile', href: '/wallet/demo-wallet-address', icon: Wallet },
]

export function Sidebar() {
  const pathname = usePathname()
  const router = useRouter()
  const { sidebarOpen, setSidebarOpen } = useUI()

  return (
    <>
      {/* Mobile backdrop */}
      {sidebarOpen && (
        <div 
          className="fixed inset-0 bg-black/50 z-40 lg:hidden"
          onClick={() => setSidebarOpen(false)}
        />
      )}

      {/* Sidebar */}
      <div className={cn(
        "fixed inset-y-0 left-0 z-50 w-64 bg-charcoal border-r border-blue-ice/20 transform transition-transform duration-300 ease-in-out lg:translate-x-0",
        sidebarOpen ? "translate-x-0" : "-translate-x-full"
      )}>
        <div className="flex flex-col h-full">
          {/* Header */}
          <div className="flex items-center justify-between p-6 border-b border-blue-ice/20">
            <div className="flex items-center space-x-3">
              <div className="w-8 h-8 bg-gradient-to-br from-cyan-bright to-blue-steel rounded-lg flex items-center justify-center">
                <WalletIcon size={20} className="text-white" />
              </div>
              <span className="text-xl font-bold gradient-text">Wallet Analyser</span>
            </div>
            <Button 
              variant="ghost" 
              size="icon"
              onClick={() => setSidebarOpen(false)}
              className="lg:hidden"
            >
              <X className="w-5 h-5" />
            </Button>
          </div>

          {/* Navigation */}
          <nav className="flex-1 px-4 py-6 space-y-2">
            {navigation.map((item) => {
              const isActive = pathname === item.href
              const Icon = item.icon
              
              return (
                <Link
                  key={item.name}
                  href={item.href}
                  className={cn(
                    "flex items-center space-x-3 px-4 py-3 rounded-lg transition-all duration-200",
                    isActive 
                      ? "bg-cyan-bright/20 text-cyan-bright border border-cyan-bright/30 neon-glow" 
                      : "text-gray-300 hover:bg-blue-steel/20 hover:text-white"
                  )}
                  onClick={() => setSidebarOpen(false)}
                >
                  <Icon className="w-5 h-5" />
                  <span className="font-medium">{item.name}</span>
                </Link>
              )
            })}
          </nav>

          {/* Footer */}
          <div className="p-4 border-t border-blue-ice/20">
            <Button 
              variant="ghost" 
              onClick={() => router.push('/')}
              className="w-full justify-start space-x-3 text-gray-300 hover:text-white hover:bg-red-500/20"
            >
              <LogOut className="w-5 h-5" />
              <span>Logout</span>
            </Button>
          </div>
        </div>
      </div>
    </>
  )
}