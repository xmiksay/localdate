import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, freshAccessToken, get, post, request, setAuthHooks } from './client'
import { tokenStorage } from './tokens'

const json = (status: number, body: unknown) =>
  new Response(body === undefined ? null : JSON.stringify(body), { status })
const err = (status: number, code: string) => json(status, { error: { code, message: 'm' } })
const tokens = {
  access_token: 'new-a',
  refresh_token: 'new-r',
  user: { id: 'u', username: 'bob', created_at: '' },
}

const fetchMock = vi.fn()
const authLost = vi.fn()
const banned = vi.fn()

beforeEach(() => {
  localStorage.clear()
  fetchMock.mockReset()
  authLost.mockReset()
  banned.mockReset()
  vi.stubGlobal('fetch', fetchMock)
  setAuthHooks({ onAuthLost: authLost, onBanned: banned })
  tokenStorage.set('old-a', 'old-r')
})

describe('api client', () => {
  it('sends bearer token and parses json', async () => {
    fetchMock.mockResolvedValue(json(200, { ok: 1 }))
    expect(await get('/me')).toEqual({ ok: 1 })
    const [url, init] = fetchMock.mock.calls[0]
    expect(url).toBe('/api/me')
    expect(init.headers.Authorization).toBe('Bearer old-a')
  })

  it('returns undefined on 204', async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 204 }))
    expect(await post('/blocks', { user_id: 'x' })).toBeUndefined()
  })

  it('returns undefined on 201 with empty body', async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 201 }))
    expect(await post('/reports', { user_id: 'x', reason: 'spam' })).toBeUndefined()
  })

  it('parses error envelope into ApiError', async () => {
    fetchMock.mockImplementation(async () => err(409, 'username_taken'))
    await expect(get('/x')).rejects.toMatchObject({ code: 'username_taken', status: 409 })
    await expect(get('/x')).rejects.toBeInstanceOf(ApiError)
  })

  it('maps non-JSON failures to unknown and fetch failures to network', async () => {
    fetchMock.mockResolvedValueOnce(new Response('boom', { status: 502 }))
    await expect(get('/x')).rejects.toMatchObject({ code: 'unknown', status: 502 })
    fetchMock.mockRejectedValueOnce(new TypeError('offline'))
    await expect(get('/x')).rejects.toMatchObject({ code: 'network' })
  })

  it('refreshes once and retries on 401 unauthorized', async () => {
    fetchMock.mockImplementation(async (url: string, init: RequestInit) => {
      if (url === '/api/auth/refresh') return json(200, tokens)
      const auth = (init.headers as Record<string, string>).Authorization
      return auth === 'Bearer new-a' ? json(200, { ok: 1 }) : err(401, 'unauthorized')
    })
    expect(await get('/me')).toEqual({ ok: 1 })
    expect(tokenStorage.access()).toBe('new-a')
    expect(tokenStorage.refresh()).toBe('new-r')
  })

  it('deduplicates concurrent refreshes', async () => {
    fetchMock.mockImplementation(async (url: string, init: RequestInit) => {
      if (url === '/api/auth/refresh') return json(200, tokens)
      const auth = (init.headers as Record<string, string>).Authorization
      return auth === 'Bearer new-a' ? json(200, {}) : err(401, 'unauthorized')
    })
    await Promise.all([get('/a'), get('/b'), get('/c')])
    const refreshes = fetchMock.mock.calls.filter(([u]) => u === '/api/auth/refresh')
    expect(refreshes).toHaveLength(1)
  })

  it('clears tokens and signals auth loss when refresh fails', async () => {
    fetchMock.mockImplementation(async (url: string) =>
      url === '/api/auth/refresh' ? err(401, 'invalid_refresh_token') : err(401, 'unauthorized'),
    )
    await expect(get('/me')).rejects.toMatchObject({ code: 'unauthorized' })
    expect(tokenStorage.access()).toBeNull()
    expect(authLost).toHaveBeenCalledOnce()
  })

  it('does not refresh on invalid_credentials', async () => {
    fetchMock.mockResolvedValue(err(401, 'invalid_credentials'))
    await expect(get('/x')).rejects.toMatchObject({ code: 'invalid_credentials' })
    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(authLost).not.toHaveBeenCalled()
  })

  it('clears tokens and signals a ban on 403 banned', async () => {
    fetchMock.mockResolvedValue(err(403, 'banned'))
    await expect(get('/me')).rejects.toMatchObject({ code: 'banned', status: 403 })
    expect(tokenStorage.access()).toBeNull()
    expect(tokenStorage.refresh()).toBeNull()
    expect(banned).toHaveBeenCalledOnce()
  })

  it('signals a ban on anonymous requests too (login)', async () => {
    tokenStorage.clear()
    fetchMock.mockResolvedValue(err(403, 'banned'))
    await expect(
      request('/auth/login', { method: 'POST', body: {}, anon: true }),
    ).rejects.toMatchObject({ code: 'banned' })
    expect(banned).toHaveBeenCalledOnce()
  })

  it('does not treat other 403s as a ban', async () => {
    fetchMock.mockResolvedValue(err(403, 'forbidden'))
    await expect(get('/x')).rejects.toMatchObject({ code: 'forbidden' })
    expect(banned).not.toHaveBeenCalled()
    expect(tokenStorage.access()).toBe('old-a')
  })

  it('signals a ban when the refresh call is rejected as banned', async () => {
    fetchMock.mockImplementation(async (url: string) =>
      url === '/api/auth/refresh' ? err(403, 'banned') : err(401, 'unauthorized'),
    )
    await expect(get('/me')).rejects.toMatchObject({ code: 'banned', status: 403 })
    expect(banned).toHaveBeenCalledOnce()
    expect(authLost).not.toHaveBeenCalled()
    expect(tokenStorage.refresh()).toBeNull()
  })
})

describe('freshAccessToken', () => {
  const jwt = (expSec: number) => `h.${btoa(JSON.stringify({ exp: expSec }))}.s`

  it('returns a still-valid token without refreshing', async () => {
    const valid = jwt(Date.now() / 1000 + 600)
    tokenStorage.set(valid, 'r')
    expect(await freshAccessToken()).toBe(valid)
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('refreshes an expiring token', async () => {
    tokenStorage.set(jwt(Date.now() / 1000 + 5), 'r')
    fetchMock.mockResolvedValue(json(200, tokens))
    expect(await freshAccessToken()).toBe('new-a')
  })

  it('does not drop the session when the refresh fails', async () => {
    fetchMock.mockRejectedValue(new Error('offline'))
    expect(await freshAccessToken(true)).toBe('old-a')
    expect(authLost).not.toHaveBeenCalled()
  })

  it('signals a ban when the WebSocket token refresh is rejected as banned', async () => {
    fetchMock.mockResolvedValue(err(403, 'banned'))
    expect(await freshAccessToken(true)).toBeNull()
    expect(banned).toHaveBeenCalledOnce()
  })
})
