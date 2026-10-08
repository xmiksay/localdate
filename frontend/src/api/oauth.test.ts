import { beforeEach, describe, expect, it, vi } from 'vitest'
import { oauthExchange, oauthLink, oauthSignup, oauthStartUrl } from './oauth'
import { tokenStorage } from './tokens'

const fetchMock = vi.fn()
const calls = () =>
  fetchMock.mock.calls.map(([url, init]) => ({
    url,
    method: init.method,
    auth: init.headers.Authorization,
    body: init.body ? JSON.parse(init.body) : undefined,
    credentials: init.credentials,
  }))

beforeEach(() => {
  localStorage.clear()
  fetchMock.mockReset()
  fetchMock.mockImplementation(async () => new Response('{}', { status: 200 }))
  vi.stubGlobal('fetch', fetchMock)
  tokenStorage.set('a', 'r')
})

describe('oauthStartUrl', () => {
  it('points the browser at the start route, with the redirect encoded', () => {
    expect(oauthStartUrl('google')).toBe('/api/auth/oauth/google/start')
    expect(oauthStartUrl('google', null)).toBe('/api/auth/oauth/google/start')
    expect(oauthStartUrl('google', '/matches/m1?x=1&y=2')).toBe(
      '/api/auth/oauth/google/start?redirect=%2Fmatches%2Fm1%3Fx%3D1%26y%3D2',
    )
    expect(oauthStartUrl('telegram')).toBe('/api/auth/oauth/telegram/start')
  })
})

describe('oauth endpoints', () => {
  it('link is authenticated and sends redirect and lang as given', async () => {
    fetchMock.mockImplementation(async () => new Response('{"url":"https://g/x"}', { status: 200 }))
    await expect(oauthLink('google', { redirect: '/settings', lang: 'en' })).resolves.toEqual({
      url: 'https://g/x',
    })
    await oauthLink('google', {})
    expect(calls()).toEqual([
      {
        url: '/api/auth/oauth/google/link',
        method: 'POST',
        auth: 'Bearer a',
        body: { redirect: '/settings', lang: 'en' },
        credentials: undefined,
      },
      {
        url: '/api/auth/oauth/google/link',
        method: 'POST',
        auth: 'Bearer a',
        body: {},
        credentials: undefined,
      },
    ])
  })

  it('exchange and signup are anonymous and keep the default same-origin cookies', async () => {
    await oauthExchange('c1')
    fetchMock.mockImplementationOnce(async () => new Response('{}', { status: 201 }))
    await oauthSignup({ token: 'st', username: 'bob' })
    expect(calls()).toEqual([
      {
        url: '/api/auth/oauth/exchange',
        method: 'POST',
        auth: undefined,
        body: { code: 'c1' },
        credentials: undefined,
      },
      {
        url: '/api/auth/oauth/signup',
        method: 'POST',
        auth: undefined,
        body: { token: 'st', username: 'bob' },
        credentials: undefined,
      },
    ])
  })

  it('surfaces a spent code as invalid_token', async () => {
    fetchMock.mockImplementationOnce(
      async () =>
        new Response(JSON.stringify({ error: { code: 'invalid_token', message: 'x' } }), {
          status: 400,
        }),
    )
    await expect(oauthExchange('c1')).rejects.toMatchObject({ code: 'invalid_token', status: 400 })
  })
})
