import { useEffect, useState } from 'react'
import { useParams, Link } from 'react-router-dom'
import { motion } from 'motion/react'
import { ArrowLeft, Check, X, AlertTriangle, Brain, ChevronDown, ChevronRight } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { StatusBadge } from '@/components/status-badge'
import { Input } from '@/components/ui/input'
import { api } from '@/api/client'
import { toast } from 'sonner'
import type { Action } from '@/types'

interface VerdictExplain {
  id: string
  action_id: string
  decision: string
  reason: string
  risk_level: string
  policy_matched: string | null
  reasoning_chain: string | null
  confidence: number | null
  provider: string | null
  model: string | null
  created_at: string
}

export default function ActionDetail() {
  const { id } = useParams<{ id: string }>()
  const [action, setAction] = useState<Action | null>(null)
  const [loading, setLoading] = useState(true)
  const [overrideReason, setOverrideReason] = useState('')
  const [overriding, setOverriding] = useState(false)
  const [verdictExplain, setVerdictExplain] = useState<VerdictExplain | null>(null)
  const [explainLoading, setExplainLoading] = useState(false)
  const [explainExpanded, setExplainExpanded] = useState(false)

  useEffect(() => {
    if (!id) return
    api.getAction(id)
      .then(setAction)
      .catch(() => {})
      .finally(() => setLoading(false))
  }, [id])

  useEffect(() => {
    if (!action?.verdict_decision) return
    setExplainLoading(true)
    api.explainVerdict(action.id)
      .then(setVerdictExplain)
      .catch(() => {})
      .finally(() => setExplainLoading(false))
  }, [action?.id, action?.verdict_decision])

  const handleOverride = async (decision: string) => {
    if (!overrideReason.trim()) {
      toast.error('Override reason is required')
      return
    }
    if (!id) return
    setOverriding(true)
    try {
      const result = await api.overrideAction(id, decision, overrideReason)
      toast.success(`Action ${result.new_status}`)
      setAction(prev => prev ? { ...prev, status: result.new_status as Action['status'], verdict_decision: result.new_status === 'approved' ? 'approved' : prev.verdict_decision } : prev)
      setOverrideReason('')
    } catch (e) {
      toast.error(e instanceof Error ? e.message : 'Override failed')
    } finally {
      setOverriding(false)
    }
  }

  const canOverride = action && (action.status === 'denied' || action.status === 'escalated' || action.status === 'pending')

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

      {canOverride && (
        <motion.div
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.15, duration: 0.2 }}
        >
          <Card className="border-amber-500/30">
            <CardHeader>
              <CardTitle className="flex items-center gap-2 text-amber-500">
                <AlertTriangle className="h-4 w-4" />
                Admin Override
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-3">
              <p className="text-sm text-muted-foreground">
                Current status: <StatusBadge status={action.status} /> — Enter a reason and choose an action.
              </p>
              <Input
                placeholder="Reason for override..."
                value={overrideReason}
                onChange={e => setOverrideReason(e.target.value)}
                disabled={overriding}
              />
              <div className="flex gap-2">
                <Button
                  variant="default"
                  size="sm"
                  disabled={overriding || !overrideReason.trim()}
                  onClick={() => handleOverride('approved')}
                >
                  <Check className="mr-1 h-3 w-3" /> Approve
                </Button>
                <Button
                  variant="destructive"
                  size="sm"
                  disabled={overriding || !overrideReason.trim()}
                  onClick={() => handleOverride('denied')}
                >
                  <X className="mr-1 h-3 w-3" /> Deny
                </Button>
                {action.status === 'pending' && (
                  <Button
                    variant="secondary"
                    size="sm"
                    disabled={overriding || !overrideReason.trim()}
                    onClick={() => handleOverride('escalated')}
                  >
                    <AlertTriangle className="mr-1 h-3 w-3" /> Escalate
                  </Button>
                )}
              </div>
            </CardContent>
          </Card>
        </motion.div>
      )}

      {verdictExplain?.reasoning_chain && (
        <motion.div
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.2, duration: 0.2 }}
        >
          <Card className="border-emerald-500/30">
            <CardHeader>
              <CardTitle className="flex items-center gap-2 text-emerald-500">
                <Brain className="h-4 w-4" />
                AI Reasoning
                <Button
                  variant="ghost"
                  size="icon"
                  className="ml-auto h-6 w-6 p-0"
                  onClick={() => setExplainExpanded(!explainExpanded)}
                >
                  {explainExpanded ? <ChevronDown className="h-4 w-4" /> : <ChevronRight className="h-4 w-4" />}
                </Button>
              </CardTitle>
            </CardHeader>
            <CardContent>
              {explainExpanded && (
                <div className="space-y-2">
                  <div className="flex items-center gap-2 text-xs text-muted-foreground mb-2">
                    <span className="px-2 py-0.5 rounded bg-emerald-500/10 text-emerald-500">
                      {verdictExplain.confidence !== null && `${Math.round(verdictExplain.confidence * 100)}% confidence`}
                    </span>
                    {verdictExplain.provider && <span className="px-2 py-0.5 rounded bg-muted">{verdictExplain.provider}: {verdictExplain.model}</span>}
                    {verdictExplain.policy_matched && <span className="px-2 py-0.5 rounded bg-amber-500/10 text-amber-500">Policy: {verdictExplain.policy_matched}</span>}
                  </div>
                  <div className="font-mono text-sm space-y-1">
                    {verdictExplain.reasoning_chain.split(' → ').map((step, i) => (
                      <div key={i} className="flex items-start gap-2 text-zinc-300">
                        <span className="text-emerald-500 shrink-0 mt-0.5">→</span>
                        <span>{step}</span>
                      </div>
                    ))}
                  </div>
                </div>
              )}
              {!explainExpanded && (
                <p className="text-sm text-muted-foreground text-center py-2">
                  Click to expand reasoning chain
                </p>
              )}
            </CardContent>
          </Card>
        </motion.div>
      )}
    </div>
  )
}
