import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from 'recharts'

interface MetricsChartProps {
  data: Array<{
    date: string
    volume: number
    profit: number
    wallets: number
  }>
}

export function MetricsChart({ data }: MetricsChartProps) {
  return (
    <div className="h-80">
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={data}>
          <CartesianGrid strokeDasharray="3 3" stroke="#334155" />
          <XAxis 
            dataKey="date" 
            stroke="#64748B"
            fontSize={12}
          />
          <YAxis 
            stroke="#64748B"
            fontSize={12}
          />
          <Tooltip 
            contentStyle={{
              backgroundColor: '#1E2A3A',
              border: '1px solid #00D4FF',
              borderRadius: '8px',
              color: '#fff'
            }}
          />
          <Line 
            type="monotone" 
            dataKey="volume" 
            stroke="#00D4FF" 
            strokeWidth={2}
            dot={{ fill: '#00D4FF', strokeWidth: 2 }}
          />
          <Line 
            type="monotone" 
            dataKey="profit" 
            stroke="#10B981" 
            strokeWidth={2}
            dot={{ fill: '#10B981', strokeWidth: 2 }}
          />
        </LineChart>
      </ResponsiveContainer>
    </div>
  )
}