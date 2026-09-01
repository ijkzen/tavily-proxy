import { useState } from 'react'
import { Check, Copy, Eye, EyeOff } from 'lucide-react'
import { useSecret, type Secret } from '@/lib/use-secret'

function ActionButtons({
  secret,
  copyBuild,
}: {
  secret: Secret
  copyBuild?: (plain: string) => string
}) {
  const [copied, setCopied] = useState(false)
  return (
    <span className="inline-flex items-center gap-1 shrink-0">
      <button
        type="button"
        title={secret.visible ? '隐藏明文' : '显示明文'}
        className="p-1 rounded hover:bg-muted text-muted-foreground transition-colors"
        onClick={secret.toggle}
      >
        {secret.visible ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
      </button>
      <button
        type="button"
        title="复制"
        className="p-1 rounded hover:bg-muted text-muted-foreground transition-colors"
        onClick={async () => {
          if (await secret.copy(copyBuild)) {
            setCopied(true)
            setTimeout(() => setCopied(false), 1500)
          }
        }}
      >
        {copied ? <Check className="size-4 text-emerald-600" /> : <Copy className="size-4" />}
      </button>
    </span>
  )
}

/** 密钥展示行（展示组件，状态由调用方的 useSecret 提供，可与 MCP 链接行共享）。 */
export function SecretLine({ secret, masked }: { secret: Secret; masked: string }) {
  if (secret.unavailable) {
    return <span className="text-muted-foreground text-xs">{masked}（旧密钥无明文，吊销重建后可启用）</span>
  }
  return (
    <span className="flex items-center gap-1">
      <code className="font-mono text-sm text-foreground/80 break-all">
        {secret.visible && secret.plain !== null ? secret.plain : masked}
      </code>
      <ActionButtons secret={secret} />
    </span>
  )
}

/** 自含状态的密钥单元格（上游密钥页用）。 */
export function SecretCell({ revealPath, masked }: { revealPath: string; masked: string }) {
  const secret = useSecret(revealPath)
  return <SecretLine secret={secret} masked={masked} />
}

/** MCP 集成链接（查询参数形式）：独立一行卡片式展示，复制时含完整 key。
 *  单元格自带 whitespace-nowrap，这里显式恢复换行，避免长链接把表格撑爆。 */
export function McpLinkLine({ secret }: { secret: Secret }) {
  if (secret.unavailable) return null
  const origin = window.location.origin
  const shown =
    secret.visible && secret.plain !== null
      ? `${origin}/mcp?key=${secret.plain}`
      : `${origin}/mcp?key=tp-••••`
  return (
    <div className="mt-1.5 rounded-lg border border-border bg-muted/50 px-2.5 py-2 whitespace-normal">
      <div className="flex items-center justify-between gap-2 mb-1">
        <span className="text-xs text-muted-foreground shrink-0">MCP 集成链接</span>
        <ActionButtons secret={secret} copyBuild={(p) => `${origin}/mcp?key=${p}`} />
      </div>
      <code className="block font-mono text-xs text-foreground/80 break-all select-all">
        {shown}
      </code>
    </div>
  )
}
