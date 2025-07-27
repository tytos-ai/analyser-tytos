import React from 'react'

interface WalletIconProps {
  className?: string
  size?: number
}

export function WalletIcon({ className = '', size = 24 }: WalletIconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
    >
      {/* Wallet body */}
      <rect
        x="3"
        y="6"
        width="18"
        height="12"
        rx="2"
        ry="2"
        stroke="currentColor"
        strokeWidth="2"
        fill="none"
      />
      
      {/* Wallet flap */}
      <path
        d="M3 8V6a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v2"
        stroke="currentColor"
        strokeWidth="2"
        fill="none"
      />
      
      {/* Card slot */}
      <rect
        x="6"
        y="9"
        width="8"
        height="1"
        rx="0.5"
        fill="currentColor"
        opacity="0.6"
      />
      
      {/* Coin/button */}
      <circle
        cx="16"
        cy="12"
        r="1.5"
        stroke="currentColor"
        strokeWidth="1.5"
        fill="none"
      />
    </svg>
  )
}