import { useEffect, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Layout } from '@/components/Layout'
import { McpLinkLine, SecretLine, useSecret } from '@/components/Secret'
import { api } from '@/lib/api'

interface ProxyKey {
  id: number
  name: string
  key_tail: string
  total_credits: number
  last_used_at: number | null
  created_at: number
}

function formatTime(unix: number | null) {
  if (!unix) return '从未使用'
  return new Date(unix * 1000).toLocaleString('zh-CN')
}

function ProxyKeyRow({ k, onRevoke }: { k: ProxyKey; onRevoke: (id: number) => void }) {
  // 密钥展示与 MCP 链接共享同一份明文（一次 reveal 两个用途）
  const secret = useSecret(`/api/proxy-keys/${k.id}/reveal`)
  return (
    <TableRow>
      <TableCell>{k.name}</TableCell>
      <TableCell className="min-w-72">
        <div className="space-y-0.5">
          <SecretLine secret={secret} masked={`tp-••••${k.key_tail}`} />
          <McpLinkLine secret={secret} />
        </div>
      </TableCell>
      <TableCell>{k.total_credits} credits</TableCell>
      <TableCell className="text-neutral-500">{formatTime(k.last_used_at)}</TableCell>
      <TableCell className="text-right">
        <Button variant="destructive" size="sm" onClick={() => onRevoke(k.id)}>删除</Button>
      </TableCell>
    </TableRow>
  )
}

export default function ProxyKeysPage() {
  const [keys, setKeys] = useState<ProxyKey[]>([])
  const [name, setName] = useState('')
  const [justCreated, setJustCreated] = useState('')
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)

  async function refresh() {
    const resp = await api<ProxyKey[]>('/api/proxy-keys')
    if (resp.ok && resp.data) setKeys(resp.data)
  }

  useEffect(() => {
    refresh()
  }, [])

  async function create(e: React.FormEvent) {
    e.preventDefault()
    setBusy(true)
    setError('')
    setJustCreated('')
    const resp = await api<ProxyKey & { key: string }>('/api/proxy-keys', {
      method: 'POST',
      json: { name },
    })
    if (resp.ok && resp.data) {
      setJustCreated(resp.data.key)
      setName('')
      await refresh()
    } else {
      setError('创建失败：名称不能为空')
    }
    setBusy(false)
  }

  async function revoke(id: number) {
    const resp = await api(`/api/proxy-keys/${id}/revoke`, { method: 'POST' })
    if (resp.ok) await refresh()
  }

  return (
    <Layout title="代理密钥">
      <Card className="mb-6">
        <CardHeader>
          <CardTitle>签发代理密钥</CardTitle>
          <CardDescription>
            MCP 客户端用代理密钥连接本服务（Authorization: Bearer tp-… 或 ?key=tp-…）。
            密钥可随时在下方查看明文或复制 MCP 集成链接。
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={create} className="flex gap-4 items-end">
            <div className="space-y-2 flex-1 max-w-xs">
              <Label htmlFor="name">名称</Label>
              <Input id="name" value={name} onChange={(e) => setName(e.target.value)} placeholder="例如：笔记本 Claude Code" />
            </div>
            <Button type="submit" disabled={busy}>{busy ? '签发中…' : '签发'}</Button>
          </form>
          {error && <p className="text-sm text-red-600 mt-3">{error}</p>}
          {justCreated && (
            <div className="mt-4 rounded-md border border-amber-300 bg-amber-50 p-3">
              <p className="text-sm font-medium text-amber-800 mb-1">新密钥（也可稍后在列表中随时查看）：</p>
              <code className="text-sm break-all select-all">{justCreated}</code>
            </div>
          )}
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
                <TableHead>名称</TableHead>
                <TableHead>密钥 / MCP 链接</TableHead>
                <TableHead>累计用量</TableHead>
                <TableHead>最近使用</TableHead>
                <TableHead className="text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {keys.map((k) => (
                <ProxyKeyRow key={k.id} k={k} onRevoke={revoke} />
              ))}
              {keys.length === 0 && (
                <TableRow>
                  <TableCell colSpan={5} className="text-center text-neutral-500 py-8">
                    还没有代理密钥，签发一个给 MCP 客户端用
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
