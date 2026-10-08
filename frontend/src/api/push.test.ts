import { beforeEach, describe, expect, it, vi } from 'vitest'
import { getPushConfig, getPushPrefs, patchPushPrefs, subscribePush, unsubscribePush } from './push'
import { tokenStorage } from './tokens'

const fetchMock = vi.fn()

beforeEach(() => {
  localStorage.clear()
  fetchMock.mockReset()
  fetchMock.mockImplementation(async () => new Response('{}', { status: 200 }))
  vi.stubGlobal('fetch', fetchMock)
  tokenStorage.set('a', 'r')
})

describe('push endpoints', () => {
  it('hit the documented routes with the documented bodies', async () => {
    const body = {
      endpoint: 'https://fcm.googleapis.com/x',
      keys: { p256dh: 'p', auth: 'a' },
      lang: 'en' as const,
    }
    await getPushConfig()
    await subscribePush(body)
    await unsubscribePush(body.endpoint)
    await getPushPrefs()
    await patchPushPrefs({ messages: false })
    const calls = fetchMock.mock.calls.map(([url, init]) => [
      init.method,
      url,
      init.body ? JSON.parse(init.body) : undefined,
    ])
    expect(calls).toEqual([
      ['GET', '/api/push/config', undefined],
      ['POST', '/api/me/push/subscriptions', body],
      ['DELETE', '/api/me/push/subscriptions', { endpoint: body.endpoint }],
      ['GET', '/api/me/push/prefs', undefined],
      ['PATCH', '/api/me/push/prefs', { messages: false }],
    ])
  })

  it('reads the config without a token', async () => {
    await getPushConfig()
    expect(fetchMock.mock.calls[0][1].headers.Authorization).toBeUndefined()
  })
})
