import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import * as authApi from '@/api/auth'
import { ApiError, signalBanned } from '@/api/client'
import { tokenStorage } from '@/api/tokens'
import { useAuthStore } from './auth'

vi.mock('@/api/auth')

const tokens = {
  access_token: 'a1',
  refresh_token: 'r1',
  user: { id: 'u1', username: 'bob', created_at: '2026-01-01T00:00:00Z' },
}

beforeEach(() => {
  localStorage.clear()
  setActivePinia(createPinia())
  vi.resetAllMocks()
})

describe('auth store', () => {
  it('login stores tokens and normalizes username', async () => {
    vi.mocked(authApi.login).mockResolvedValue(tokens)
    const s = useAuthStore()
    await s.login({ username: ' Bob ', password: 'x'.repeat(10) })
    expect(authApi.login).toHaveBeenCalledWith({ username: 'bob', password: 'xxxxxxxxxx' })
    expect(s.isAuthed).toBe(true)
    expect(s.user?.username).toBe('bob')
    expect(tokenStorage.refresh()).toBe('r1')
  })

  it('register stores tokens', async () => {
    vi.mocked(authApi.register).mockResolvedValue(tokens)
    const s = useAuthStore()
    await s.register({ username: 'bob', password: 'x'.repeat(10) })
    expect(tokenStorage.access()).toBe('a1')
  })

  it('failed login leaves the store logged out', async () => {
    vi.mocked(authApi.login).mockRejectedValue(new Error('bad'))
    const s = useAuthStore()
    await expect(s.login({ username: 'bob', password: 'x' })).rejects.toThrow()
    expect(s.isAuthed).toBe(false)
  })

  it('logout clears state even if the server call fails', async () => {
    vi.mocked(authApi.login).mockResolvedValue(tokens)
    vi.mocked(authApi.logout).mockRejectedValue(new Error('offline'))
    const s = useAuthStore()
    await s.login({ username: 'bob', password: 'x'.repeat(10) })
    await s.logout()
    expect(authApi.logout).toHaveBeenCalledWith('r1')
    expect(s.isAuthed).toBe(false)
    expect(tokenStorage.access()).toBeNull()
  })

  it('a ban signalled by the API client logs out and flags the account suspended', async () => {
    vi.mocked(authApi.login).mockResolvedValue(tokens)
    const s = useAuthStore()
    await s.login({ username: 'bob', password: 'x'.repeat(10) })
    signalBanned()
    expect(s.isAuthed).toBe(false)
    expect(s.user).toBeNull()
    expect(s.suspended).toBe(true)
    expect(tokenStorage.access()).toBeNull()
  })

  it('login rejected as banned leaves the store suspended', async () => {
    vi.mocked(authApi.login).mockImplementation(async () => {
      // The real client signals before throwing; the API module is mocked here.
      signalBanned()
      throw new ApiError('banned', 403, 'banned')
    })
    const s = useAuthStore()
    await expect(s.login({ username: 'bob', password: 'x'.repeat(10) })).rejects.toThrow()
    expect(s.isAuthed).toBe(false)
    expect(s.suspended).toBe(true)
  })

  it('a successful login clears the suspended flag', async () => {
    const s = useAuthStore()
    s.markBanned()
    vi.mocked(authApi.login).mockResolvedValue(tokens)
    await s.login({ username: 'bob', password: 'x'.repeat(10) })
    expect(s.suspended).toBe(false)
  })

  it('a later failed login on another account clears the ban notice and shows its own error', async () => {
    const s = useAuthStore()
    signalBanned()
    expect(s.suspended).toBe(true)
    vi.mocked(authApi.login).mockRejectedValue(
      new ApiError('invalid_credentials', 401, 'invalid username or password'),
    )
    await expect(s.login({ username: 'eva', password: 'x'.repeat(10) })).rejects.toMatchObject({
      code: 'invalid_credentials',
    })
    expect(s.suspended).toBe(false)
  })

  it('logging out clears the ban notice', async () => {
    const s = useAuthStore()
    signalBanned()
    await s.logout()
    expect(s.suspended).toBe(false)
  })

  it('loadProviders reflects the server and treats failure as disabled', async () => {
    const s = useAuthStore()
    expect(s.emailEnabled).toBe(false)
    vi.mocked(authApi.getProviders).mockResolvedValue({ email: true })
    await s.loadProviders()
    expect(s.emailEnabled).toBe(true)
    vi.mocked(authApi.getProviders).mockRejectedValue(new Error('offline'))
    await s.loadProviders()
    expect(s.emailEnabled).toBe(false)
  })

  it('emailStart normalizes the address and sends the UI language', async () => {
    vi.mocked(authApi.emailStart).mockResolvedValue(undefined)
    await useAuthStore().emailStart('  Eva@Example.CZ ')
    expect(authApi.emailStart).toHaveBeenCalledWith({ email: 'eva@example.cz', lang: 'cs' })
  })

  it('emailPreview passes the token through without touching the session', async () => {
    const preview = { purpose: 'login' as const, username: 'bob', email: 'bob@example.cz' }
    vi.mocked(authApi.emailPreview).mockResolvedValue(preview)
    const s = useAuthStore()
    await expect(s.emailPreview('tok')).resolves.toEqual(preview)
    expect(authApi.emailPreview).toHaveBeenCalledWith('tok')
    expect(s.isAuthed).toBe(false)
  })

  it('emailVerify stores tokens and clears an old ban notice', async () => {
    vi.mocked(authApi.emailVerify).mockResolvedValue(tokens)
    const s = useAuthStore()
    s.markBanned()
    await s.emailVerify('tok')
    expect(s.isAuthed).toBe(true)
    expect(s.suspended).toBe(false)
    expect(tokenStorage.refresh()).toBe('r1')
  })

  it('emailVerify rejected as banned leaves the store suspended', async () => {
    vi.mocked(authApi.emailVerify).mockImplementation(async () => {
      signalBanned()
      throw new ApiError('banned', 403, 'banned')
    })
    const s = useAuthStore()
    await expect(s.emailVerify('tok')).rejects.toMatchObject({ code: 'banned' })
    expect(s.isAuthed).toBe(false)
    expect(s.suspended).toBe(true)
  })

  it('emailSignup normalizes the username and stores tokens', async () => {
    vi.mocked(authApi.emailSignup).mockResolvedValue(tokens)
    const s = useAuthStore()
    await s.emailSignup('tok', ' Bob ')
    expect(authApi.emailSignup).toHaveBeenCalledWith({ token: 'tok', username: 'bob' })
    expect(s.isAuthed).toBe(true)
  })
})
