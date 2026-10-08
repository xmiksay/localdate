import { beforeEach, describe, expect, it, vi } from 'vitest'
import { passwordForgot, passwordReset, passwordResetPreview } from './auth'
import { putPassword } from './me'
import { tokenStorage } from './tokens'

const fetchMock = vi.fn()
const calls = () =>
  fetchMock.mock.calls.map(([url, init]) => ({
    url,
    method: init.method,
    auth: init.headers.Authorization,
    body: init.body ? JSON.parse(init.body) : undefined,
  }))

beforeEach(() => {
  localStorage.clear()
  fetchMock.mockReset()
  fetchMock.mockImplementation(async () => new Response('{}', { status: 200 }))
  vi.stubGlobal('fetch', fetchMock)
  tokenStorage.set('a', 'r')
})

describe('password endpoints', () => {
  it('reset endpoints are anonymous and hit the documented routes', async () => {
    fetchMock.mockImplementationOnce(async () => new Response(null, { status: 202 }))
    await passwordForgot({ login: 'bob', lang: 'en' })
    await passwordResetPreview('tok')
    fetchMock.mockImplementationOnce(async () => new Response(null, { status: 204 }))
    await passwordReset({ token: 'tok', new_password: 'n'.repeat(10) })
    expect(calls()).toEqual([
      {
        url: '/api/auth/password/forgot',
        method: 'POST',
        auth: undefined,
        body: { login: 'bob', lang: 'en' },
      },
      {
        url: '/api/auth/password/reset/preview',
        method: 'POST',
        auth: undefined,
        body: { token: 'tok' },
      },
      {
        url: '/api/auth/password/reset',
        method: 'POST',
        auth: undefined,
        body: { token: 'tok', new_password: 'nnnnnnnnnn' },
      },
    ])
  })

  it('a wrong current password surfaces as invalid_credentials without a token refresh', async () => {
    fetchMock.mockImplementationOnce(
      async () =>
        new Response(JSON.stringify({ error: { code: 'invalid_credentials', message: 'no' } }), {
          status: 401,
        }),
    )
    await expect(
      putPassword({ current_password: 'old', new_password: 'n'.repeat(10) }),
    ).rejects.toMatchObject({ code: 'invalid_credentials', status: 401 })
    expect(calls()).toEqual([
      {
        url: '/api/me/password',
        method: 'PUT',
        auth: 'Bearer a',
        body: { current_password: 'old', new_password: 'nnnnnnnnnn' },
      },
    ])
    expect(tokenStorage.access()).toBe('a')
  })
})
