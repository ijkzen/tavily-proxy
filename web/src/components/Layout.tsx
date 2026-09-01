import { Link, NavLink, Outlet, useNavigate } from 'react-router-dom'
import { KeyRound, LayoutDashboard, LogOut, Network, Settings, ShieldCheck } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { ThemeToggle } from '@/components/ThemeToggle'
import { api } from '@/lib/api'
import { cn } from '@/lib/utils'

const NAV = [
  { to: '/', label: '看板', icon: LayoutDashboard },
  { to: '/upstream-keys', label: '上游密钥', icon: KeyRound },
  { to: '/proxy-keys', label: '代理密钥', icon: Network },
  { to: '/settings', label: '设置', icon: Settings },
]

/** 登录后的通用布局：侧边栏（跟随主题，浅色浅底/暗色深底）+ 页眉，移动端折叠为顶栏。 */
export function Layout({ title, children }: { title: string; children?: React.ReactNode }) {
  const navigate = useNavigate()

  async function logout() {
    await api('/api/logout', { method: 'POST' })
    navigate('/login', { replace: true })
  }

  return (
    <div className="min-h-screen lg:flex">
      {/* 侧边栏（移动端顶栏） */}
      <aside className="flex flex-col bg-sidebar text-sidebar-foreground lg:min-h-screen lg:w-56 lg:fixed lg:inset-y-0 lg:border-r lg:border-sidebar-border">
        <div className="flex items-center justify-between px-5 h-14 border-b border-sidebar-border">
          <Link to="/" className="flex items-center gap-2 font-semibold">
            <ShieldCheck className="size-5 text-sidebar-primary" />
            <span>tavily-proxy</span>
          </Link>
          <ThemeToggle className="lg:hidden text-sidebar-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground" />
        </div>
        <nav className="flex gap-1 px-3 py-2 lg:flex-col lg:flex-1">
          {NAV.map(({ to, label, icon: Icon }) => (
            <NavLink
              key={to}
              to={to}
              end={to === '/'}
              className={({ isActive }) =>
                cn(
                  'flex items-center gap-2.5 rounded-md px-3 py-2 text-sm transition-colors',
                  isActive
                    ? 'bg-sidebar-primary text-sidebar-primary-foreground'
                    : 'text-sidebar-foreground/80 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground'
                )
              }
            >
              <Icon className="size-4" />
              <span>{label}</span>
            </NavLink>
          ))}
        </nav>
        <div className="hidden lg:flex flex-col gap-1 p-3 border-t border-sidebar-border">
          <ThemeToggle className="text-sidebar-foreground/80 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground justify-start" />
          <Button
            variant="ghost"
            className="justify-start text-sidebar-foreground/80 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
            onClick={logout}
          >
            <LogOut className="size-4" />
            退出登录
          </Button>
        </div>
      </aside>

      {/* 内容区 */}
      <div className="flex-1 lg:ml-56 flex flex-col min-w-0">
        <header className="h-14 border-b bg-card flex items-center justify-between px-4 lg:px-8">
          <h1 className="text-lg font-semibold">{title}</h1>
          <div className="flex items-center gap-3 text-sm text-muted-foreground">
            <span className="flex items-center gap-1.5">
              <span className="size-2 rounded-full bg-emerald-500" />
              服务在线
            </span>
            <Button variant="outline" size="sm" className="lg:hidden" onClick={logout}>
              <LogOut className="size-3.5" />
              退出
            </Button>
          </div>
        </header>
        <main className="flex-1 px-4 lg:px-8 py-6 max-w-6xl w-full">{children ?? <Outlet />}</main>
      </div>
    </div>
  )
}
