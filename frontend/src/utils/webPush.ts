import type { PushLang, PushSubscriptionBody } from '@/api/types'

export type PushSupport = 'supported' | 'ios_needs_install' | 'unsupported'

export interface PushEnv {
  userAgent: string
  maxTouchPoints: number
  /** Launched from the home screen (iOS `navigator.standalone` or display-mode standalone). */
  standalone: boolean
  hasPush: boolean
}

/** iOS/iPadOS only offers Web Push to a web app added to the home screen. */
export function detectSupport(env: PushEnv): PushSupport {
  const ios =
    /iPad|iPhone|iPod/.test(env.userAgent) ||
    // iPadOS reports itself as a Mac; touch gives it away.
    (/Macintosh/.test(env.userAgent) && env.maxTouchPoints > 1)
  if (ios && !env.standalone) return 'ios_needs_install'
  return env.hasPush ? 'supported' : 'unsupported'
}

export function currentEnv(): PushEnv {
  const nav = navigator as Navigator & { standalone?: boolean }
  return {
    userAgent: nav.userAgent,
    maxTouchPoints: nav.maxTouchPoints ?? 0,
    standalone:
      nav.standalone === true || window.matchMedia?.('(display-mode: standalone)').matches === true,
    hasPush: 'serviceWorker' in nav && 'PushManager' in window && 'Notification' in window,
  }
}

export const toPushLang = (locale: string): PushLang => (locale === 'en' ? 'en' : 'cs')

export function base64UrlToBytes(s: string): Uint8Array<ArrayBuffer> {
  const b64 = s.replace(/-/g, '+').replace(/_/g, '/').replace(/=+$/, '')
  const bin = atob(b64 + '='.repeat((4 - (b64.length % 4)) % 4))
  const out = new Uint8Array(new ArrayBuffer(bin.length))
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
  return out
}

/** Whether a subscription was made with the server's current VAPID key (keys can be rotated). */
export function sameKey(key: ArrayBuffer | null | undefined, publicKey: string): boolean {
  if (!key) return false
  const a = new Uint8Array(key)
  const b = base64UrlToBytes(publicKey)
  return a.length === b.length && a.every((v, i) => v === b[i])
}

export function subscriptionBody(sub: PushSubscription, lang: PushLang): PushSubscriptionBody {
  const keys = sub.toJSON().keys ?? {}
  return {
    endpoint: sub.endpoint,
    keys: { p256dh: keys.p256dh ?? '', auth: keys.auth ?? '' },
    lang,
  }
}

// Browser adapter below: the only code touching Notification / PushManager (mocked in tests).

export const permission = (): NotificationPermission => Notification.permission

export const requestPermission = (): Promise<NotificationPermission> =>
  Notification.requestPermission()

/**
 * The active registration. `ready` waits for the worker to activate (right after the first load it
 * may still be installing, and subscribing needs an active one); it never settles without a
 * registration, as in `make run-web`, hence the check first.
 */
async function registration(): Promise<ServiceWorkerRegistration | undefined> {
  if (!(await navigator.serviceWorker.getRegistration())) return undefined
  return navigator.serviceWorker.ready
}

export async function existingSubscription(): Promise<PushSubscription | null> {
  const reg = await registration()
  return reg ? reg.pushManager.getSubscription() : null
}

/** The current subscription, renewed if it was made with another VAPID key. */
export async function subscribeBrowser(publicKey: string): Promise<PushSubscription> {
  const reg = await registration()
  if (!reg) throw new Error('no service worker registered')
  const existing = await reg.pushManager.getSubscription()
  if (existing && sameKey(existing.options.applicationServerKey, publicKey)) return existing
  await existing?.unsubscribe()
  return reg.pushManager.subscribe({
    userVisibleOnly: true,
    applicationServerKey: base64UrlToBytes(publicKey),
  })
}
