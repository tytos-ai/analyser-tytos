import { createContext, useContext, useEffect, ReactNode } from 'react'
import { socketService } from '@/lib/socket'

const SocketContext = createContext(socketService)

interface SocketProviderProps {
  children: ReactNode
}

export function SocketProvider({ children }: SocketProviderProps) {
  useEffect(() => {
    socketService.connect()
    socketService.startMockData()

    return () => {
      socketService.disconnect()
    }
  }, [])

  return (
    <SocketContext.Provider value={socketService}>
      {children}
    </SocketContext.Provider>
  )
}

export const useSocket = () => {
  const context = useContext(SocketContext)
  if (!context) {
    throw new Error('useSocket must be used within a SocketProvider')
  }
  return context
}