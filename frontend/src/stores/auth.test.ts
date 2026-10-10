import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import * as authApi from '@/api/auth'
import * as meApi from '@/api/me'
import * as oauthApi from '@/api/oauth'
import { ApiError, signalBanned } from '@/api/client'
import { tokenStorage } from '@/api/tokens'
import { useAuthStore } from './auth'
import { usePushStore } from './push'

vi.mock('@/api/auth')
vi.mock('@/api/me')
vi.mock('@/api/oauth')

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
    expect(authApi.login).toHaveBeenCalledWith({ username: 'Bob', password: 'xxxxxxxxxx' })
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
    // Anonymous calls do not signal bans in the client; the store flags it itself.
    vi.mocked(authApi.login).mockRejectedValue(new ApiError('banned', 403, 'banned'))
    const s = useAuthStore()
    await expect(s.login({ username: 'bob', password: 'x'.repeat(10) })).rejects.toThrow()
    expect(s.isAuthed).toBe(false)
    expect(s.suspended).toBe(true)
  })

  it('email sign-in refused as banned is flagged too, other failures are not', async () => {
    vi.mocked(authApi.emailVerify).mockRejectedValueOnce(new ApiError('banned', 403, 'banned'))
    const s = useAuthStore()
    await expect(s.emailVerify('tok')).rejects.toThrow()
    expect(s.suspended).toBe(true)
    vi.mocked(authApi.emailSignup).mockRejectedValueOnce(new ApiError('username_taken', 409, 'x'))
    await expect(s.emailSignup('tok', 'bob')).rejects.toThrow()
    expect(s.suspended).toBe(false)
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
    expect(s.oauthProviders).toEqual([])
    vi.mocked(authApi.getProviders).mockResolvedValue({
      email: true,
      google: true,
      telegram: true,
      facebook: true,
      password_reset: true,
    })
    await s.loadProviders()
    expect(s.emailEnabled).toBe(true)
    expect(s.oauthProviders).toEqual(['google', 'telegram', 'facebook'])
    expect(s.passwordResetEnabled).toBe(true)
    expect(s.resetByUsernameOnly).toBe(false)
    vi.mocked(authApi.getProviders).mockResolvedValue({
      email: false,
      google: false,
      telegram: true,
      facebook: false,
      password_reset: true,
    })
    await s.loadProviders()
    expect(s.oauthProviders).toEqual(['telegram'])
    expect(s.resetByUsernameOnly).toBe(true)
    vi.mocked(authApi.getProviders).mockResolvedValue({
      email: false,
      google: false,
      telegram: false,
      facebook: false,
      password_reset: false,
    })
    await s.loadProviders()
    expect(s.oauthProviders).toEqual([])
    expect(s.passwordResetEnabled).toBe(false)
    expect(s.resetByUsernameOnly).toBe(false)
    vi.mocked(authApi.getProviders).mockRejectedValue(new Error('offline'))
    await s.loadProviders()
    expect(s.emailEnabled).toBe(false)
    expect(s.oauthProviders).toEqual([])
    expect(s.passwordResetEnabled).toBe(false)
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
    expect(authApi.emailSignup).toHaveBeenCalledWith({ token: 'tok', username: 'Bob' })
    expect(s.isAuthed).toBe(true)
  })

  it('passwordForgot trims the login, keeps its case and sends the UI language', async () => {
    vi.mocked(authApi.passwordForgot).mockResolvedValue(undefined)
    const s = useAuthStore()
    await s.passwordForgot(' Bob ')
    await s.passwordForgot(' Bob@Example.CZ ')
    expect(vi.mocked(authApi.passwordForgot).mock.calls).toEqual([
      [{ login: 'Bob', lang: 'cs' }],
      [{ login: 'Bob@Example.CZ', lang: 'cs' }],
    ])
  })

  it('passwordReset ends the local session, which the server just revoked', async () => {
    vi.mocked(authApi.login).mockResolvedValue(tokens)
    vi.mocked(authApi.passwordReset).mockResolvedValue(undefined)
    const s = useAuthStore()
    await s.login({ username: 'bob', password: 'x'.repeat(10) })
    await s.passwordReset('tok', 'n'.repeat(10), 'bob')
    expect(authApi.passwordReset).toHaveBeenCalledWith({ token: 'tok', new_password: 'nnnnnnnnnn' })
    expect(s.isAuthed).toBe(false)
    expect(tokenStorage.refresh()).toBeNull()
  })

  it('a refused passwordReset keeps the session', async () => {
    vi.mocked(authApi.login).mockResolvedValue(tokens)
    vi.mocked(authApi.passwordReset).mockRejectedValue(new ApiError('invalid_token', 400, 'x'))
    const s = useAuthStore()
    await s.login({ username: 'bob', password: 'x'.repeat(10) })
    await expect(s.passwordReset('tok', 'n'.repeat(10), 'bob')).rejects.toThrow()
    expect(s.isAuthed).toBe(true)
  })

  it("resetting another account's password keeps this session", async () => {
    vi.mocked(authApi.login).mockResolvedValue(tokens)
    vi.mocked(authApi.passwordReset).mockResolvedValue(undefined)
    const s = useAuthStore()
    await s.login({ username: 'bob', password: 'x'.repeat(10) })
    await s.passwordReset('tok', 'n'.repeat(10), 'alice')
    expect(s.isAuthed).toBe(true)
    expect(tokenStorage.refresh()).toBe('r1')
  })

  it('changePassword swaps in the fresh session the server returns', async () => {
    vi.mocked(meApi.putPassword).mockResolvedValue({
      ...tokens,
      access_token: 'a2',
      refresh_token: 'r2',
    })
    const s = useAuthStore()
    await s.changePassword('n'.repeat(10), 'old password')
    await s.changePassword('m'.repeat(10))
    expect(vi.mocked(meApi.putPassword).mock.calls).toEqual([
      [{ current_password: 'old password', new_password: 'nnnnnnnnnn' }],
      [{ current_password: undefined, new_password: 'mmmmmmmmmm' }],
    ])
    expect(tokenStorage.access()).toBe('a2')
    expect(tokenStorage.refresh()).toBe('r2')
  })

  it('changePassword re-registers this device for push with the fresh session', async () => {
    vi.mocked(meApi.putPassword).mockResolvedValue({ ...tokens, access_token: 'a2' })
    const resync = vi.spyOn(usePushStore(), 'resync').mockImplementation(async () => {
      // Runs with the new session: the server dropped every subscription of the account.
      expect(tokenStorage.access()).toBe('a2')
    })
    await useAuthStore().changePassword('n'.repeat(10), 'old password')
    expect(resync).toHaveBeenCalledWith('cs')
  })

  it('a failed resync does not fail the password change', async () => {
    vi.mocked(meApi.putPassword).mockResolvedValue(tokens)
    vi.spyOn(usePushStore(), 'resync').mockRejectedValue(new Error('offline'))
    await expect(useAuthStore().changePassword('n'.repeat(10))).resolves.toBeUndefined()
  })

  it('oauthExchange logs a known account in and clears an old ban notice', async () => {
    vi.mocked(oauthApi.oauthExchange).mockResolvedValue({ session: tokens })
    const s = useAuthStore()
    s.markBanned()
    await expect(s.oauthExchange('code')).resolves.toBeNull()
    expect(oauthApi.oauthExchange).toHaveBeenCalledWith('code')
    expect(s.isAuthed).toBe(true)
    expect(s.suspended).toBe(false)
    expect(tokenStorage.refresh()).toBe('r1')
  })

  it('oauthExchange hands back the sign-up token for a new account without a session', async () => {
    vi.mocked(oauthApi.oauthExchange).mockResolvedValue({
      signup: { token: 'st', provider: 'google', expires_at: '2026-10-08T10:15:00Z' },
    })
    const s = useAuthStore()
    await expect(s.oauthExchange('code')).resolves.toEqual({ signupToken: 'st' })
    expect(s.isAuthed).toBe(false)
  })

  it('oauthExchange rejected as banned leaves the store suspended', async () => {
    // Anonymous calls do not signal bans themselves: signIn() must mark it.
    vi.mocked(oauthApi.oauthExchange).mockRejectedValue(new ApiError('banned', 403, 'banned'))
    const s = useAuthStore()
    await expect(s.oauthExchange('code')).rejects.toMatchObject({ code: 'banned' })
    expect(s.suspended).toBe(true)
  })

  it('oauthSignup normalizes the username and stores tokens', async () => {
    vi.mocked(oauthApi.oauthSignup).mockResolvedValue(tokens)
    const s = useAuthStore()
    await s.oauthSignup('st', ' Bob ')
    expect(oauthApi.oauthSignup).toHaveBeenCalledWith({ token: 'st', username: 'Bob' })
    expect(s.isAuthed).toBe(true)
  })
})
