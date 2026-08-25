import { useEffect, useState, type FormEvent } from 'react'
import { motion } from 'motion/react'
import { Plus, Key, Copy, Check, TriangleAlert } from 'lucide-react'
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
import type { Agent } from '@/types'
import { toast } from 'sonner'

export default function Agents() {
  const [agents, setAgents] = useState<Agent[]>([])
  const [loading, setLoading] = useState(true)
  const [name, setName] = useState('')
  const [creating, setCreating] = useState(false)
  const [createdKey, setCreatedKey] = useState<{ name: string; apiKey: string } | null>(null)
  const [copied, setCopied] = useState(false)

  const fetchAgents = () => {
    api.listAgents()
      .then(setAgents)
      .catch(() => {})
      .finally(() => setLoading(false))
  }

  useEffect(() => { fetchAgents() }, [])

  const handleCreate = async (e: FormEvent) => {
    e.preventDefault()
    if (!name.trim()) return
    setCreating(true)
    try {
      const result = await api.registerAgent(name.trim())
      // the raw key is never retrievable again; force explicit acknowledgment
      setCreatedKey({ name: result.name ?? name.trim(), apiKey: result.api_key })
      setCopied(false)
      setName('')
      fetchAgents()
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to create agent')
    } finally {
      setCreating(false)
    }
  }

  const copyKey = async () => {
    if (!createdKey) return
    try {
      await navigator.clipboard.writeText(createdKey.apiKey)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    } catch {
      toast.error('Clipboard unavailable — select and copy manually')
    }
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Agents</h1>
        <p className="text-sm text-muted-foreground">Manage AI agents and their API keys</p>
      </div>

      {createdKey && (
        <motion.div initial={{ opacity: 0, y: -8 }} animate={{ opacity: 1, y: 0 }}>
          <Card className="border-amber-500/50 bg-amber-500/5">
            <CardHeader>
              <CardTitle className="flex items-center gap-2 text-base">
                <TriangleAlert className="h-4 w-4 text-amber-500" />
                save "{createdKey.name}"'s api key now — shown only once
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-3">
              <div className="flex items-center gap-2">
                <code className="flex-1 rounded-md border bg-muted px-3 py-2 font-mono text-sm break-all">
                  {createdKey.apiKey}
                </code>
                <Button type="button" variant="outline" size="sm" onClick={copyKey}>
                  {copied ? <Check className="mr-1 h-4 w-4" /> : <Copy className="mr-1 h-4 w-4" />}
                  {copied ? 'Copied' : 'Copy'}
                </Button>
              </div>
              <p className="text-xs text-muted-foreground">
                deko stores only a hash of this key. losing it means rotating the key.
              </p>
              <Button
                type="button"
                size="sm"
                variant={copied ? 'default' : 'secondary'}
                onClick={() => setCreatedKey(null)}
              >
                I've stored it safely
              </Button>
            </CardContent>
          </Card>
        </motion.div>
      )}

      <motion.div
        initial={{ opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.2 }}
      >
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Register New Agent</CardTitle>
          </CardHeader>
          <CardContent>
            <form onSubmit={handleCreate} className="flex gap-2">
              <Input
                placeholder="Agent name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                className="max-w-xs"
              />
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
            <CardTitle className="text-base">Registered Agents</CardTitle>
          </CardHeader>
          <CardContent>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Created</TableHead>
                  <TableHead>Deactivated</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {loading ? (
                  <TableRow>
                    <TableCell colSpan={4} className="h-24 text-center">Loading...</TableCell>
                  </TableRow>
                ) : agents.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={4} className="h-24 text-center text-muted-foreground">
                      No agents registered
                    </TableCell>
                  </TableRow>
                ) : (
                  agents.map((agent) => (
                    <TableRow key={agent.id}>
                      <TableCell className="font-medium">{agent.name}</TableCell>
                      <TableCell>
                        <Badge variant={agent.active ? 'approved' : 'denied'}>
                          {agent.active ? 'Active' : 'Inactive'}
                        </Badge>
                      </TableCell>
                      <TableCell>{new Date(agent.created_at).toLocaleDateString()}</TableCell>
                      <TableCell>{agent.deactivated_reason ?? '-'}</TableCell>
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
