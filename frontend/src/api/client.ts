import type { ClientErrorCode, ErrorCode, Tokens } from './types'
import { tokenStorage } from './tokens'

const BASE = '/api'

export class ApiError extends Error {
  constructor(
    public code: ErrorCode | ClientErrorCode,
    public status: number,
    message: string,
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

type Hooks = {
  onTokens?: (t: Tokens) => void
  onAuthLost?: () => void
}
const hooks: Hooks = {}

/** The auth store registers here so the client stays free of store/router imports. */
export function setAuthHooks(h: Hooks) {
  hooks.onTokens = h.onTokens
  hooks.onAuthLost = h.onAuthLost
}

async function parseError(res: Response): Promise<ApiError> {
  try {
    const body = await res.json()
    if (body?.error?.code) {
      return new ApiError(body.error.code, res.status, String(body.error.message ?? ''))
    }
  } catch {
    /* non-JSON body */
  }
  return new ApiError('unknown', res.status, res.statusText)
}

let refreshing: Promise<boolean> | null = null

/** One in-flight refresh shared by all concurrent 401s. */
function refreshTokens(): Promise<boolean> {
  refreshing ??= (async () => {
    const refresh = tokenStorage.refresh()
    if (!refresh) return false
    try {
      const res = await fetch(`${BASE}/auth/refresh`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ refresh_token: refresh }),
      })
      if (!res.ok) return false
      const t = (await res.json()) as Tokens
      tokenStorage.set(t.access_token, t.refresh_token)
      hooks.onTokens?.(t)
      return true
    } catch {
      return false
    }
  })().finally(() => {
    refreshing = null
  })
  return refreshing
}

function accessExpiresSoon(token: string): boolean {
  try {
    const payload = JSON.parse(atob(token.split('.')[1].replace(/-/g, '+').replace(/_/g, '/')))
    return typeof payload.exp === 'number' && payload.exp * 1000 - Date.now() < 30_000
  } catch {
    return false
  }
}

/**
 * Access token for non-HTTP consumers (WebSocket): refreshes when expired or `force`d.
 * A failed refresh is not treated as auth loss (it may just be offline); the HTTP path
 * decides that on its next 401.
 */
export async function freshAccessToken(force = false): Promise<string | null> {
  const current = tokenStorage.access()
  if (current && !force && !accessExpiresSoon(current)) return current
  await refreshTokens()
  return tokenStorage.access()
}

export interface RequestOptions {
  method?: string
  body?: unknown
  form?: FormData
  query?: Record<string, string | number | undefined>
  /** Skip Bearer + refresh logic (auth endpoints, /interests). */
  anon?: boolean
}

function buildUrl(path: string, query?: RequestOptions['query']): string {
  if (!query) return BASE + path
  const qs = new URLSearchParams()
  for (const [k, v] of Object.entries(query)) if (v !== undefined) qs.set(k, String(v))
  const s = qs.toString()
  return BASE + path + (s ? `?${s}` : '')
}

async function send(path: string, o: RequestOptions): Promise<Response> {
  const headers: Record<string, string> = {}
  let body: BodyInit | undefined
  if (o.form) body = o.form
  else if (o.body !== undefined) {
    headers['Content-Type'] = 'application/json'
    body = JSON.stringify(o.body)
  }
  const access = o.anon ? null : tokenStorage.access()
  if (access) headers.Authorization = `Bearer ${access}`
  try {
    return await fetch(buildUrl(path, o.query), { method: o.method ?? 'GET', headers, body })
  } catch {
    throw new ApiError('network', 0, 'network error')
  }
}

export async function request<T>(path: string, o: RequestOptions = {}): Promise<T> {
  let res = await send(path, o)
  if (res.status === 401 && !o.anon) {
    const err = await parseError(res.clone())
    if (err.code === 'unauthorized') {
      if (await refreshTokens()) {
        res = await send(path, o)
      } else {
        tokenStorage.clear()
        hooks.onAuthLost?.()
        throw err
      }
    }
  }
  if (!res.ok) throw await parseError(res)
  // 204 and e.g. `201` from POST /reports carry no body.
  const text = await res.text()
  return (text ? JSON.parse(text) : undefined) as T
}

export const get = <T>(path: string, query?: RequestOptions['query']) => request<T>(path, { query })
export const post = <T>(path: string, body?: unknown) => request<T>(path, { method: 'POST', body })
export const put = <T>(path: string, body?: unknown) => request<T>(path, { method: 'PUT', body })
export const patch = <T>(path: string, body?: unknown) =>
  request<T>(path, { method: 'PATCH', body })
export const del = <T = void>(path: string) => request<T>(path, { method: 'DELETE' })
