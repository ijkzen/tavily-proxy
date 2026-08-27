/** 上游密钥的共享视图模型与状态展示映射（看板与上游密钥页共用）。 */
export interface UpstreamKey {
  id: number
  nickname: string
  key_tail: string
  status: 'active' | 'cooling' | 'exhausted' | 'disabled'
  reset_day: number
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
