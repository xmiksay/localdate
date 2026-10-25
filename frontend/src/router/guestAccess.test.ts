import { describe, expect, it } from 'vitest'
import type { RouteLocationNormalized } from 'vue-router'
import router, { guestAccess } from './index'

const resolved = (path: string) => router.resolve(path) as unknown as RouteLocationNormalized

describe('guestAccess', () => {
  it('opens the guest preview for the bare root', () => {
    // What navigation hands the guard after following the `/` record's redirect.
    const to = { ...resolved('/nearby'), redirectedFrom: resolved('/') }
    expect(guestAccess(to)).toEqual({ name: 'preview' })
    expect(guestAccess(resolved('/nearby'))).toMatchObject({ name: 'login' })
  })

  it('sends deep links to login with a way back', () => {
    expect(guestAccess(resolved('/matches'))).toEqual({
      name: 'login',
      query: { redirect: '/matches' },
    })
  })

  it('lets public routes through', () => {
    expect(guestAccess(resolved('/preview'))).toBe(true)
    expect(guestAccess(resolved('/login'))).toBe(true)
  })
})
