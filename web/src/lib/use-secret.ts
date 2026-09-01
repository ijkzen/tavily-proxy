import { useState } from 'react'
import { api } from '@/lib/api'
import { copyText } from '@/lib/clipboard'

/**
 * 一个密钥的明文状态：按需调 reveal 接口解密（懒加载，不主动暴露），
 * 眼睛切换显示，复制即用。双写改造前的旧代理密钥 reveal 返回 409 → unavailable。
 */
export function useSecret(revealPath: string) {
  const [plain, setPlain] = useState<string | null>(null)
  const [visible, setVisible] = useState(false)
  const [unavailable, setUnavailable] = useState(false)

  async function ensure(): Promise<string | null> {
    if (plain !== null) return plain
    const resp = await api<{ key: string }>(revealPath, { method: 'POST' })
    if (resp.ok && resp.data) {
      setPlain(resp.data.key)
      return resp.data.key
    }
    if (resp.status === 409) setUnavailable(true)
    return null
  }

  async function toggle() {
    if (!visible && plain === null && (await ensure()) === null) return
    setVisible(!visible)
  }

  /** 复制；build 可把明文包装成别的文本（如完整 MCP 链接）。 */
  async function copy(build?: (plain: string) => string): Promise<boolean> {
    const p = await ensure()
    if (p === null) return false
    return copyText(build ? build(p) : p)
  }

  return { plain, visible, unavailable, toggle, copy }
}

export type Secret = ReturnType<typeof useSecret>
