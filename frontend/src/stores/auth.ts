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

  function applyTokens(t: Tokens) {
    tokenStorage.set(t.access_token, t.refresh_token)
    accessToken.value = t.access_token
    user.value = t.user
  }

  function clear() {
    tokenStorage.clear()
    accessToken.value = null
    user.value = null
  }

  setAuthHooks({ onTokens: applyTokens, onAuthLost: clear })

  async function login(c: Credentials) {
    applyTokens(await authApi.login({ ...c, username: normalizeUsername(c.username) }))
  }

  async function register(c: Credentials) {
    applyTokens(await authApi.register({ ...c, username: normalizeUsername(c.username) }))
  }

  async function logout() {
    const refresh = tokenStorage.refresh()
    clear()
    if (refresh) await authApi.logout(refresh).catch(() => undefined)
  }

  return { accessToken, user, isAuthed, login, register, logout, clear }
})
