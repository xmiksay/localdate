import { beforeEach, describe, expect, it, vi } from 'vitest'
import { freshAccessToken, get, setAuthHooks, setImpersonationHook } from './client'
import { impersonationStorage, tokenStorage } from './tokens'

const json = (status: number, body: unknown) => new Response(JSON.stringify(body), { status })
const unauthorized = () => json(401, { error: { code: 'unauthorized', message: '' } })
// exp in the past: a normal session would refresh this token.
const expiredJwt = `x.${btoa(JSON.stringify({ exp: 1 }))}.y`

const fetchMock = vi.fn()
const authLost = vi.fn()
const lost = vi.fn()

function impersonate(token = 'imp-a') {
  impersonationStorage.set({
    access_token: token,
    expires_at: new Date(Date.now() + 3_600_000).toISOString(),
    user: { id: 't', username: 'tester', created_at: '' },
    admin: null,
  })
}

beforeEach(() => {
  localStorage.clear()
  sessionStorage.clear()
  fetchMock.mockReset()
  authLost.mockReset()
  lost.mockReset()
  vi.stubGlobal('fetch', fetchMock)
  setAuthHooks({ onAuthLost: authLost })
  setImpersonationHook(lost)
  tokenStorage.set('admin-a', 'admin-r')
})

describe('api client while impersonating', () => {
  it('sends the impersonation token and hides the admin refresh token', async () => {
    impersonate()
    fetchMock.mockResolvedValue(json(200, {}))
    await get('/me')
    expect(fetchMock.mock.calls[0][1].headers.Authorization).toBe('Bearer imp-a')
    expect(tokenStorage.refresh()).toBeNull()
  })

  it('never refreshes for the WebSocket, even with an expired token', async () => {
    impersonate(expiredJwt)
    expect(await freshAccessToken(true)).toBe(expiredJwt)
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('a 401 ends the impersonation without touching the admin session', async () => {
    impersonate()
    fetchMock.mockResolvedValue(unauthorized())
    await expect(get('/nearby')).rejects.toMatchObject({ code: 'unauthorized' })
    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(lost).toHaveBeenCalledWith('expired')
    expect(authLost).not.toHaveBeenCalled()
    expect(tokenStorage.access()).toBe('admin-a')
    expect(tokenStorage.refresh()).toBe('admin-r')
  })

  it('a 401 for an impersonation already over is not retried as the admin', async () => {
    impersonate()
    fetchMock.mockImplementation(async () => {
      impersonationStorage.clear()
      return unauthorized()
    })
    await expect(get('/me/location')).rejects.toMatchObject({ code: 'unauthorized' })
    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(lost).not.toHaveBeenCalled()
    expect(authLost).not.toHaveBeenCalled()
  })

  it('an admin request answered after the impersonation began is not retried as the target', async () => {
    fetchMock.mockImplementation(async () => {
      impersonate()
      return unauthorized()
    })
    await expect(get('/admin/reports')).rejects.toMatchObject({ code: 'unauthorized' })
    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(lost).not.toHaveBeenCalled()
    expect(authLost).not.toHaveBeenCalled()
  })
})
