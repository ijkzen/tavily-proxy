import { useCallback, useEffect, useState } from 'react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Layout } from '@/components/Layout'
import { api } from '@/lib/api'
import { STATUS_LABEL, STATUS_VARIANT, type UpstreamKey } from '@/lib/upstream-keys'

interface ProxyKey {
  id: number
  name: string
  total_credits: number
  last_used_at: number | null
}

interface Stats {
  total: number
  successes: number
  success_rate: number
  avg_duration_ms: number
  p95_duration_ms: number
  total_credits: number
}

interface LogEntry {
  id: number
  proxy_key_id: number | null
  proxy_key_name: string | null
  tool: string
  params_summary: string | null
  upstream_key_id: number | null
  upstream_key_nickname: string | null
  credits: number
  duration_ms: number
  success: boolean
  error: string | null
  created_at: number
}

interface LogsResponse {
  total: number
  items: LogEntry[]
}

interface Alert {
  id: number
  upstream_key_id: number | null
  kind: string
  message: string
  created_at: number
}

const TOOLS = ['tavily_search', 'tavily_extract']

function fmtTime(unixSecs: number | null): string {
  if (!unixSecs) return '—'
  return new Date(unixSecs * 1000).toLocaleString('zh-CN', { hour12: false })
}

export default function DashboardPage() {
  const [upstreamKeys, setUpstreamKeys] = useState<UpstreamKey[]>([])
  const [proxyKeys, setProxyKeys] = useState<ProxyKey[]>([])
  const [stats, setStats] = useState<Stats | null>(null)
  const [alerts, setAlerts] = useState<Alert[]>([])
  const [logs, setLogs] = useState<LogsResponse>({ total: 0, items: [] })
  const [filterProxyKey, setFilterProxyKey] = useState('')
  const [filterTool, setFilterTool] = useState('')
  const [filterSuccess, setFilterSuccess] = useState('')

  const refresh = useCallback(async () => {
    const params = new URLSearchParams()
    if (filterProxyKey) params.set('proxy_key_id', filterProxyKey)
    if (filterTool) params.set('tool', filterTool)
    if (filterSuccess) params.set('success', filterSuccess)
    const qs = params.size > 0 ? `?${params}` : ''

    const [uk, pk, st, lg, al] = await Promise.all([
      api<UpstreamKey[]>('/api/upstream-keys'),
      api<ProxyKey[]>('/api/proxy-keys'),
      api<Stats>('/api/stats'),
      api<LogsResponse>(`/api/logs${qs}`),
      api<Alert[]>('/api/alerts'),
    ])
    if (uk.ok && uk.data) setUpstreamKeys(uk.data)
    if (pk.ok && pk.data) setProxyKeys(pk.data)
    if (st.ok && st.data) setStats(st.data)
    if (lg.ok && lg.data) setLogs(lg.data)
    if (al.ok && al.data) setAlerts(al.data)
  }, [filterProxyKey, filterTool, filterSuccess])

  useEffect(() => {
    refresh()
    const timer = setInterval(refresh, 15000)
    return () => clearInterval(timer)
  }, [refresh])

  const selectClass =
    'h-9 rounded-md border border-neutral-300 bg-white px-2 text-sm shadow-sm focus:outline-none focus:ring-1 focus:ring-neutral-400'

  return (
    <Layout title="看板">
      {/* 聚合统计 */}
      <div className="grid grid-cols-2 gap-4 sm:grid-cols-5 mb-6">
        <StatCard label="总请求（30 天）" value={stats?.total ?? '—'} />
        <StatCard
          label="成功率"
          value={stats ? `${(stats.success_rate * 100).toFixed(1)}%` : '—'}
        />
        <StatCard
          label="平均延迟"
          value={stats ? `${Math.round(stats.avg_duration_ms)} ms` : '—'}
        />
        <StatCard label="p95 延迟" value={stats ? `${stats.p95_duration_ms} ms` : '—'} />
        <StatCard label="总消耗 credits" value={stats?.total_credits ?? '—'} />
      </div>

      {/* 告警：401 自动禁用、额度轮询失败等 */}
      {alerts.length > 0 && (
        <Card className="mb-6 border-amber-200 bg-amber-50">
          <CardHeader>
            <CardTitle className="text-amber-800">告警（{alerts.length}）</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2">
            {alerts.slice(0, 5).map((a) => (
              <div key={a.id} className="flex items-baseline gap-3 text-sm">
                <span className="shrink-0 text-neutral-500">{fmtTime(a.created_at)}</span>
                <span className="shrink-0 font-medium">
                  {upstreamKeys.find((k) => k.id === a.upstream_key_id)?.nickname ?? '系统'}
                </span>
                <span className="text-neutral-700">{a.message}</span>
              </div>
            ))}
            {alerts.length > 5 && (
              <p className="text-sm text-neutral-500">… 其余 {alerts.length - 5} 条略</p>
            )}
          </CardContent>
        </Card>
      )}

      <div className="grid gap-4 lg:grid-cols-2 mb-6">
        {/* 上游密钥用量 */}
        <Card>
          <CardHeader>
            <CardTitle>上游密钥用量</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            {upstreamKeys.map((k) => {
              const pct = k.limit ? Math.min(100, (k.usage / k.limit) * 100) : null
              return (
                <div key={k.id} className="space-y-1">
                  <div className="flex items-center justify-between text-sm">
                    <span className="font-medium">{k.nickname}</span>
                    <span className="flex items-center gap-2">
                      <span className="text-neutral-500">
                        {k.limit === null ? `${k.usage} / 未知` : `${k.usage} / ${k.limit}`}
                      </span>
                      <Badge variant={STATUS_VARIANT[k.status]}>{STATUS_LABEL[k.status]}</Badge>
                    </span>
                  </div>
                  <div className="h-2 rounded-full bg-neutral-100 overflow-hidden">
                    <div
                      className={`h-full rounded-full ${
                        pct !== null && pct > 90
                          ? 'bg-red-500'
                          : pct !== null && pct > 70
                            ? 'bg-amber-500'
                            : 'bg-emerald-500'
                      }`}
                      style={{ width: `${pct ?? 0}%` }}
                    />
                  </div>
                </div>
              )
            })}
            {upstreamKeys.length === 0 && (
              <p className="text-sm text-neutral-500">还没有上游密钥</p>
            )}
          </CardContent>
        </Card>

        {/* 代理密钥用量 */}
        <Card>
          <CardHeader>
            <CardTitle>代理密钥用量</CardTitle>
          </CardHeader>
          <CardContent>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>名称</TableHead>
                  <TableHead>累计 credits</TableHead>
                  <TableHead>最近使用</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {proxyKeys.map((k) => (
                  <TableRow key={k.id}>
                    <TableCell>{k.name}</TableCell>
                    <TableCell>{k.total_credits}</TableCell>
                    <TableCell className="text-neutral-500">{fmtTime(k.last_used_at)}</TableCell>
                  </TableRow>
                ))}
                {proxyKeys.length === 0 && (
                  <TableRow>
                    <TableCell colSpan={3} className="text-center text-neutral-500 py-6">
                      还没有代理密钥
                    </TableCell>
                  </TableRow>
                )}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      </div>

      {/* 请求日志 */}
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle>请求日志（共 {logs.total} 条）</CardTitle>
            <Button variant="outline" size="sm" onClick={refresh}>刷新</Button>
          </div>
        </CardHeader>
        <CardContent>
          <div className="flex gap-3 mb-4">
            <select
              className={selectClass}
              value={filterProxyKey}
              onChange={(e) => setFilterProxyKey(e.target.value)}
            >
              <option value="">全部代理密钥</option>
              {proxyKeys.map((k) => (
                <option key={k.id} value={k.id}>{k.name}</option>
              ))}
            </select>
            <select
              className={selectClass}
              value={filterTool}
              onChange={(e) => setFilterTool(e.target.value)}
            >
              <option value="">全部工具</option>
              {TOOLS.map((t) => (
                <option key={t} value={t}>{t}</option>
              ))}
            </select>
            <select
              className={selectClass}
              value={filterSuccess}
              onChange={(e) => setFilterSuccess(e.target.value)}
            >
              <option value="">成功 + 失败</option>
              <option value="true">仅成功</option>
              <option value="false">仅失败</option>
            </select>
          </div>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>时间</TableHead>
                <TableHead>代理密钥</TableHead>
                <TableHead>工具</TableHead>
                <TableHead>上游密钥</TableHead>
                <TableHead>结果</TableHead>
                <TableHead className="text-right">credits</TableHead>
                <TableHead className="text-right">耗时</TableHead>
                <TableHead>参数摘要</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {logs.items.map((l) => (
                <TableRow key={l.id}>
                  <TableCell className="whitespace-nowrap text-neutral-500">
                    {fmtTime(l.created_at)}
                  </TableCell>
                  <TableCell>{l.proxy_key_name ?? '—'}</TableCell>
                  <TableCell className="font-mono text-xs">{l.tool}</TableCell>
                  <TableCell>{l.upstream_key_nickname ?? '—'}</TableCell>
                  <TableCell>
                    {l.success ? (
                      <Badge variant="default">成功</Badge>
                    ) : (
                      <span title={l.error ?? ''}>
                        <Badge variant="destructive">失败</Badge>
                      </span>
                    )}
                  </TableCell>
                  <TableCell className="text-right">{l.credits}</TableCell>
                  <TableCell className="text-right text-neutral-500">{l.duration_ms} ms</TableCell>
                  <TableCell
                    className="max-w-48 truncate text-neutral-500"
                    title={l.params_summary ?? ''}
                  >
                    {l.params_summary ?? '—'}
                  </TableCell>
                </TableRow>
              ))}
              {logs.items.length === 0 && (
                <TableRow>
                  <TableCell colSpan={8} className="text-center text-neutral-500 py-8">
                    暂无日志
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

function StatCard({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <Card>
      <CardContent className="pt-6">
        <p className="text-sm text-neutral-500">{label}</p>
        <p className="text-2xl font-semibold mt-1">{value}</p>
      </CardContent>
    </Card>
  )
}
