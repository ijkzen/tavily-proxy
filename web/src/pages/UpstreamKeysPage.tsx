import { useEffect, useState } from 'react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Layout } from '@/components/Layout'
import { api } from '@/lib/api'

interface UpstreamKey {
  id: number
  nickname: string
  key_tail: string
  status: 'active' | 'cooling' | 'exhausted' | 'disabled'
  reset_day: number
  usage: number
  limit: number | null
  created_at: number
}

const STATUS_LABEL: Record<UpstreamKey['status'], string> = {
  active: '正常',
  cooling: '冷却',
  exhausted: '耗尽',
  disabled: '禁用',
}

const STATUS_VARIANT: Record<UpstreamKey['status'], 'default' | 'secondary' | 'destructive' | 'outline'> = {
  active: 'default',
  cooling: 'secondary',
  exhausted: 'destructive',
  disabled: 'outline',
}

export default function UpstreamKeysPage() {
  const [keys, setKeys] = useState<UpstreamKey[]>([])
  const [newKey, setNewKey] = useState('')
  const [nickname, setNickname] = useState('')
  const [resetDay, setResetDay] = useState('1')
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)

  async function refresh() {
    const resp = await api<UpstreamKey[]>('/api/upstream-keys')
    if (resp.ok && resp.data) setKeys(resp.data)
  }

  useEffect(() => {
    refresh()
  }, [])

  async function add(e: React.FormEvent) {
    e.preventDefault()
    setBusy(true)
    setError('')
    const resp = await api('/api/upstream-keys', {
      method: 'POST',
      json: { key: newKey, nickname, reset_day: Number(resetDay) || 1 },
    })
    if (resp.ok) {
      setNewKey('')
      setNickname('')
      await refresh()
    } else {
      setError('添加失败：密钥与昵称不能为空，重置日 1-28')
    }
    setBusy(false)
  }

  async function act(id: number, action: 'disable' | 'enable' | 'delete') {
    const resp =
      action === 'delete'
        ? await api(`/api/upstream-keys/${id}`, { method: 'DELETE' })
        : await api(`/api/upstream-keys/${id}/${action}`, { method: 'POST' })
    if (resp.ok) await refresh()
  }

  return (
    <Layout title="上游密钥池">
      <Card className="mb-6">
        <CardHeader>
          <CardTitle>添加上游密钥</CardTitle>
          <CardDescription>Tavily 官方 API key（tvly-…），添加后参与负载均衡。密钥加密存储，此后只显示尾号。</CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={add} className="grid gap-4 sm:grid-cols-[1fr_1fr_120px_auto] items-end">
            <div className="space-y-2">
              <Label htmlFor="key">密钥</Label>
              <Input id="key" type="password" value={newKey} onChange={(e) => setNewKey(e.target.value)} placeholder="tvly-…" />
            </div>
            <div className="space-y-2">
              <Label htmlFor="nickname">昵称</Label>
              <Input id="nickname" value={nickname} onChange={(e) => setNickname(e.target.value)} placeholder="例如：主力账号" />
            </div>
            <div className="space-y-2">
              <Label htmlFor="reset_day">重置日</Label>
              <Input id="reset_day" type="number" min={1} max={28} value={resetDay} onChange={(e) => setResetDay(e.target.value)} />
            </div>
            <Button type="submit" disabled={busy}>{busy ? '添加中…' : '添加'}</Button>
          </form>
          {error && <p className="text-sm text-red-600 mt-3">{error}</p>}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>密钥列表</CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>昵称</TableHead>
                <TableHead>密钥</TableHead>
                <TableHead>状态</TableHead>
                <TableHead>已用 / 总额度</TableHead>
                <TableHead className="text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {keys.map((k) => (
                <TableRow key={k.id}>
                  <TableCell>{k.nickname}</TableCell>
                  <TableCell className="font-mono text-neutral-500">••••{k.key_tail}</TableCell>
                  <TableCell>
                    <Badge variant={STATUS_VARIANT[k.status]}>{STATUS_LABEL[k.status]}</Badge>
                  </TableCell>
                  <TableCell>{k.limit === null ? `${k.usage} / 未知` : `${k.usage} / ${k.limit}`}</TableCell>
                  <TableCell className="text-right space-x-2">
                    {k.status === 'disabled' ? (
                      <Button variant="outline" size="sm" onClick={() => act(k.id, 'enable')}>启用</Button>
                    ) : (
                      <Button variant="outline" size="sm" onClick={() => act(k.id, 'disable')}>禁用</Button>
                    )}
                    <Button variant="destructive" size="sm" onClick={() => act(k.id, 'delete')}>删除</Button>
                  </TableCell>
                </TableRow>
              ))}
              {keys.length === 0 && (
                <TableRow>
                  <TableCell colSpan={5} className="text-center text-neutral-500 py-8">
                    还没有上游密钥，先添加一个
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </Layout>
  )
}
