import { Link, Outlet, useNavigate } from 'react-router-dom'
import { Button } from '@/components/ui/button'
import { api } from '@/lib/api'

/** 登录后的通用布局：顶部导航 + 内容区。 */
export function Layout({ title, children }: { title: string; children?: React.ReactNode }) {
  const navigate = useNavigate()

  async function logout() {
    await api('/api/logout', { method: 'POST' })
    navigate('/login', { replace: true })
  }

  return (
    <div className="min-h-screen bg-neutral-50">
      <header className="border-b bg-white">
        <div className="max-w-5xl mx-auto px-4 h-14 flex items-center justify-between">
          <div className="flex items-center gap-6">
            <span className="font-semibold">tavily-proxy</span>
            <nav className="flex gap-4 text-sm text-neutral-600">
              <Link to="/" className="hover:text-neutral-900">看板</Link>
              <Link to="/upstream-keys" className="hover:text-neutral-900">上游密钥</Link>
              <Link to="/proxy-keys" className="hover:text-neutral-900">代理密钥</Link>
              <Link to="/settings" className="hover:text-neutral-900">设置</Link>
            </nav>
          </div>
          <Button variant="outline" size="sm" onClick={logout}>退出登录</Button>
        </div>
      </header>
      <main className="max-w-5xl mx-auto px-4 py-8">
        <h1 className="text-xl font-semibold mb-6">{title}</h1>
        {children ?? <Outlet />}
      </main>
    </div>
  )
}
