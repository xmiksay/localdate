import { describe, expect, it } from 'vitest'
import { parseOAuthFragment } from './oauthFragment'

describe('parseOAuthFragment', () => {
  it('reads a one-time code with its redirect', () => {
    expect(parseOAuthFragment('#code=abc_-1&redirect=%2Fmatches%2Fm1')).toEqual({
      kind: 'code',
      code: 'abc_-1',
      redirect: '/matches/m1',
    })
    expect(parseOAuthFragment('code=abc')).toEqual({ kind: 'code', code: 'abc', redirect: null })
  })

  it('drops a redirect that would leave the app', () => {
    for (const bad of ['%2F%2Fevil.com', 'https%3A%2F%2Fevil.com', '%2F%5Cevil.com', 'settings']) {
      expect(parseOAuthFragment(`#code=abc&redirect=${bad}`)).toEqual({
        kind: 'code',
        code: 'abc',
        redirect: null,
      })
    }
  })

  it('reads a finished link', () => {
    expect(parseOAuthFragment('#linked=google&redirect=%2Fsettings')).toEqual({
      kind: 'linked',
      provider: 'google',
      redirect: '/settings',
    })
  })

  it('reads known error codes and maps the rest to unknown', () => {
    expect(parseOAuthFragment('#error=identity_taken')).toEqual({
      kind: 'error',
      error: 'identity_taken',
      redirect: null,
    })
    expect(parseOAuthFragment('#error=teapot')).toMatchObject({ error: 'unknown' })
    expect(parseOAuthFragment('#error=')).toMatchObject({ error: 'unknown' })
  })

  it('is none without a usable key', () => {
    for (const h of ['', '#', '#code=', '#linked=facebook', '#token=abc']) {
      expect(parseOAuthFragment(h)).toEqual({ kind: 'none' })
    }
  })
})
