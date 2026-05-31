import {
  BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, Area, AreaChart,
  ResponsiveContainer,
} from 'recharts'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import type { TimelineEntry } from '@/types'

const PALETTE = {
  approved: '#06b6d4',
  denied: '#fb7185',
  escalated: '#fbbf24',
  pending: '#52525b',
}

export function ActionTrendsChart({ data }: { data: TimelineEntry[] }) {
  if (data.length === 0) return null

  return (
    <Card className="border-zinc-800/60">
      <CardHeader className="pb-2">
        <CardTitle className="text-sm font-medium text-zinc-400">Actions (Last 30 Days)</CardTitle>
      </CardHeader>
      <CardContent className="pt-0">
        <ResponsiveContainer width="100%" height={200}>
          <AreaChart data={data} margin={{ top: 8, right: 0, bottom: 0, left: -16 }}>
            <defs>
              {(['approved', 'denied', 'escalated'] as const).map(k => (
                <linearGradient key={k} id={`fill-${k}`} x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0%" stopColor={PALETTE[k]} stopOpacity={0.15} />
                  <stop offset="100%" stopColor={PALETTE[k]} stopOpacity={0} />
                </linearGradient>
              ))}
            </defs>
            <CartesianGrid strokeDasharray="3 3" stroke="rgba(255,255,255,0.04)" vertical={false} />
            <XAxis dataKey="date" tick={{ fontSize: 11, fill: '#52525b' }} tickFormatter={(v: string) => v.slice(5)} axisLine={false} tickLine={false} />
            <YAxis tick={{ fontSize: 11, fill: '#52525b' }} allowDecimals={false} axisLine={false} tickLine={false} width={24} />
            <Tooltip
              contentStyle={{ background: '#18181b', border: '1px solid #27272a', borderRadius: '8px', fontSize: '13px', boxShadow: '0 4px 12px rgba(0,0,0,0.3)' }}
              itemStyle={{ color: '#d4d4d8' }}
              labelStyle={{ color: '#71717a' }}
            />
            {(['approved', 'denied', 'escalated'] as const).map((k, i) => (
              <Area key={k} type="monotone" dataKey={k} stroke={PALETTE[k]} strokeWidth={1.5} fill={`url(#fill-${k})`} dot={false} name={k.charAt(0).toUpperCase() + k.slice(1)} animationDuration={400 + i * 150} />
            ))}
          </AreaChart>
        </ResponsiveContainer>
      </CardContent>
    </Card>
  )
}

export function VerdictPieChartCard({
  approved, denied, escalated, pending,
}: {
  approved: number
  denied: number
  escalated: number
  pending: number
}) {
  const items = [
    { label: 'Approved', value: approved, color: PALETTE.approved },
    { label: 'Denied', value: denied, color: PALETTE.denied },
    { label: 'Escalated', value: escalated, color: PALETTE.escalated },
    { label: 'Pending', value: pending, color: PALETTE.pending },
  ].filter(d => d.value > 0)

  const total = items.reduce((s, d) => s + d.value, 0)
  if (total === 0) return null

  return (
    <Card className="border-zinc-800/60">
      <CardHeader className="pb-2">
        <CardTitle className="text-sm font-medium text-zinc-400">Verdict Distribution</CardTitle>
      </CardHeader>
      <CardContent>
        {/* stacked horizontal bar */}
        <div className="mb-4 flex h-8 w-full overflow-hidden rounded-md bg-zinc-800/50">
          {items.map(d => (
            <div
              key={d.label}
              className="transition-all duration-500 ease-out first:rounded-l-md last:rounded-r-md"
              style={{
                width: `${(d.value / total) * 100}%`,
                backgroundColor: d.color,
                opacity: 0.85,
              }}
              title={`${d.label}: ${d.value} (${((d.value / total) * 100).toFixed(1)}%)`}
            />
          ))}
        </div>

        {/* legend rows */}
        <div className="space-y-1.5">
          {items.map(d => {
            const pct = (d.value / total) * 100
            return (
              <div key={d.label} className="flex items-center justify-between text-xs">
                <div className="flex items-center gap-2">
                  <span className="inline-block h-2 w-2 rounded-full shrink-0" style={{ backgroundColor: d.color }} />
                  <span className="text-zinc-400">{d.label}</span>
                </div>
                <div className="flex items-center gap-3">
                  <span className="text-zinc-500 tabular-nums">{d.value}</span>
                  <span className="w-10 text-right text-zinc-600 tabular-nums">{pct.toFixed(1)}%</span>
                </div>
              </div>
            )
          })}
          <div className="flex items-center justify-between border-t border-zinc-800 pt-1.5 text-xs">
            <span className="text-zinc-500">Total</span>
            <span className="text-zinc-400 tabular-nums">{total}</span>
          </div>
        </div>
      </CardContent>
    </Card>
  )
}
