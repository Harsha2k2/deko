const BASE = ''

async function request<T>(path: string, options: RequestInit = {}): Promise<T> {
  const headers: Record<string, string> = {
    ...(options.headers as Record<string, string>),
  }

  const token = localStorage.getItem('deko-jwt')
  if (token) {
    headers['Authorization'] = `Bearer ${token}`
  }

  // Include cookies for session-based auth
  const res = await fetch(`${BASE}${path}`, {
    ...options,
    headers,
    credentials: 'same-origin',
  })

  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: res.statusText }))
    if (res.status === 403 || res.status === 401) {
      localStorage.removeItem('deko-jwt')
    }
    throw new Error(body.error || `Request failed: ${res.status}`)
  }

  // Handle both JSON and text responses
  const ct = res.headers.get('content-type') || ''
  if (ct.includes('application/json')) {
    return res.json()
  }
  return res.text() as unknown as T
}

export const api = {
  // Auth
  login: (password: string) =>
    request<{ success: boolean }>('/admin/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: new URLSearchParams({ password }),
    }),

  logout: () => request<{ success: boolean }>('/admin/logout', { method: 'POST' }),

  // JWT
  exchangeToken: (apiKey: string) =>
    request<{ token: string; expires_in: number }>('/auth/token', {
      method: 'POST',
      headers: { 'X-API-Key': apiKey },
    }),

  // Dashboard
  dashboard: () => request<import('@/types').DashboardStats>('/api/admin/dashboard'),

  // Actions
  listActions: (params?: string) =>
    request<import('@/types').Action[]>(`/api/admin/actions${params ? `?${params}` : ''}`),

  getAction: (id: string) => request<import('@/types').Action>(`/api/admin/actions/${id}`),

  exportActions: () => {
    window.open(`${BASE}/admin/actions/export`, '_blank')
  },

  // Agents
  listAgents: () =>
    request<import('@/types').Agent[]>('/api/admin/agents'),

  registerAgent: (name: string) =>
    request<{ agent_id: string; api_key: string }>('/admin/agents/register', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name }),
    }),

  // Verdicts
  listVerdicts: () =>
    request<import('@/types').Verdict[]>('/api/admin/verdicts'),

  // Policies
  listPolicies: () =>
    request<import('@/types').Policy[]>('/api/admin/policies'),

  createPolicy: (data: { name: string; rules_json: string; active: boolean; priority: number }) =>
    request<import('@/types').Policy>('/admin/policies', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(data),
    }),

  updatePolicy: (id: string, data: Partial<import('@/types').Policy>) =>
    request<import('@/types').Policy>(`/admin/policies/${id}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(data),
    }),

  deletePolicy: (id: string) =>
    request<void>(`/admin/policies/${id}`, { method: 'DELETE' }),

  testPolicy: (data: { policy: import('@/types').Policy; action: Record<string, unknown> }) =>
    request<{ matched: boolean; reason: string }>('/admin/policies/test', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(data),
    }),

  // Audit
  listAuditLog: (params?: string) =>
    request<import('@/types').AuditLog[]>(`/api/admin/audit${params ? `?${params}` : ''}`),

  searchAuditLog: (query: string) =>
    request<import('@/types').AuditLog[]>(`/admin/audit/search?q=${encodeURIComponent(query)}`),

  exportAuditLog: () => {
    window.open(`${BASE}/admin/audit/export`, '_blank')
  },
}
