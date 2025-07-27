// Browser-compatible EventEmitter implementation
class EventEmitter {
  private events: { [key: string]: Function[] } = {}

  on(event: string, listener: Function) {
    if (!this.events[event]) {
      this.events[event] = []
    }
    this.events[event].push(listener)
  }

  off(event: string, listener: Function) {
    if (!this.events[event]) return
    this.events[event] = this.events[event].filter(l => l !== listener)
  }

  emit(event: string, ...args: any[]) {
    if (!this.events[event]) return false
    this.events[event].forEach(listener => listener(...args))
    return true
  }

  removeAllListeners(event?: string) {
    if (event) {
      delete this.events[event]
    } else {
      this.events = {}
    }
  }
}

class SocketService extends EventEmitter {
  private connected = false
  private mockDataInterval: NodeJS.Timeout | null = null

  connect() {
    // Simulate connection without actual WebSocket
    setTimeout(() => {
      this.connected = true
      console.log('Mock socket connected')
      this.emit('connect')
    }, 100)
  }

  subscribe(event: string, callback: (data: any) => void) {
    this.on(event, callback)
  }

  unsubscribe(event: string, callback?: (data: any) => void) {
    if (callback) {
      this.off(event, callback)
    } else {
      this.removeAllListeners(event)
    }
  }

  emit(event: string, data?: any) {
    super.emit(event, data)
    return true
  }

  disconnect() {
    this.connected = false
    if (this.mockDataInterval) {
      clearInterval(this.mockDataInterval)
      this.mockDataInterval = null
    }
    console.log('Mock socket disconnected')
  }

  // Mock real-time data for demo
  startMockData() {
    if (this.mockDataInterval) {
      clearInterval(this.mockDataInterval)
    }
    
    this.mockDataInterval = setInterval(() => {
      if (this.connected) {
        this.emit('mock:metrics', {
          cpu: Math.floor(Math.random() * 30) + 20,
          memory: Math.floor(Math.random() * 40) + 30,
          activeUsers: Math.floor(Math.random() * 100) + 50
        })
      }
    }, 3000)
  }
}

export const socketService = new SocketService()