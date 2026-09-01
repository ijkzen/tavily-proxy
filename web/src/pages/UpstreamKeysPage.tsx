import { useEffect, useState } from 'react'
import { KeyRound, Plus } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Layout } from '@/components/Layout'
import { SecretCell } from '@/components/Secret'
import { api } from '@/lib/api'
import { STATUS_LABEL, STATUS_VARIANT, type UpstreamKey } from '@/lib/upstream-keys'

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
          <CardDescription>Tavily 官方 API key（tvly-…），添加后参与负载均衡。密钥加密存储，可随时点眼睛查看明文或一键复制。</CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={add} className="grid gap-4 sm:grid-cols-[1fr_1fr_120px_auto] items-end">
            <div className="space-y-2">
              <Label htmlFor="key">密钥</Label>
              <Input id="key" type="password" value={newKey} onChange={(e) => setNewKey(e.target.value)} placeholder="tvly-…" className="font-mono" />
            </div>
            <div className="space-y-2">
              <Label htmlFor="nickname">昵称</Label>
              <Input id="nickname" value={nickname} onChange={(e) => setNickname(e.target.value)} placeholder="例如：主力账号" />
            </div>
            <div className="space-y-2">
              <Label htmlFor="reset_day">重置日</Label>
              <Input id="reset_day" type="number" min={1} max={28} value={resetDay} onChange={(e) => setResetDay(e.target.value)} />
            </div>
            <Button type="submit" disabled={busy}>
              <Plus className="size-4" />
              {busy ? '添加中…' : '添加'}
            </Button>
          </form>
          {error && <p className="text-sm text-red-600 dark:text-red-400 mt-3">{error}</p>}
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
                  <TableCell>
                    <SecretCell revealPath={`/api/upstream-keys/${k.id}/reveal`} masked={`tvly-••••${k.key_tail}`} />
                  </TableCell>
                  <TableCell>
                    <Badge variant={STATUS_VARIANT[k.status]}>{STATUS_LABEL[k.status]}</Badge>
                  </TableCell>
                  <TableCell className="tabular-nums">{k.limit === null ? `${k.usage} / 未知` : `${k.usage} / ${k.limit}`}</TableCell>
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
                  <TableCell colSpan={5} className="text-center text-muted-foreground py-8">
                    <KeyRound className="mx-auto size-6 mb-2 opacity-60" />
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
