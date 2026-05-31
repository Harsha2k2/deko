import { useEffect, useState, type FormEvent } from 'react'
import { motion } from 'motion/react'
import { Plus, Trash2, Play } from 'lucide-react'
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

export default function Policies() {
  const [policies, setPolicies] = useState<Policy[]>([])
  const [loading, setLoading] = useState(true)
  const [name, setName] = useState('')
  const [rules, setRules] = useState('{}')
  const [creating, setCreating] = useState(false)

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
      const result = await api.testPolicy({ policy, action: { intent: 'test action', payload: {} } })
      toast.info(`Result: ${result.matched ? 'Matched' : 'No match'} - ${result.reason}`)
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Test failed')
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
