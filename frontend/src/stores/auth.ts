import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import * as authApi from '@/api/auth'
import { setAuthHooks } from '@/api/client'
import { tokenStorage } from '@/api/tokens'
import type { Credentials, Tokens, User } from '@/api/types'
import { mailLang } from '@/i18n'
import { normalizeEmail, normalizeUsername } from '@/utils/validation'

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

  async function login(c: Credentials) {
    // A new attempt (maybe another account) must show its own outcome, not the old ban.
    suspended.value = false
    applyTokens(await authApi.login({ ...c, username: normalizeUsername(c.username) }))
  }

  async function register(c: Credentials) {
    suspended.value = false
    applyTokens(await authApi.register({ ...c, username: normalizeUsername(c.username) }))
  }

  /** Whether the server can send login emails; false until known, so the option never flashes. */
  const emailEnabled = ref(false)

  async function loadProviders() {
    try {
      emailEnabled.value = (await authApi.getProviders()).email
    } catch {
      emailEnabled.value = false
    }
  }

  async function emailStart(email: string) {
    await authApi.emailStart({ email: normalizeEmail(email), lang: mailLang() })
  }

  const emailPreview = (token: string) => authApi.emailPreview(token)

  async function emailVerify(token: string) {
    suspended.value = false
    applyTokens(await authApi.emailVerify(token))
  }

  async function emailSignup(token: string, username: string) {
    suspended.value = false
    applyTokens(await authApi.emailSignup({ token, username: normalizeUsername(username) }))
  }

  async function logout() {
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
    loadProviders,
    emailStart,
    emailPreview,
    emailVerify,
    emailSignup,
    login,
    register,
    logout,
    clear,
    markBanned,
  }
})
