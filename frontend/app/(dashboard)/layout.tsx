'use client'

import { Sidebar } from '@/components/layout/Sidebar'
import { Header } from '@/components/layout/Header'
import { QueryProvider } from '@/providers/QueryProvider'
import { SocketProvider } from '@/providers/SocketProvider'

export default function DashboardLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return (
    <QueryProvider>
      <SocketProvider>
        <div className="min-h-screen bg-navy-deep">
          <Sidebar />
          <div className="lg:ml-64">
            <Header />
            <main className="p-6">
              {children}
            </main>
          </div>
        </div>
      </SocketProvider>
    </QueryProvider>
  )
}