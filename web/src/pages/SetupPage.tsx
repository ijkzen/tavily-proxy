import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { api } from '@/lib/api'

export default function SetupPage() {
  const navigate = useNavigate()
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)

  async function submit(e: React.FormEvent) {
    e.preventDefault()
    setBusy(true)
    setError('')
    const resp = await api('/api/setup', { method: 'POST', json: { username, password } })
    if (resp.ok) {
      // 创建成功后自动登录
      const login = await api('/api/login', { method: 'POST', json: { username, password } })
      if (login.ok) {
        navigate('/', { replace: true })
        return
      }
      navigate('/login', { replace: true })
      return
    }
    setError(resp.status === 403 ? '账号已存在，请直接登录' : '创建失败：用户名不能为空，密码至少 8 位')
    setBusy(false)
  }

  return (
    <div className="min-h-screen flex items-center justify-center bg-neutral-50 p-4">
      <Card className="w-full max-w-sm">
        <CardHeader>
          <CardTitle>欢迎使用 tavily-proxy</CardTitle>
          <CardDescription>首次使用，请创建你的账号。账号创建后本页将永久关闭。</CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={submit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="username">用户名</Label>
              <Input id="username" value={username} onChange={(e) => setUsername(e.target.value)} autoComplete="username" />
            </div>
            <div className="space-y-2">
              <Label htmlFor="password">密码</Label>
              <Input id="password" type="password" value={password} onChange={(e) => setPassword(e.target.value)} autoComplete="new-password" />
              <p className="text-xs text-neutral-500">至少 8 位</p>
            </div>
            {error && <p className="text-sm text-red-600">{error}</p>}
            <Button type="submit" className="w-full" disabled={busy}>
              {busy ? '创建中…' : '创建账号'}
            </Button>
          </form>
        </CardContent>
      </Card>
    </div>
  )
}
