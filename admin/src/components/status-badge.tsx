import { Badge } from '@/components/ui/badge'

const statusMap: Record<string, 'approved' | 'denied' | 'escalated' | 'pending' | 'processing'> = {
  approved: 'approved',
  denied: 'denied',
  escalated: 'escalated',
  pending: 'pending',
  processing: 'processing',
  forwarded: 'processing',
}

export function StatusBadge({ status }: { status: string }) {
  const variant = statusMap[status] || 'default'
  return <Badge variant={variant}>{status}</Badge>
}
