import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import * as adminApi from '@/api/admin'
import { get } from '@/api/client'
import * as meApi from '@/api/me'
import * as socialApi from '@/api/social'
import * as windowApi from '@/api/window'
import { impersonationStorage, tokenStorage } from '@/api/tokens'
import type { Impersonation, MeResponse, User } from '@/api/types'
import router from '@/router'
import { useAuthStore } from './auth'
import { useImpersonationStore } from './impersonation'
import { useMeStore } from './me'

vi.mock('@/api/admin')
vi.mock('@/api/me')
vi.mock('@/api/social')
vi.mock('@/api/window')
vi.mock('@/router', () => ({ default: { replace: vi.fn() } }))

const T0 = new Date('2026-10-10T12:00:00Z')
const admin: User = { id: 'adm', username: 'admin', created_at: '' }
const target: User = { id: 'tst', username: 'tester', created_at: '' }
const imp = (minutes = 60): Impersonation => ({
  access_token: 'imp-a',
  expires_at: new Date(T0.getTime() + minutes * 60_000).toISOString(),
  user: target,
})
const meOf = (user: User, is_admin: boolean): MeResponse => ({
  user,
  profile: null,
  filter: null,
  is_admin,
})
const json = (status: number, body: unknown) => new Response(JSON.stringify(body), { status })
const fetchMock = vi.fn()

async function acting() {
  const auth = useAuthStore()
  auth.user = admin
  const s = useImpersonationStore()
  vi.mocked(adminApi.impersonate).mockResolvedValue(imp())
  vi.mocked(meApi.getMe).mockResolvedValue(meOf(target, false))
  await s.start('tst')
  return { s, auth }
}

beforeEach(() => {
  vi.useFakeTimers()
  vi.setSystemTime(T0)
  localStorage.clear()
  sessionStorage.clear()
  tokenStorage.set('admin-a', 'admin-r')
  setActivePinia(createPinia())
  vi.resetAllMocks()
  vi.mocked(router.replace).mockResolvedValue(undefined)
  vi.mocked(windowApi.getWindow).mockResolvedValue(null)
  vi.mocked(socialApi.getMatches).mockResolvedValue([])
  fetchMock.mockReset()
  vi.stubGlobal('fetch', fetchMock)
})
afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

