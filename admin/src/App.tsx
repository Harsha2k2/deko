import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom'
import { Toaster } from 'sonner'
import { AuthProvider, useAuth } from '@/hooks/use-auth'
import { Layout } from '@/components/layout'
import Login from '@/pages/login'
import Dashboard from '@/pages/dashboard'
import ActionsPage from '@/pages/actions'
import ActionDetail from '@/pages/action-detail'
import Agents from '@/pages/agents'
import Policies from '@/pages/policies'
import Verdicts from '@/pages/verdicts'
import AuditLog from '@/pages/audit-log'

function ProtectedRoute({ children }: { children: React.ReactNode }) {
  const { authenticated, loading } = useAuth()
  if (loading) {
    return (
      <div className="flex h-screen items-center justify-center">
        <div className="h-8 w-8 animate-spin rounded-full border-4 border-primary border-t-transparent" />
      </div>
    )
  }
  if (!authenticated) return <Navigate to="/admin/login" replace />
  return <>{children}</>
}

export default function App() {
  return (
    <BrowserRouter>
      <AuthProvider>
        <Routes>
          <Route path="/admin/login" element={<Login />} />
          <Route
            path="/admin"
            element={
              <ProtectedRoute>
                <Layout />
              </ProtectedRoute>
            }
          >
            <Route index element={<Dashboard />} />
            <Route path="actions" element={<ActionsPage />} />
            <Route path="actions/:id" element={<ActionDetail />} />
            <Route path="agents" element={<Agents />} />
            <Route path="policies" element={<Policies />} />
            <Route path="verdicts" element={<Verdicts />} />
            <Route path="audit" element={<AuditLog />} />
          </Route>
          <Route path="*" element={<Navigate to="/admin" replace />} />
        </Routes>
        <Toaster
          position="bottom-right"
          richColors
          closeButton
          theme="dark"
        />
      </AuthProvider>
    </BrowserRouter>
  )
}
