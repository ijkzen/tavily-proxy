export interface ApiResult<T = unknown> {
  ok: boolean
  status: number
  data?: T
}

export async function api<T = unknown>(
  path: string,
  init?: RequestInit & { json?: unknown },
): Promise<ApiResult<T>> {
  const { json, ...rest } = init ?? {}
  const resp = await fetch(path, {
    ...rest,
    headers: json !== undefined ? { 'Content-Type': 'application/json' } : undefined,
    body: json !== undefined ? JSON.stringify(json) : undefined,
  })
  const data = resp.headers.get('content-type')?.includes('json')
    ? ((await resp.json()) as T)
    : undefined
  return { ok: resp.ok, status: resp.status, data }
}
