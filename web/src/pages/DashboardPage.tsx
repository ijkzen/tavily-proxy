import { useCallback, useEffect, useState } from 'react'
import { KeyRound, PackageOpen, RefreshCw, TriangleAlert } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Layout } from '@/components/Layout'
import LogDetailDialog, { type LogDetail } from '@/components/LogDetailDialog'
import { api } from '@/lib/api'
import {
  KIND_LABEL,
  STATUS_LABEL,
  STATUS_VARIANT,
  fmtUsage,
  remaining,
  type UpstreamKey,
} from '@/lib/upstream-keys'
import { cn } from '@/lib/utils'

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
  params_json: string | null
  response_json: string | null
  upstream_key_id: number | null
  upstream_key_nickname: string | null
  upstream_key_kind: 'tavily' | 'exa' | null
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
const PAGE_SIZES = [10, 20, 30, 40, 50]

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
  const [filterKind, setFilterKind] = useState('')
  const [filterSuccess, setFilterSuccess] = useState('')
  const [page, setPage] = useState(1)
  // 分页大小持久化到 localStorage：刷新后保持用户选择（10~50 步进 10，非法值回退 20）
  const [pageSize, setPageSize] = useState(() => {
    const saved = Number(localStorage.getItem('logPageSize'))
    return PAGE_SIZES.includes(saved) ? saved : 20
  })
  const [detail, setDetail] = useState<LogDetail | null>(null)

  const refresh = useCallback(async () => {
    const params = new URLSearchParams()
    if (filterProxyKey) params.set('proxy_key_id', filterProxyKey)
    if (filterTool) params.set('tool', filterTool)
    if (filterKind) params.set('kind', filterKind)
    if (filterSuccess) params.set('success', filterSuccess)
    params.set('limit', String(pageSize))
    params.set('offset', String((page - 1) * pageSize))
    const qs = params.size > 0 ? `?${params.toString()}` : ''

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
  }, [filterProxyKey, filterTool, filterKind, filterSuccess, page, pageSize])

  useEffect(() => {
    void Promise.resolve().then(refresh)
    const timer = setInterval(() => void Promise.resolve().then(refresh), 15000)
    return () => clearInterval(timer)
  }, [refresh])

  const totalPages = Math.max(1, Math.ceil(logs.total / pageSize))

  const selectClass =
    'h-9 rounded-md border border-input bg-card px-2 text-sm shadow-sm focus:outline-none focus:ring-1 focus:ring-ring'

  const activeCount = upstreamKeys.filter((k) => k.status === 'active').length
  const coolingCount = upstreamKeys.filter((k) => k.status === 'cooling').length
  const exhaustedCount = upstreamKeys.filter((k) => k.status === 'exhausted').length

  // 按提供商分组，组内按剩余额度降序排名（tavily credits / exa 美元）
  const groups: { kind: 'tavily' | 'exa'; label: string; unit: string; keys: UpstreamKey[] }[] = [
    { kind: 'tavily', label: 'Tavily', unit: 'credits', keys: upstreamKeys.filter((k) => k.kind === 'tavily') },
    { kind: 'exa', label: 'Exa', unit: '美元', keys: upstreamKeys.filter((k) => k.kind === 'exa') },
  ]
  for (const g of groups) g.keys.sort((a, b) => remaining(b) - remaining(a))

  return (
    <Layout title="看板">
      {/* 密钥池签名卡：按提供商分组 + 组内剩余额度排名 */}
      <Card className="mb-6">
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle className="flex items-center gap-2">
              <KeyRound className="size-4 text-primary" />
              密钥池
            </CardTitle>
            <span className="text-xs text-muted-foreground tabular-nums">
              {upstreamKeys.length === 0
                ? '未配置'
                : `${activeCount} 正常 · ${coolingCount} 冷却 · ${exhaustedCount} 耗尽`}
            </span>
          </div>
        </CardHeader>
        <CardContent className="space-y-6">
          {groups.map((g) => (
            <div key={g.kind} className="space-y-1.5">
              <p className="text-xs font-medium text-muted-foreground">
                {g.label} · 剩余{g.unit}排名
              </p>
              {g.keys.map((k) => {
                const pct = k.limit ? Math.min(100, (k.usage / k.limit) * 100) : null
                return (
                  <div key={k.id} className="space-y-1.5">
                    <div className="flex items-center justify-between gap-4 text-sm">
                      <div className="flex items-center gap-2">
                        <Badge variant="outline" className={k.kind === 'exa' ? 'border-violet-500/40 text-violet-600 dark:text-violet-400' : ''}>
                          {KIND_LABEL[k.kind]}
                        </Badge>
                        <span className="font-medium">{k.nickname}</span>
                        <Badge variant={STATUS_VARIANT[k.status]}>{STATUS_LABEL[k.status]}</Badge>
                      </div>
                      <span className="font-mono text-xs text-muted-foreground tabular-nums whitespace-nowrap">
                        {k.limit === null
                          ? `${fmtUsage(k.usage)} / 未知`
                          : `${fmtUsage(k.usage)} / ${fmtUsage(k.limit)}${k.kind === 'exa' ? ' 美元' : ''}`}
                        <span className="mx-1 text-border">·</span>
                        {k.kind === 'exa' ? '每月 1 日重置' : `每月 ${k.reset_day} 日重置`}
                      </span>
                    </div>
                    <div className="h-2 rounded-full bg-muted overflow-hidden">
                      <div
                        className={cn(
                          'h-full rounded-full transition-all',
                          pct !== null && pct > 90
                            ? 'bg-red-500'
                            : pct !== null && pct > 70
                              ? 'bg-amber-500'
                              : 'bg-emerald-500'
                        )}
                        style={{ width: `${pct ?? 0}%` }}
                      />
                    </div>
                  </div>
                )
              })}
              {g.keys.length === 0 && (
                <p className="text-sm text-muted-foreground py-1">还没有 {g.label} 密钥</p>
              )}
            </div>
          ))}
          {upstreamKeys.length === 0 && (
            <div className="flex flex-col items-center gap-2 py-6 text-muted-foreground">
              <KeyRound className="size-6" />
              <p className="text-sm">还没有上游密钥，去「上游密钥」页添加第一个</p>
            </div>
          )}
        </CardContent>
      </Card>

      {/* 统计数据带 */}
      <Card className="mb-6">
        <CardContent className="grid grid-cols-2 gap-y-6 sm:grid-cols-3 lg:grid-cols-5 px-0 py-0">
          <StatCell label="总请求（30 天）" value={stats?.total ?? '—'} />
          <StatCell label="成功率" value={stats ? `${(stats.success_rate * 100).toFixed(1)}%` : '—'} />
          <StatCell label="平均延迟" value={stats ? `${Math.round(stats.avg_duration_ms)} ms` : '—'} />
          <StatCell label="p95 延迟" value={stats ? `${stats.p95_duration_ms} ms` : '—'} />
          <StatCell label="总消耗 credits" value={stats?.total_credits ?? '—'} />
        </CardContent>
      </Card>

      <div className={cn('grid gap-4 mb-6', alerts.length > 0 && 'lg:grid-cols-2')}>
        {/* 告警：401 自动禁用、额度轮询失败等 */}
        {alerts.length > 0 && (
          <Card className="border-amber-200 bg-amber-50 dark:border-amber-500/30 dark:bg-amber-500/10">
            <CardHeader>
              <CardTitle className="flex items-center gap-2 text-amber-800 dark:text-amber-300">
                <TriangleAlert className="size-4" />
                告警（{alerts.length}）
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-2">
              {alerts.slice(0, 5).map((a) => (
                <div key={a.id} className="flex items-baseline gap-3 text-sm">
                  <span className="shrink-0 text-amber-700/70 dark:text-amber-300/70 tabular-nums">
                    {fmtTime(a.created_at)}
                  </span>
                  <span className="shrink-0 font-medium text-amber-900 dark:text-amber-200">
                    {upstreamKeys.find((k) => k.id === a.upstream_key_id)?.nickname ?? '系统'}
                  </span>
                  <span className="text-amber-800/90 dark:text-amber-200/90">{a.message}</span>
                </div>
              ))}
              {alerts.length > 5 && (
                <p className="text-sm text-amber-700/70 dark:text-amber-300/70">
                  … 其余 {alerts.length - 5} 条略
                </p>
              )}
            </CardContent>
          </Card>
        )}

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
                    <TableCell className="tabular-nums">{k.total_credits}</TableCell>
                    <TableCell className="text-muted-foreground tabular-nums">{fmtTime(k.last_used_at)}</TableCell>
                  </TableRow>
                ))}
                {proxyKeys.length === 0 && (
                  <TableRow>
                    <TableCell colSpan={3} className="text-center text-muted-foreground py-6">
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
            <Button variant="outline" size="sm" onClick={refresh}>
              <RefreshCw className="size-3.5" />
              刷新
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          <div className="flex flex-wrap gap-3 mb-4">
            <select
              className={selectClass}
              value={filterProxyKey}
              onChange={(e) => {
                setFilterProxyKey(e.target.value)
                setPage(1)
              }}
            >
              <option value="">全部代理密钥</option>
              {proxyKeys.map((k) => (
                <option key={k.id} value={k.id}>{k.name}</option>
              ))}
            </select>
            <select
              className={selectClass}
              value={filterTool}
              onChange={(e) => {
                setFilterTool(e.target.value)
                setPage(1)
              }}
            >
              <option value="">全部工具</option>
              {TOOLS.map((t) => (
                <option key={t} value={t}>{t}</option>
              ))}
            </select>
            <select
              className={selectClass}
              value={filterKind}
              onChange={(e) => {
                setFilterKind(e.target.value)
                setPage(1)
              }}
            >
              <option value="">全部提供商</option>
              <option value="tavily">Tavily</option>
              <option value="exa">Exa</option>
            </select>
            <select
              className={selectClass}
              value={filterSuccess}
              onChange={(e) => {
                setFilterSuccess(e.target.value)
                setPage(1)
              }}
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
                <TableRow
                  key={l.id}
                  className={cn(
                    'cursor-pointer',
                    !l.success && 'bg-red-500/[0.04] dark:bg-red-500/10',
                    'hover:bg-muted/60'
                  )}
                  onClick={() =>
                    setDetail({
                      tool: l.tool,
                      upstream_key_kind: l.upstream_key_kind,
                      params_json: l.params_json,
                      response_json: l.response_json,
                      credits: l.credits,
                      success: l.success,
                      error: l.error,
                    })
                  }
                >
                  <TableCell className="whitespace-nowrap font-mono text-xs text-muted-foreground tabular-nums">
                    {fmtTime(l.created_at)}
                  </TableCell>
                  <TableCell>{l.proxy_key_name ?? '—'}</TableCell>
                  <TableCell className="font-mono text-xs">{l.tool}</TableCell>
                  <TableCell>
                    {l.upstream_key_nickname ?? '—'}
                    {l.upstream_key_kind && (
                      <span className="ml-1.5 font-mono text-xs text-muted-foreground">
                        {l.upstream_key_kind === 'exa' ? 'exa' : 'tav'}
                      </span>
                    )}
                  </TableCell>
                  <TableCell>
                    {l.success ? (
                      <Badge
                        variant="outline"
                        className="border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400"
                      >
                        成功
                      </Badge>
                    ) : (
                      <span title={l.error ?? ''}>
                        <Badge variant="destructive">失败</Badge>
                      </span>
                    )}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">{l.credits}</TableCell>
                  <TableCell className="text-right text-muted-foreground tabular-nums">{l.duration_ms} ms</TableCell>
                  <TableCell
                    className="max-w-48 truncate text-muted-foreground"
                    title={l.params_summary ?? ''}
                  >
                    {l.params_summary ?? '—'}
                  </TableCell>
                </TableRow>
              ))}
              {logs.items.length === 0 && (
                <TableRow>
                  <TableCell colSpan={8} className="text-center text-muted-foreground py-10">
                    <PackageOpen className="mx-auto size-6 mb-2 opacity-60" />
                    暂无日志，发起一次搜索后这里会显示记录
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>

          {/* 分页：每页条数 10~50（步进 10）+ 页码 */}
          <div className="flex items-center justify-between mt-4">
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <span>每页</span>
              <select
                className={selectClass}
                value={pageSize}
                onChange={(e) => {
                  const n = Number(e.target.value)
                  localStorage.setItem('logPageSize', String(n))
                  setPageSize(n)
                }}
              >
                {PAGE_SIZES.map((n) => (
                  <option key={n} value={n}>{n}</option>
                ))}
              </select>
              <span>条</span>
            </div>
            <div className="flex items-center gap-1">
              <Button
                variant="outline"
                size="sm"
                disabled={page <= 1}
                onClick={() => setPage((p) => p - 1)}
              >
                上一页
              </Button>
              <span className="px-2 text-sm tabular-nums text-muted-foreground">
                {page} / {totalPages}
              </span>
              <Button
                variant="outline"
                size="sm"
                disabled={page >= totalPages}
                onClick={() => setPage((p) => p + 1)}
              >
                下一页
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* 请求明细弹窗 */}
      <LogDetailDialog log={detail} onClose={() => setDetail(null)} />
    </Layout>
  )
}

function StatCell({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="px-4 py-5">
      <p className="text-xs text-muted-foreground">{label}</p>
      <p className="mt-1 text-2xl font-semibold tabular-nums">{value}</p>
    </div>
  )
}
