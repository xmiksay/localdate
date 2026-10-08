import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import * as authApi from '@/api/auth'
import * as pushApi from '@/api/push'
import { tokenStorage } from '@/api/tokens'
import * as webPush from '@/utils/webPush'
import { useAuthStore } from './auth'
import { usePushStore } from './push'

vi.mock('@/api/push')
vi.mock('@/api/auth')
vi.mock('@/utils/webPush', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/utils/webPush')>()),
  currentEnv: vi.fn(),
  permission: vi.fn(),
  requestPermission: vi.fn(),
  existingSubscription: vi.fn(),
  subscribeBrowser: vi.fn(),
}))

const ENABLED = { enabled: true, public_key: 'AQID' }
const desktop = {
  userAgent: 'Mozilla/5.0 (X11; Linux x86_64)',
  maxTouchPoints: 0,
  standalone: false,
  hasPush: true,
}

/** A PushSubscription as the PushManager would hand it out. */
function fakeSub(endpoint = 'https://fcm.googleapis.com/fcm/send/1') {
  return {
    endpoint,
    toJSON: () => ({ endpoint, keys: { p256dh: 'P', auth: 'A' } }),
    unsubscribe: vi.fn(async () => true),
  } as unknown as PushSubscription & { unsubscribe: ReturnType<typeof vi.fn> }
}

beforeEach(() => {
  vi.resetAllMocks()
  vi.mocked(webPush.currentEnv).mockReturnValue(desktop)
  vi.mocked(pushApi.getPushConfig).mockResolvedValue(ENABLED)
  vi.mocked(pushApi.subscribePush).mockResolvedValue(undefined)
  vi.mocked(pushApi.unsubscribePush).mockResolvedValue(undefined)
  setActivePinia(createPinia())
})

describe('push store', () => {
  it('enable asks permission, subscribes with the server key and registers the device', async () => {
    const sub = fakeSub()
    vi.mocked(webPush.requestPermission).mockResolvedValue('granted')
    vi.mocked(webPush.permission).mockReturnValue('granted')
    vi.mocked(webPush.subscribeBrowser).mockResolvedValue(sub)
    const push = usePushStore()
    await push.enable('en')
    expect(webPush.subscribeBrowser).toHaveBeenCalledWith('AQID')
    expect(pushApi.subscribePush).toHaveBeenCalledWith({
      endpoint: sub.endpoint,
      keys: { p256dh: 'P', auth: 'A' },
      lang: 'en',
    })
    expect(push.subscribed).toBe(true)
  })

  it('enable stops when permission is denied', async () => {
    vi.mocked(webPush.requestPermission).mockResolvedValue('denied')
    const push = usePushStore()
    await push.enable('cs')
    expect(push.permission).toBe('denied')
    expect(webPush.subscribeBrowser).not.toHaveBeenCalled()
    expect(push.subscribed).toBe(false)
  })

  it('resync re-posts an existing subscription but never creates one', async () => {
    vi.mocked(webPush.permission).mockReturnValue('granted')
    vi.mocked(webPush.existingSubscription).mockResolvedValue(null)
    const push = usePushStore()
    await push.resync('cs')
    expect(webPush.subscribeBrowser).not.toHaveBeenCalled()
    expect(pushApi.subscribePush).not.toHaveBeenCalled()

    const sub = fakeSub('https://fcm.googleapis.com/fcm/send/rotated')
    vi.mocked(webPush.existingSubscription).mockResolvedValue(sub)
    vi.mocked(webPush.subscribeBrowser).mockResolvedValue(sub)
    await push.resync('cs')
    expect(pushApi.subscribePush).toHaveBeenCalledWith(
      expect.objectContaining({ endpoint: sub.endpoint, lang: 'cs' }),
    )
    expect(push.subscribed).toBe(true)
  })

  it('resync does nothing without permission or when the server has push off', async () => {
    vi.mocked(webPush.permission).mockReturnValue('default')
    await usePushStore().resync('cs')
    vi.mocked(webPush.permission).mockReturnValue('granted')
    setActivePinia(createPinia())
    vi.mocked(pushApi.getPushConfig).mockResolvedValue({ enabled: false, public_key: null })
    await usePushStore().resync('cs')
    expect(webPush.existingSubscription).not.toHaveBeenCalled()
    expect(pushApi.subscribePush).not.toHaveBeenCalled()
  })

  it('iOS outside the home screen is never offered push', async () => {
    vi.mocked(webPush.currentEnv).mockReturnValue({
      ...desktop,
      userAgent: 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X)',
    })
    const push = usePushStore()
    await push.loadConfig()
    expect(push.support).toBe('ios_needs_install')
    expect(push.available).toBe(false)
  })

  it('disable tells the server and drops the browser subscription even if the server fails', async () => {
    const sub = fakeSub()
    vi.mocked(webPush.existingSubscription).mockResolvedValue(sub)
    vi.mocked(pushApi.unsubscribePush).mockRejectedValue(new Error('offline'))
    const push = usePushStore()
    push.subscribed = true
    await push.disable()
    expect(pushApi.unsubscribePush).toHaveBeenCalledWith(sub.endpoint)
    expect(sub.unsubscribe).toHaveBeenCalled()
    expect(push.subscribed).toBe(false)
  })

  it('logout tells the server with the still-valid token, then unsubscribes, then clears tokens', async () => {
    tokenStorage.set('access-1', 'refresh-1')
    const steps: string[] = []
    const sub = fakeSub()
    sub.unsubscribe.mockImplementation(async () => {
      steps.push(`unsubscribe:${tokenStorage.access()}`)
      return true
    })
    vi.mocked(webPush.existingSubscription).mockResolvedValue(sub)
    vi.mocked(pushApi.unsubscribePush).mockImplementation(async () => {
      steps.push(`delete:${tokenStorage.access()}`)
    })
    vi.mocked(authApi.logout).mockResolvedValue(undefined)
    const push = usePushStore()
    push.subscribed = true

    await useAuthStore().logout()
    expect(steps).toEqual(['delete:access-1', 'unsubscribe:access-1'])
    expect(tokenStorage.access()).toBeNull()
    expect(push.subscribed).toBe(false)
  })

  it('losing the session without a logout keeps the browser subscription for the next login', async () => {
    const sub = fakeSub()
    vi.mocked(webPush.existingSubscription).mockResolvedValue(sub)
    vi.mocked(webPush.permission).mockReturnValue('granted')
    vi.mocked(webPush.subscribeBrowser).mockResolvedValue(sub)
    const push = usePushStore()
    push.subscribed = true

    push.reset()
    expect(sub.unsubscribe).not.toHaveBeenCalled()
    expect(pushApi.unsubscribePush).not.toHaveBeenCalled()
    expect(push.subscribed).toBe(false)

    // Next login: resync re-posts it, which moves the server row to the new account.
    await push.resync('cs')
    expect(pushApi.subscribePush).toHaveBeenCalledWith(
      expect.objectContaining({ endpoint: sub.endpoint }),
    )
    expect(push.subscribed).toBe(true)
  })

  it('setPref patches one kind and keeps the server answer', async () => {
    const saved = { waves: true, matches: true, messages: false }
    vi.mocked(pushApi.patchPushPrefs).mockResolvedValue(saved)
    const push = usePushStore()
    await push.setPref('messages', false)
    expect(pushApi.patchPushPrefs).toHaveBeenCalledWith({ messages: false })
    expect(push.prefs).toEqual(saved)
  })
})
