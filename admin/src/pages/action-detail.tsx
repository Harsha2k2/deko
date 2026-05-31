import { useEffect, useState } from 'react'
import { useParams, Link } from 'react-router-dom'
import { motion } from 'motion/react'
import { ArrowLeft } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { StatusBadge } from '@/components/status-badge'
import { api } from '@/api/client'
import type { Action } from '@/types'

export default function ActionDetail() {
  const { id } = useParams<{ id: string }>()
  const [action, setAction] = useState<Action | null>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    if (!id) return
    api.getAction(id)
      .then(setAction)
      .catch(() => {})
      .finally(() => setLoading(false))
  }, [id])

  if (loading) {
    return (
      <div className="flex items-center justify-center py-20">
        <div className="h-8 w-8 animate-spin rounded-full border-4 border-primary border-t-transparent" />
      </div>
    )
  }

  if (!action) {
    return <div className="py-20 text-center text-muted-foreground">Action not found</div>
  }

  const rows = [
    { label: 'ID', value: action.id },
    { label: 'Agent', value: action.agent_name },
    { label: 'Intent', value: action.intent },
    { label: 'Status', value: <StatusBadge status={action.status} /> },
    { label: 'Risk Level', value: action.risk_level ?? '-' },
    { label: 'Verdict', value: action.verdict_decision ?? 'Pending' },
    { label: 'Reason', value: action.verdict_reason ?? '-' },
    { label: 'Target URL', value: action.target_url ?? '-' },
    { label: 'Target Method', value: action.target_method ?? '-' },
    { label: 'Created', value: new Date(action.created_at).toLocaleString() },
    { label: 'Updated', value: action.updated_at ? new Date(action.updated_at).toLocaleString() : '-' },
  ]

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-4">
        <Button variant="ghost" size="icon" asChild>
          <Link to="/admin/actions">
            <ArrowLeft className="h-4 w-4" />
          </Link>
        </Button>
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Action Detail</h1>
          <p className="text-sm text-muted-foreground">{action.id}</p>
        </div>
      </div>

      <motion.div
        initial={{ opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.2 }}
      >
        <Card>
          <CardHeader>
            <CardTitle>Action Information</CardTitle>
          </CardHeader>
          <CardContent>
            <dl className="divide-y">
              {rows.map(({ label, value }) => (
                <div key={label} className="flex justify-between py-3 text-sm">
                  <dt className="text-muted-foreground">{label}</dt>
                  <dd className="font-medium">{value}</dd>
                </div>
              ))}
            </dl>
          </CardContent>
        </Card>
      </motion.div>

      {action.payload && (
        <motion.div
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.1, duration: 0.2 }}
        >
          <Card>
            <CardHeader>
              <CardTitle>Payload</CardTitle>
            </CardHeader>
            <CardContent>
              <pre className="overflow-auto rounded bg-muted p-4 text-xs">{action.payload}</pre>
            </CardContent>
          </Card>
        </motion.div>
      )}
    </div>
  )
}
