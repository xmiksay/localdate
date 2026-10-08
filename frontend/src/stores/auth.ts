import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import * as authApi from '@/api/auth'
import * as meApi from '@/api/me'
import * as oauthApi from '@/api/oauth'
import { ApiError, setAuthHooks } from '@/api/client'
import { tokenStorage } from '@/api/tokens'
import {
  OAUTH_PROVIDERS,
  type Credentials,
  type OAuthProvider,
  type Tokens,
  type User,
} from '@/api/types'
import { i18n, mailLang } from '@/i18n'
import { normalizeEmail, normalizeLogin, normalizeUsername } from '@/utils/validation'
import { toPushLang } from '@/utils/webPush'
import { usePushStore } from './push'

export const useAuthStore = defineStore('auth', () => {
  const accessToken = ref<string | null>(tokenStorage.access())
  const user = ref<User | null>(null)
  const isAuthed = computed(() => accessToken.value !== null)
  /** Set when the server reported the account as banned; shown on the login screen. */
  const suspended = ref(false)

  function applyTokens(t: Tokens) {
    suspended.value = false
    tokenStorage.set(t.access_token, t.refresh_token)
    accessToken.value = t.access_token
    user.value = t.user
  }

  function clear() {
    suspended.value = false
    tokenStorage.clear()
    accessToken.value = null
    user.value = null
  }

  /** Tokens are already gone (the API client clears them before calling this). */
  function markBanned() {
    clear()
    suspended.value = true
  }

  setAuthHooks({ onTokens: applyTokens, onAuthLost: clear, onBanned: markBanned })

  /**
   * Starts a session from an anonymous call. The API client does not signal bans for anonymous
   * requests, so a sign-in refused as banned is flagged here, for the notice on the login screen.
   * `call` may answer `null`: no session yet (an OAuth exchange that leads to the username step).
   */
  async function signIn(call: () => Promise<Tokens | null>) {
    // A new attempt (maybe another account) must show its own outcome, not the old ban.
    suspended.value = false
    try {
      const tokens = await call()
      if (tokens) applyTokens(tokens)
    } catch (e) {
      if (e instanceof ApiError && e.code === 'banned') markBanned()
      throw e
    }
  }

  const login = (c: Credentials) =>
    signIn(() => authApi.login({ ...c, username: normalizeUsername(c.username) }))

  const register = (c: Credentials) =>
    signIn(() => authApi.register({ ...c, username: normalizeUsername(c.username) }))

  /** Login methods the server offers; false until known, so an option never flashes. */
  const emailEnabled = ref(false)
  /** Enabled OAuth providers, in `OAUTH_PROVIDERS` order. */
  const oauthProviders = ref<OAuthProvider[]>([])

  async function loadProviders() {
    try {
      const p = await authApi.getProviders()
      emailEnabled.value = p.email
      oauthProviders.value = OAUTH_PROVIDERS.filter((o) => p[o] === true)
    } catch {
      emailEnabled.value = false
      oauthProviders.value = []
    }
  }

  async function emailStart(email: string) {
    await authApi.emailStart({ email: normalizeEmail(email), lang: mailLang() })
  }

  const emailPreview = (token: string) => authApi.emailPreview(token)

  const emailVerify = (token: string) => signIn(() => authApi.emailVerify(token))

  const emailSignup = (token: string, username: string) =>
    signIn(() => authApi.emailSignup({ token, username: normalizeUsername(username) }))

  async function passwordForgot(login: string) {
    await authApi.passwordForgot({ login: normalizeLogin(login), lang: mailLang() })
  }

  const passwordResetPreview = (token: string) => authApi.passwordResetPreview(token)

  /**
   * Every session of the reset account ends on the server; drop the local one only if it is that
   * account (`username` from the preview), not when the link was for someone else.
   */
  async function passwordReset(token: string, newPassword: string, username: string) {
    await authApi.passwordReset({ token, new_password: newPassword })
    if (user.value?.username === username) clear()
  }

  /** The server revokes every session and hands this one fresh tokens. */
  async function changePassword(newPassword: string, currentPassword?: string) {
    applyTokens(
      await meApi.putPassword({ current_password: currentPassword, new_password: newPassword }),
    )
    // The change deleted every push subscription of the account; put this device's back.
    void usePushStore()
      .resync(toPushLang(i18n.global.locale.value))
      .catch(() => undefined)
  }

  /** Logs a known account in; for a new one returns the sign-up token to pick a username with. */
  async function oauthExchange(code: string): Promise<{ signupToken: string } | null> {
    let signupToken: string | null = null
    await signIn(async () => {
      const r = await oauthApi.oauthExchange(code)
      if ('session' in r) return r.session
      signupToken = r.signup.token
      return null
    })
    return signupToken === null ? null : { signupToken }
  }

  const oauthSignup = (token: string, username: string) =>
    signIn(() => oauthApi.oauthSignup({ token, username: normalizeUsername(username) }))

  async function logout() {
    // While the tokens still work: afterwards the server could not be told to forget this device.
    await usePushStore().forgetDevice()
    const refresh = tokenStorage.refresh()
    clear()
    if (refresh) await authApi.logout(refresh).catch(() => undefined)
  }

  return {
    accessToken,
    user,
    isAuthed,
    suspended,
    emailEnabled,
    oauthProviders,
    loadProviders,
    emailStart,
    emailPreview,
    emailVerify,
    emailSignup,
    passwordForgot,
    passwordResetPreview,
    passwordReset,
    changePassword,
    oauthExchange,
    oauthSignup,
    login,
    register,
    logout,
    clear,
    markBanned,
  }
})
