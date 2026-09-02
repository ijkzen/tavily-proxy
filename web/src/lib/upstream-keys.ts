/** 上游密钥的共享视图模型与状态展示映射（看板与上游密钥页共用）。 */
export type KeyKind = 'tavily' | 'exa'

export interface UpstreamKey {
  id: number
  nickname: string
  key_tail: string
  status: 'active' | 'cooling' | 'exhausted' | 'disabled'
  kind: KeyKind
  reset_day: number
  /** tavily 为 credits，exa 为美元（本地记账浮点）。 */
  usage: number
  limit: number | null
  created_at: number
}

export const STATUS_LABEL: Record<UpstreamKey['status'], string> = {
  active: '正常',
  cooling: '冷却',
  exhausted: '耗尽',
  disabled: '禁用',
}

export const STATUS_VARIANT: Record<
  UpstreamKey['status'],
  'default' | 'secondary' | 'destructive' | 'outline'
> = {
  active: 'default',
  cooling: 'secondary',
  exhausted: 'destructive',
  disabled: 'outline',
}

export const KIND_LABEL: Record<KeyKind, string> = {
  tavily: 'Tavily',
  exa: 'Exa',
}

/** 用量展示：tavily 按 credits，exa 按美元。 */
export function usageText(k: UpstreamKey): string {
  if (k.limit === null) return `${fmtUsage(k.usage)} / 未知`
  return `${fmtUsage(k.usage)} / ${fmtUsage(k.limit)}`
}

export function fmtUsage(v: number): string {
  return v >= 100 ? Math.round(v).toLocaleString() : v.toFixed(2)
}

/** 组内排名键：剩余额度（tavily credits / exa 美元），limit 未知按无限。 */
export function remaining(k: UpstreamKey): number {
  return k.limit === null ? Number.MAX_SAFE_INTEGER : k.limit - k.usage
}
