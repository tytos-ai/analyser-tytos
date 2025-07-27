import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Search } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'

export function GlobalWalletSearch() {
  const [walletAddress, setWalletAddress] = useState('')
  const navigate = useNavigate()

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (walletAddress.trim()) {
      navigate(`/wallet/${walletAddress.trim()}`)
      setWalletAddress('')
    }
  }

  return (
    <form onSubmit={handleSubmit} className="flex items-center space-x-2 max-w-md">
      <div className="relative flex-1">
        <Search className="absolute left-3 top-1/2 transform -translate-y-1/2 text-gray-400 w-4 h-4" />
        <Input
          type="text"
          placeholder="Enter wallet address..."
          value={walletAddress}
          onChange={(e) => setWalletAddress(e.target.value)}
          className="pl-10 bg-navy-deep/50 border-blue-ice/20 text-white placeholder-gray-400"
        />
      </div>
      <Button 
        type="submit" 
        variant="neon" 
        size="sm"
        disabled={!walletAddress.trim()}
      >
        Analyze
      </Button>
    </form>
  )
}