describe('impersonation store', () => {
  it('start puts the target token in front of the admin session, which stays stored', async () => {
    const { s, auth } = await acting()
    expect(adminApi.impersonate).toHaveBeenCalledWith('tst')
    expect(s.isActive).toBe(true)
    expect(s.target).toEqual(target)
    expect(tokenStorage.access()).toBe('imp-a')
    expect(tokenStorage.refresh()).toBeNull()
    expect(localStorage.getItem('localdate.access')).toBe('admin-a')
    expect(localStorage.getItem('localdate.refresh')).toBe('admin-r')
    expect(impersonationStorage.get()).toMatchObject({ access_token: 'imp-a', admin })
    expect(auth.accessToken).toBe('imp-a')
    expect(auth.user).toEqual(target)
    expect(router.replace).toHaveBeenCalledWith({ name: 'nearby' })
    expect(useMeStore().isAdmin).toBe(false)
  })

  it('a refused start changes nothing', async () => {
    const s = useImpersonationStore()
    vi.mocked(adminApi.impersonate).mockRejectedValue(new Error('409'))
    await expect(s.start('tst')).rejects.toThrow()
    expect(s.isActive).toBe(false)
    expect(tokenStorage.access()).toBe('admin-a')
    expect(router.replace).not.toHaveBeenCalled()
  })

  it('stop restores the admin session and resets per-user state', async () => {
    const { s, auth } = await acting()
    vi.mocked(meApi.getMe).mockResolvedValue(meOf(admin, true))
    await s.stop()
    expect(s.isActive).toBe(false)
    expect(s.notice).toBeNull()
    expect(sessionStorage.length).toBe(0)
    expect(tokenStorage.access()).toBe('admin-a')
    expect(tokenStorage.refresh()).toBe('admin-r')
    expect(auth.accessToken).toBe('admin-a')
    expect(auth.user).toEqual(admin)
    expect(useMeStore().isAdmin).toBe(true)
    expect(router.replace).toHaveBeenLastCalledWith({ name: 'admin' })
  })

  it('survives a reload: the next store picks the impersonation up from sessionStorage', async () => {
    await acting()
    setActivePinia(createPinia())
    const s = useImpersonationStore()
    expect(s.isActive).toBe(true)
    expect(s.target?.username).toBe('tester')
    expect(useAuthStore().accessToken).toBe('imp-a')
  })

  it('drops an impersonation that expired before the reload, with a notice', async () => {
    impersonationStorage.set({ ...imp(-1), admin })
    const s = useImpersonationStore()
    expect(s.isActive).toBe(false)
    expect(s.notice).toBe('expired')
    expect(tokenStorage.access()).toBe('admin-a')
  })

  it('returns to the admin automatically at expires_at', async () => {
    const { s, auth } = await acting()
    vi.mocked(meApi.getMe).mockResolvedValue(meOf(admin, true))
    await vi.advanceTimersByTimeAsync(60 * 60_000)
    expect(s.isActive).toBe(false)
    expect(s.notice).toBe('expired')
    expect(auth.accessToken).toBe('admin-a')
    expect(router.replace).toHaveBeenLastCalledWith({ name: 'admin' })
    s.dismissNotice()
    expect(s.notice).toBeNull()
  })

  it('a 401 while impersonating returns to the admin and never tries a refresh', async () => {
    const { s, auth } = await acting()
    vi.mocked(meApi.getMe).mockResolvedValue(meOf(admin, true))
    fetchMock.mockResolvedValue(json(401, { error: { code: 'unauthorized', message: '' } }))
    await expect(get('/nearby')).rejects.toMatchObject({ code: 'unauthorized' })
    await vi.advanceTimersByTimeAsync(0)
    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(fetchMock.mock.calls[0][1].headers.Authorization).toBe('Bearer imp-a')
    expect(s.isActive).toBe(false)
    expect(s.notice).toBe('expired')
    expect(auth.accessToken).toBe('admin-a')
    expect(localStorage.getItem('localdate.refresh')).toBe('admin-r')
  })

  it('a banned target ends the impersonation, not the admin session', async () => {
    const { s, auth } = await acting()
    vi.mocked(meApi.getMe).mockResolvedValue(meOf(admin, true))
    fetchMock.mockResolvedValue(json(403, { error: { code: 'banned', message: '' } }))
    await expect(get('/nearby')).rejects.toMatchObject({ code: 'banned' })
    await vi.advanceTimersByTimeAsync(0)
    expect(s.notice).toBe('banned')
    expect(auth.suspended).toBe(false)
    expect(auth.accessToken).toBe('admin-a')
  })

  it("the admin session ending (logout) ends the impersonation's storage too", async () => {
    const { s } = await acting()
    useAuthStore().clear()
    s.reset()
    expect(impersonationStorage.get()).toBeNull()
    expect(tokenStorage.access()).toBeNull()
    expect(s.isActive).toBe(false)
  })

  it('an admin logout in another tab ends the impersonation in this one', async () => {
    const { s, auth } = await acting()
    // What the other tab's logout leaves behind, and the event this tab then gets.
    localStorage.removeItem('localdate.access')
    localStorage.removeItem('localdate.refresh')
    window.dispatchEvent(
      new StorageEvent('storage', { key: 'localdate.access', oldValue: 'admin-a', newValue: null }),
    )
    expect(s.isActive).toBe(false)
    expect(impersonationStorage.get()).toBeNull()
    expect(tokenStorage.access()).toBeNull()
    expect(auth.accessToken).toBeNull()
  })

  it('an unrelated storage change in another tab is ignored', async () => {
    const { s } = await acting()
    window.dispatchEvent(
      new StorageEvent('storage', { key: 'localdate.refresh', oldValue: 'a', newValue: 'b' }),
    )
    window.dispatchEvent(
      new StorageEvent('storage', { key: 'localdate.access', oldValue: 'a', newValue: 'b' }),
    )
    expect(s.isActive).toBe(true)
    expect(tokenStorage.access()).toBe('imp-a')
  })
})
