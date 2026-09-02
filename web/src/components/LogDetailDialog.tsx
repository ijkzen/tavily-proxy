import { useState } from 'react'
import { Dialog } from '@base-ui/react/dialog'
import { X } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { renderMarkdown } from '@/lib/markdown'

export interface LogDetail {
  tool: string
  upstream_key_kind: 'tavily' | 'exa' | null
  params_json: string | null
  response_json: string | null
  credits: number
  success: boolean
  error: string | null
}

/** 极简 JSON 语法高亮：把 JSON 文本转成带 span 的 HTML。 */
function highlightJson(text: string): string {
  const esc = text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
  return esc.replace(
    /("(?:[^"\\]|\\.)*")(\s*:)?|\b(true|false|null)\b|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)/g,
    (_m, str: string, colon: string | undefined, kw: string | undefined, num: string | undefined) => {
      if (str) {
        return colon
          ? `<span class="text-sky-600 dark:text-sky-400">${str}</span>${colon}`
          : `<span class="text-emerald-600 dark:text-emerald-400">${str}</span>`
      }
      if (kw) return `<span class="text-violet-600 dark:text-violet-400">${kw}</span>`
      if (num) return `<span class="text-amber-600 dark:text-amber-400">${num}</span>`
      return _m
    }
  )
}

/** 从响应 JSON 里提取 Markdown 文本（extract 的 results[].text）。 */
function extractMarkdown(resp: unknown): string | null {
  if (typeof resp !== 'object' || resp === null) return null
  const results = (resp as Record<string, unknown>).results
  if (!Array.isArray(results) || results.length === 0) return null
  const texts = results
    .map((r) => (typeof r === 'object' && r !== null ? (r as Record<string, unknown>).text : null))
    .filter((t): t is string => typeof t === 'string' && t.length > 0)
  return texts.length > 0 ? texts.join('\n\n---\n\n') : null
}

function prettyJson(raw: string | null): string {
  if (!raw) return '—'
  try {
    return JSON.stringify(JSON.parse(raw), null, 2)
  } catch {
    return raw
  }
}

