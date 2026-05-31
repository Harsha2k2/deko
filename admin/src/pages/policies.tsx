import { useEffect, useState, type FormEvent } from 'react'
import { motion } from 'motion/react'
import { Plus, Trash2, Play, Beaker } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { Badge } from '@/components/ui/badge'
import { api } from '@/api/client'
import type { Policy } from '@/types'
import { toast } from 'sonner'

interface SimulateResult {
  policy_name: string
  matched: boolean
  immediate_deny: boolean
  reason: string
  risk_level?: string | null
}

export default function Policies() {
  const [policies, setPolicies] = useState<Policy[]>([])
  const [loading, setLoading] = useState(true)
  const [name, setName] = useState('')
  const [rules, setRules] = useState('{}')
  const [creating, setCreating] = useState(false)

  const [simIntent, setSimIntent] = useState('')
  const [simPayload, setSimPayload] = useState('')
  const [simUrl, setSimUrl] = useState('')
  const [simResult, setSimResult] = useState<SimulateResult[] | null>(null)
  const [simulating, setSimulating] = useState(false)

  const fetchPolicies = () => {
    api.listPolicies()
      .then(setPolicies)
      .catch(() => {})
      .finally(() => setLoading(false))
  }

  useEffect(() => { fetchPolicies() }, [])

  const handleCreate = async (e: FormEvent) => {
    e.preventDefault()
    if (!name.trim()) return
    setCreating(true)
    try {
      await api.createPolicy({ name: name.trim(), rules_json: rules, active: true, priority: 5 })
      toast.success('Policy created')
      setName('')
      setRules('{}')
      fetchPolicies()
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to create policy')
    } finally {
      setCreating(false)
    }
  }

  const handleDelete = async (id: string) => {
    try {
      await api.deletePolicy(id)
      toast.success('Policy deleted')
      fetchPolicies()
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to delete policy')
    }
  }

  const handleTest = async (policy: Policy) => {
    try {
      const result = await api.testPolicy({
        rules: policy.rules_json,
        intent: 'test-action',
        payload: '{}',
      })
      toast.info(`Result: ${result.matched ? 'Matched' : 'No match'} - ${result.reason}`)
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Test failed')
    }
  }

  const handleSimulate = async () => {
    if (!simIntent.trim()) {
      toast.error('Intent is required')
      return
    }
    setSimulating(true)
    setSimResult(null)
    try {
      const result = await api.simulateActions({
        intent: simIntent.trim(),
        payload: simPayload.trim() || undefined,
        target_url: simUrl.trim() || undefined,
      })
      setSimResult(result)
      if (result.length === 0) {
        toast.info('No active policies to test against')
      }
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Simulation failed')
    } finally {
      setSimulating(false)
    }
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Policies</h1>
        <p className="text-sm text-muted-foreground">Define rules that govern agent behavior</p>
      </div>

      <motion.div
        initial={{ opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.2 }}
      >
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Create Policy</CardTitle>
          </CardHeader>
          <CardContent>
            <form onSubmit={handleCreate} className="space-y-3">
              <Input
                placeholder="Policy name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                className="max-w-xs"
              />
              <div>
                <p className="mb-1 text-xs text-muted-foreground">Rules (JSON)</p>
                <textarea
                  className="flex min-h-[100px] w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-sm"
                  value={rules}
                  onChange={(e) => setRules(e.target.value)}
                />
              </div>
              <Button type="submit" disabled={creating}>
                <Plus className="mr-1 h-4 w-4" /> {creating ? 'Creating...' : 'Create'}
              </Button>
            </form>
          </CardContent>
        </Card>
      </motion.div>

      <motion.div
        initial={{ opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.05, duration: 0.2 }}
      >
        <Card className="border-indigo-500/30">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-base">
              <Beaker className="h-4 w-4 text-indigo-400" />
              What If Simulator
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <p className="text-xs text-muted-foreground">
              Test a sample action against all active policies to see which rules would fire.
            </p>
            <div className="grid gap-3 sm:grid-cols-3">
              <Input
                placeholder="Intent (e.g. delete_user)"
                value={simIntent}
                onChange={(e) => setSimIntent(e.target.value)}
              />
              <Input
                placeholder="Payload JSON (optional)"
                value={simPayload}
                onChange={(e) => setSimPayload(e.target.value)}
              />
              <Input
                placeholder="Target URL (optional)"
                value={simUrl}
                onChange={(e) => setSimUrl(e.target.value)}
              />
            </div>
            <Button
              variant="secondary"
              onClick={handleSimulate}
              disabled={simulating || !simIntent.trim()}
            >
              {simulating ? 'Simulating...' : 'Simulate'}
            </Button>

            {simResult !== null && (
              <div className="rounded border p-3 text-sm">
                {simResult.length === 0 ? (
                  <p className="text-muted-foreground">No active policies to test against.</p>
                ) : (
                  <div className="space-y-2">
                    {simResult.map((r, i) => (
                      <div key={i} className="flex items-start gap-2 rounded bg-muted/50 p-2">
                        <Badge
                          variant={r.immediate_deny ? 'denied' : r.matched ? 'default' : 'approved'}
                          className="shrink-0 mt-0.5"
                        >
                          {r.immediate_deny ? 'DENY' : r.matched ? 'FLAG' : 'PASS'}
                        </Badge>
                        <div className="min-w-0">
                          <p className="font-medium">{r.policy_name}</p>
                          <p className="text-xs text-muted-foreground truncate">
                            {r.reason}{r.risk_level ? ` (risk: ${r.risk_level})` : ''}
                          </p>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            )}
          </CardContent>
        </Card>
      </motion.div>

      <motion.div
        initial={{ opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.1, duration: 0.2 }}
      >
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Active Policies</CardTitle>
          </CardHeader>
          <CardContent>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Priority</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {loading ? (
                  <TableRow>
                    <TableCell colSpan={4} className="h-24 text-center">Loading...</TableCell>
                  </TableRow>
                ) : policies.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={4} className="h-24 text-center text-muted-foreground">
                      No policies defined
                    </TableCell>
                  </TableRow>
                ) : (
                  policies.map((policy) => (
                    <TableRow key={policy.id}>
                      <TableCell className="font-medium">{policy.name}</TableCell>
                      <TableCell>{policy.priority}</TableCell>
                      <TableCell>
                        <Badge variant={policy.active ? 'approved' : 'denied'}>
                          {policy.active ? 'Active' : 'Inactive'}
                        </Badge>
                      </TableCell>
                      <TableCell>
                        <div className="flex gap-1">
                          <Button variant="ghost" size="sm" onClick={() => handleTest(policy)}>
                            <Play className="h-3 w-3" />
                          </Button>
                          <Button variant="ghost" size="sm" onClick={() => handleDelete(policy.id)}>
                            <Trash2 className="h-3 w-3" />
                          </Button>
                        </div>
                      </TableCell>
                    </TableRow>
                  ))
                )}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      </motion.div>
    </div>
  )
}
