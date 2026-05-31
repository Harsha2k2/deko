export interface Agent {
  id: string
  name: string
  active: boolean
  created_at: string
  deactivated_reason: string | null
  deactivated_at: string | null
}

export interface Action {
  id: string
  agent_id: string
  agent_name: string
  intent: string
  payload: string
  status: ActionStatus
  created_at: string
  updated_at: string | null
  target_url: string | null
  target_method: string | null
  risk_level: RiskLevel | null
  verdict_decision: VerdictDecision | null
  verdict_reason: string | null
}

export type ActionStatus = 'pending' | 'processing' | 'approved' | 'denied' | 'escalated' | 'forwarded'
export type RiskLevel = 'low' | 'medium' | 'high' | 'critical'
export type VerdictDecision = 'approved' | 'denied' | 'escalate'

export interface Verdict {
  id: string
  action_id: string
  decision: VerdictDecision
  reason: string
  risk_level: RiskLevel
  policy_matched: string | null
  llm_raw_response: string | null
  confidence: number | null
  created_at: string
}

export interface Policy {
  id: string
  name: string
  rules_json: string
  active: boolean
  priority: number
  created_at: string
  updated_at: string
}

export interface AuditLog {
  id: string
  action_id: string | null
  event_type: string
  details: Record<string, unknown> | null
  created_at: string
}

export interface DashboardStats {
  total_actions: number
  pending_actions: number
  approved_actions: number
  denied_actions: number
  escalated_actions: number
  active_agents: number
  active_policies: number
}

export interface TimelineEntry {
  date: string
  total: number
  approved: number
  denied: number
  escalated: number
}

export interface ApiKey {
  id: string
  agent_id: string
  label: string
  key_prefix: string
  expires_at: string | null
  active: boolean
  created_at: string
  last_used_at: string | null
}
