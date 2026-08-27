import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Layout } from '@/components/Layout'
import { api } from '@/lib/api'

export default function SettingsPage() {
  const navigate = useNavigate()
  const [currentPassword, setCurrentPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [message, setMessage] = useState('')
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)

  async function submit(e: React.FormEvent) {
    e.preventDefault()
    setBusy(true)
    setError('')
    setMessage('')
    const resp = await api('/api/password', {
      method: 'POST',
      json: { current_password: currentPassword, new_password: newPassword },
    })
    if (resp.ok) {
      // 改密后所有 session 失效，回到登录页
      navigate('/login', { replace: true })
      return
    }
    setError(resp.status === 403 ? '当前密码不正确' : '修改失败：新密码至少 8 位')
    setBusy(false)
  }

  return (
    <Layout title="设置">
      <Card className="max-w-md">
        <CardHeader>
          <CardTitle>修改密码</CardTitle>
          <CardDescription>修改成功后需要重新登录</CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={submit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="current">当前密码</Label>
              <Input id="current" type="password" value={currentPassword} onChange={(e) => setCurrentPassword(e.target.value)} autoComplete="current-password" />
            </div>
            <div className="space-y-2">
              <Label htmlFor="new">新密码</Label>
              <Input id="new" type="password" value={newPassword} onChange={(e) => setNewPassword(e.target.value)} autoComplete="new-password" />
              <p className="text-xs text-neutral-500">至少 8 位</p>
            </div>
            {error && <p className="text-sm text-red-600">{error}</p>}
            {message && <p className="text-sm text-green-600">{message}</p>}
            <Button type="submit" disabled={busy}>{busy ? '提交中…' : '修改密码'}</Button>
          </form>
        </CardContent>
      </Card>
    </Layout>
  )
}
