/**
 * Service-worker push handling as plain functions (docs/api.md "Push notifications" → PushPayload),
 * so they run under Vitest without a worker scope.
 */

export type PushType = 'wave' | 'match' | 'message'

export interface PushPayload {
  type: PushType
  title: string
  body: string
  url: string
  tag: string
}

const FALLBACK: PushPayload = {
  type: 'message',
  title: 'localdate',
  body: '',
  url: '/',
  tag: 'localdate',
}

// Backslashes (browsers read them as `/`) and control characters (stripped by the URL parser, so
// "/\t/evil.com" would become "//evil.com") must never reach the resolver.
// eslint-disable-next-line no-control-regex -- matching control characters is the point
const UNSAFE = /[\\\u0000-\u001f\u007f]/

/**
 * `url` resolved against `origin` if it stays on that origin, as path + query + hash; anything else
 * (another origin, a protocol-relative or malformed URL) opens the app root.
 */
export function safePath(url: unknown, origin: string): string {
  if (typeof url !== 'string' || UNSAFE.test(url)) return '/'
  try {
    const resolved = new URL(url, origin)
    if (resolved.origin !== new URL(origin).origin) return '/'
    return resolved.pathname + resolved.search + resolved.hash
  } catch {
    return '/'
  }
}

const sameOrigin = (url: string, origin: string) => {
  try {
    return new URL(url).origin === new URL(origin).origin
  } catch {
    return false
  }
}

/** Never throws: a push must always end in a notification, or browsers penalise the origin. */
export function parsePayload(text: string | null | undefined, origin: string): PushPayload {
  let raw: Partial<Record<keyof PushPayload, unknown>> = {}
  try {
    const parsed: unknown = text ? JSON.parse(text) : null
    if (parsed && typeof parsed === 'object') raw = parsed
  } catch {
    /* not JSON: show the generic notification */
  }
  const str = (v: unknown, fallback: string) => (typeof v === 'string' && v ? v : fallback)
  const type = raw.type === 'wave' || raw.type === 'match' ? raw.type : FALLBACK.type
  return {
    type,
    title: str(raw.title, FALLBACK.title),
    body: str(raw.body, FALLBACK.body),
    url: safePath(raw.url, origin),
    tag: str(raw.tag, FALLBACK.tag),
  }
}

export function notificationFor(p: PushPayload): { title: string; options: NotificationOptions } {
  return {
    title: p.title,
    options: {
      body: p.body,
      tag: p.tag,
      icon: '/icon-192.png',
      badge: '/badge-96.png',
      data: { url: p.url },
    },
  }
}

/** What the page receives when a notification click reuses an open tab. */
export interface NavigateMessage {
  type: 'navigate'
  url: string
}

interface WindowClientLike {
  url: string
  focus(): Promise<unknown>
  postMessage(message: NavigateMessage): void
}

interface ClientsLike {
  matchAll(options: {
    type: 'window'
    includeUncontrolled: boolean
  }): Promise<readonly WindowClientLike[]>
  openWindow(url: string): Promise<unknown>
}

/** Focuses an open tab of the app and routes it to `url`, or opens a new one. */
export async function openOrFocus(clients: ClientsLike, origin: string, url: unknown) {
  const path = safePath(url, origin)
  const open = await clients.matchAll({ type: 'window', includeUncontrolled: true })
  const tab = open.find((c) => sameOrigin(c.url, origin))
  if (!tab) {
    await clients.openWindow(path)
    return
  }
  tab.postMessage({ type: 'navigate', url: path })
  await tab.focus()
}

/** The in-app route of a `navigate` message from the service worker, else null. */
export function navigateTarget(data: unknown, origin: string): string | null {
  if (!data || typeof data !== 'object') return null
  const msg = data as Partial<NavigateMessage>
  return msg.type === 'navigate' && typeof msg.url === 'string' ? safePath(msg.url, origin) : null
}
