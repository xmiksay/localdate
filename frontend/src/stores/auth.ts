import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import * as authApi from '@/api/auth'
import { setAuthHooks } from '@/api/client'
import { tokenStorage } from '@/api/tokens'
import type { Credentials, Tokens, User } from '@/api/types'
import { normalizeUsername } from '@/utils/validation'

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
    login,
    register,
    logout,
    clear,
    markBanned,
  }
})
