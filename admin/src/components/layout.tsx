import { Outlet, Link, useLocation, useNavigate } from 'react-router-dom'
import { motion } from 'motion/react'
import {
  LayoutDashboard,
  Shield,
  Users,
  ScrollText,
  Eye,
  FileText,
  LogOut,
  Menu,
  X,
} from 'lucide-react'
import { Button } from '@/components/ui/button'
import { ThemeToggle } from '@/components/theme-toggle'
import { useAuth } from '@/hooks/use-auth'
import { useState } from 'react'
import { cn } from '@/lib/utils'

const navItems = [
  { path: '/admin', label: 'Dashboard', icon: LayoutDashboard },
  { path: '/admin/actions', label: 'Actions', icon: Shield },
  { path: '/admin/agents', label: 'Agents', icon: Users },
  { path: '/admin/policies', label: 'Policies', icon: FileText },
  { path: '/admin/verdicts', label: 'Verdicts', icon: Eye },
  { path: '/admin/audit', label: 'Audit Log', icon: ScrollText },
]

function isActive(pathname: string, navPath: string) {
  if (navPath === '/admin') return pathname === '/admin'
  return pathname.startsWith(navPath)
}

export function Layout() {
  const { pathname } = useLocation()
  const { logout } = useAuth()
  const navigate = useNavigate()
  const [sidebarOpen, setSidebarOpen] = useState(false)

  const handleLogout = async () => {
    try {
      await logout()
    } finally {
      navigate('/admin/login')
    }
  }

  return (
    <div className="flex h-screen overflow-hidden bg-background">
      {sidebarOpen && (
        <div
          className="fixed inset-0 z-40 bg-black/50 lg:hidden"
          onClick={() => setSidebarOpen(false)}
        />
      )}

      <aside
        className={cn(
          'fixed inset-y-0 left-0 z-50 flex w-64 flex-col border-r bg-sidebar transition-transform lg:static lg:translate-x-0',
          sidebarOpen ? 'translate-x-0' : '-translate-x-full'
        )}
      >
        <Link
          to="/admin"
          onClick={() => setSidebarOpen(false)}
          className="flex h-14 items-center gap-2 border-b px-6 hover:opacity-80"
        >
          <Shield className="h-5 w-5 text-sidebar-foreground" />
          <span className="font-semibold text-sidebar-foreground">Deko Admin</span>
        </Link>

        <nav className="flex-1 space-y-1 p-3">
          {navItems.map(({ path, label, icon: Icon }) => {
            const active = isActive(pathname, path)
            return (
              <Link
                key={path}
                to={path}
                onClick={() => setSidebarOpen(false)}
                className={cn(
                  'flex items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium transition-colors',
                  active
                    ? 'bg-sidebar-accent text-sidebar-accent-foreground'
                    : 'text-sidebar-muted hover:bg-sidebar-accent hover:text-sidebar-accent-foreground'
                )}
              >
                <Icon className="h-4 w-4" />
                {label}
              </Link>
            )
          })}
        </nav>

        <div className="border-t p-3">
          <Button
            variant="ghost"
            className="w-full justify-start gap-3 text-sidebar-muted hover:text-sidebar-accent-foreground"
            onClick={handleLogout}
          >
            <LogOut className="h-4 w-4" />
            Logout
          </Button>
        </div>
      </aside>

      <div className="flex flex-1 flex-col overflow-hidden">
        <header className="flex h-14 items-center gap-4 border-b bg-background px-4 lg:px-6">
          <Button
            variant="ghost"
            size="icon"
            className="lg:hidden"
            onClick={() => setSidebarOpen(true)}
          >
            {sidebarOpen ? <X className="h-4 w-4" /> : <Menu className="h-4 w-4" />}
          </Button>

          <div className="flex-1" />

          <ThemeToggle />
        </header>

        <main className="flex-1 overflow-auto p-4 lg:p-6">
          <motion.div
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.2 }}
          >
            <Outlet />
          </motion.div>
        </main>
      </div>
    </div>
  )
}
