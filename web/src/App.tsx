import { useEffect, useState } from 'react'
import { Navigate, Route, Routes, useLocation, useNavigate } from 'react-router-dom'
import { api } from '@/lib/api'
import SetupPage from '@/pages/SetupPage'
import LoginPage from '@/pages/LoginPage'
import DashboardPage from '@/pages/DashboardPage'
import SettingsPage from '@/pages/SettingsPage'

/** 登录门卫：无账号 → /setup；未登录 → /login；否则放行。 */
function AuthGate({ children }: { children: React.ReactNode }) {
  const [verdict, setVerdict] = useState<'loading' | 'setup' | 'login' | 'ok'>('loading')
  const location = useLocation()

  useEffect(() => {
    let cancelled = false
    ;(async () => {
      const setup = await api<{ needs_setup: boolean }>('/api/setup/status')
      if (setup.data?.needs_setup) {
        if (!cancelled) setVerdict('setup')
        return
      }
      const me = await api('/api/me')
      if (!cancelled) setVerdict(me.ok ? 'ok' : 'login')
    })()
    return () => {
      cancelled = true
    }
  }, [location.pathname])

  if (verdict === 'loading') return null
  if (verdict === 'setup') return <Navigate to="/setup" replace />
  if (verdict === 'login') return <Navigate to="/login" replace />
  return children
}

function LoggedOutOnly({ children }: { children: React.ReactNode }) {
  const navigate = useNavigate()
  const [checked, setChecked] = useState(false)

  useEffect(() => {
    ;(async () => {
      const me = await api('/api/me')
      if (me.ok) navigate('/', { replace: true })
      else setChecked(true)
    })()
  }, [navigate])

  return checked ? children : null
}

export default function App() {
  return (
    <Routes>
      <Route path="/setup" element={<LoggedOutOnly><SetupPage /></LoggedOutOnly>} />
      <Route path="/login" element={<LoggedOutOnly><LoginPage /></LoggedOutOnly>} />
      <Route path="/" element={<AuthGate><DashboardPage /></AuthGate>} />
      <Route path="/settings" element={<AuthGate><SettingsPage /></AuthGate>} />
    </Routes>
  )
}
