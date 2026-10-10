import type { Health } from './types'

/**
 * `GET /api/health` for the build identity. Raw `fetch`, not `request`: a 503 (DB down) still carries
 * the version, and this must never touch the session. `null` when there is nothing usable.
 */
export async function getHealth(): Promise<Health | null> {
  try {
    const res = await fetch('/api/health')
    const body = (await res.json()) as Partial<Health> | null
    return typeof body?.version === 'string' && typeof body.commit === 'string'
      ? (body as Health)
      : null
  } catch {
    return null
  }
}
