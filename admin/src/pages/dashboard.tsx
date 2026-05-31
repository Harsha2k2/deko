import { useEffect, useState } from 'react'
import { motion } from 'motion/react'
import { Shield, AlertTriangle, CheckCircle, Clock, Users, FileText } from 'lucide-react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { api } from '@/api/client'
import type { DashboardStats } from '@/types'

export default function Dashboard() {
  const [stats, setStats] = useState<DashboardStats | null>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    api.dashboard()
      .then(setStats)
      .catch(() => {})
      .finally(() => setLoading(false))
  }, [])

  if (loading) {
    return (
      <div className="flex items-center justify-center py-20">
        <div className="h-8 w-8 animate-spin rounded-full border-4 border-primary border-t-transparent" />
      </div>
    )
  }

  const cards = [
    { label: 'Total Actions', value: stats?.total_actions ?? 0, icon: Shield, color: 'text-blue-500' },
    { label: 'Pending', value: stats?.pending_actions ?? 0, icon: Clock, color: 'text-yellow-500' },
    { label: 'Approved', value: stats?.approved_actions ?? 0, icon: CheckCircle, color: 'text-emerald-500' },
    { label: 'Denied', value: stats?.denied_actions ?? 0, icon: AlertTriangle, color: 'text-red-500' },
    { label: 'Escalated', value: stats?.escalated_actions ?? 0, icon: AlertTriangle, color: 'text-amber-500' },
    { label: 'Active Agents', value: stats?.active_agents ?? 0, icon: Users, color: 'text-purple-500' },
    { label: 'Active Policies', value: stats?.active_policies ?? 0, icon: FileText, color: 'text-cyan-500' },
  ]

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Dashboard</h1>
        <p className="text-sm text-muted-foreground">Overview of your Deko instance</p>
      </div>

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
        {cards.map(({ label, value, icon: Icon, color }, i) => (
          <motion.div
            key={label}
            initial={{ opacity: 0, y: 16 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: i * 0.05, duration: 0.3 }}
          >
            <Card>
              <CardHeader className="flex flex-row items-center justify-between pb-2">
                <CardTitle className="text-sm font-medium text-muted-foreground">{label}</CardTitle>
                <Icon className={`h-4 w-4 ${color}`} />
              </CardHeader>
              <CardContent>
                <div className="text-3xl font-bold">{value}</div>
              </CardContent>
            </Card>
          </motion.div>
        ))}
      </div>
    </div>
  )
}
