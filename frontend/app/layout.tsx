import type { Metadata } from 'next'
import './globals.css'

export const metadata: Metadata = {
  title: 'World-Class Wallet Analyzer Frontend',
  description: 'Professional wallet analysis and monitoring platform',
}

export default function RootLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return (
    <html lang="en">
      <body>
        {children}
      </body>
    </html>
  )
}