export default function LogDetailDialog({
  log,
  onClose,
}: {
  log: LogDetail | null
  onClose: () => void
}) {
  const [mdView, setMdView] = useState<'raw' | 'render'>('render')
  const [jsonView, setJsonView] = useState<'raw' | 'pretty'>('pretty')

  if (!log) return null

  const isExa = log.upstream_key_kind === 'exa'
  const isExtract = log.tool === 'tavily_extract'
  const isSearch = log.tool === 'tavily_search'
  // 失败时 response_json 为 null，提取不到 Markdown
  const response = log.response_json ? (() => {
    try {
      return JSON.parse(log.response_json)
    } catch {
      return null
    }
  })() : null
  const markdown = isExtract && response ? extractMarkdown(response) : null

  return (
    <Dialog.Root open={!!log} onOpenChange={(open) => !open && onClose()}>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 bg-black/50 z-40" />
        <Dialog.Popup className="fixed left-1/2 top-1/2 z-50 w-[min(920px,95vw)] max-h-[88vh] -translate-x-1/2 -translate-y-1/2 overflow-hidden rounded-xl border border-border bg-card text-card-foreground shadow-2xl flex flex-col">
          <Dialog.Title className="flex items-center justify-between gap-3 border-b border-border px-5 py-3.5">
            <div className="flex items-center gap-2.5 text-sm font-semibold">
              <Badge variant={isExa ? 'outline' : 'default'} className={isExa ? 'border-violet-500/40 text-violet-600 dark:text-violet-400' : ''}>
                {isExa ? 'Exa' : 'Tavily'}
              </Badge>
              <span className="font-mono">{log.tool}</span>
              <Badge variant="outline">{isSearch ? 'search' : 'extract'}</Badge>
              {!log.success && <Badge variant="destructive">失败</Badge>}
            </div>
            <Dialog.Close className="rounded-md p-1 hover:bg-muted">
              <X className="size-4" />
            </Dialog.Close>
          </Dialog.Title>

          <div className="overflow-y-auto p-5 space-y-5">
            {/* 消费额度 */}
            <div className="flex items-center gap-2 rounded-lg border border-border bg-muted/40 px-3.5 py-2.5 text-sm">
              <span className="text-muted-foreground">本次消耗</span>
              <span className="font-mono font-semibold tabular-nums">
                {isExa ? `$${log.credits.toFixed(4)}` : `${log.credits} credits`}
              </span>
              <span className="text-xs text-muted-foreground">
                {isExa ? '（Exa 按美元计费）' : '（Tavily 按 credits 计费）'}
              </span>
            </div>

            {log.error && (
              <div className="rounded-lg border border-red-300 bg-red-50 dark:border-red-500/30 dark:bg-red-500/10 px-3.5 py-2.5 text-sm text-red-700 dark:text-red-300">
                {log.error}
              </div>
            )}

            {/* 请求参数 */}
            <div>
              <div className="flex items-center justify-between mb-1.5">
                <h4 className="text-sm font-medium">请求参数</h4>
                <button
                  className="text-xs text-muted-foreground hover:text-foreground"
                  onClick={() => setJsonView(jsonView === 'pretty' ? 'raw' : 'pretty')}
                >
                  {jsonView === 'pretty' ? '原始视图' : '格式化视图'}
                </button>
              </div>
              <pre
                className="max-h-56 overflow-auto rounded-lg bg-muted/60 p-3 text-xs leading-relaxed font-mono whitespace-pre-wrap break-words"
                dangerouslySetInnerHTML={{
                  __html:
                    jsonView === 'pretty'
                      ? highlightJson(prettyJson(log.params_json))
                      : log.params_json || '—',
                }}
              />
            </div>

            {/* 响应 */}
            <div>
              <div className="flex items-center justify-between mb-1.5">
                <h4 className="text-sm font-medium">Response</h4>
                {markdown ? (
                  <div className="flex items-center gap-1 rounded-md border border-border p-0.5">
                    {(['render', 'raw'] as const).map((v) => (
                      <button
                        key={v}
                        className={`rounded px-2 py-0.5 text-xs ${mdView === v ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:text-foreground'}`}
                        onClick={() => setMdView(v)}
                      >
                        {v === 'render' ? 'Markdown 渲染' : '原样'}
                      </button>
                    ))}
                  </div>
                ) : (
                  <button
                    className="text-xs text-muted-foreground hover:text-foreground"
                    onClick={() => setJsonView(jsonView === 'pretty' ? 'raw' : 'pretty')}
                  >
                    {jsonView === 'pretty' ? '原始视图' : '格式化视图'}
                  </button>
                )}
              </div>

              {markdown ? (
                mdView === 'render' ? (
                  <div
                    className="max-h-[40vh] overflow-auto rounded-lg bg-muted/40 p-4 text-sm prose prose-sm dark:prose-invert prose-headings:font-semibold prose-p:my-2 prose-pre:bg-muted prose-pre:p-3"
                    dangerouslySetInnerHTML={{ __html: renderMarkdown(markdown) }}
                  />
                ) : (
                  <pre className="max-h-[40vh] overflow-auto rounded-lg bg-muted/60 p-3 text-xs leading-relaxed font-mono whitespace-pre-wrap break-words">
                    {markdown}
                  </pre>
                )
              ) : (
                <pre
                  className="max-h-[40vh] overflow-auto rounded-lg bg-muted/60 p-3 text-xs leading-relaxed font-mono whitespace-pre-wrap break-words"
                  dangerouslySetInnerHTML={{
                    __html: log.response_json
                      ? jsonView === 'pretty'
                        ? highlightJson(prettyJson(log.response_json))
                        : log.response_json
                      : '—',
                  }}
                />
              )}
            </div>
          </div>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  )
}
