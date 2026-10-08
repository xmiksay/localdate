import { beforeEach, describe, expect, it, vi } from 'vitest'
import { emailPreview, emailSignup, emailStart, emailVerify, getProviders } from './auth'
import { confirmEmailLink, deleteIdentity, getIdentities, linkEmail } from './identities'
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

describe('email auth endpoints', () => {
  it('are anonymous and hit the documented routes', async () => {
    await getProviders()
    fetchMock.mockImplementationOnce(async () => new Response(null, { status: 202 }))
    await emailStart({ email: 'a@b.cz', lang: 'en' })
    await emailPreview('tok')
    await emailVerify('tok')
    await emailSignup({ token: 'tok', username: 'bob' })
    expect(calls()).toEqual([
      { url: '/api/auth/providers', method: 'GET', auth: undefined, body: undefined },
      {
        url: '/api/auth/email/start',
        method: 'POST',
        auth: undefined,
        body: { email: 'a@b.cz', lang: 'en' },
      },
      { url: '/api/auth/email/preview', method: 'POST', auth: undefined, body: { token: 'tok' } },
      { url: '/api/auth/email/verify', method: 'POST', auth: undefined, body: { token: 'tok' } },
      {
        url: '/api/auth/email/signup',
        method: 'POST',
        auth: undefined,
        body: { token: 'tok', username: 'bob' },
      },
    ])
  })

  it('surfaces email_disabled as an ApiError', async () => {
    fetchMock.mockImplementationOnce(
      async () =>
        new Response(JSON.stringify({ error: { code: 'email_disabled', message: 'off' } }), {
          status: 503,
        }),
    )
    await expect(emailStart({ email: 'a@b.cz', lang: 'cs' })).rejects.toMatchObject({
      code: 'email_disabled',
      status: 503,
    })
  })
})

describe('identity endpoints', () => {
  it('are authenticated and hit the documented routes', async () => {
    await getIdentities()
    fetchMock.mockImplementationOnce(async () => new Response(null, { status: 202 }))
    await linkEmail({ email: 'a@b.cz', lang: 'cs' })
    await confirmEmailLink('tok')
    fetchMock.mockImplementationOnce(async () => new Response(null, { status: 204 }))
    await deleteIdentity('i1')
    expect(calls()).toEqual([
      { url: '/api/me/identities', method: 'GET', auth: 'Bearer a', body: undefined },
      {
        url: '/api/me/identities/email',
        method: 'POST',
        auth: 'Bearer a',
        body: { email: 'a@b.cz', lang: 'cs' },
      },
      {
        url: '/api/me/identities/email/confirm',
        method: 'POST',
        auth: 'Bearer a',
        body: { token: 'tok' },
      },
      { url: '/api/me/identities/i1', method: 'DELETE', auth: 'Bearer a', body: undefined },
    ])
  })
})
