import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import * as adminApi from '@/api/admin'
import { setImpersonationHook, type ImpersonationLoss } from '@/api/client'
import {
  ADMIN_ACCESS_KEY,
  impersonationStorage,
  tokenStorage,
  type StoredImpersonation,
} from '@/api/tokens'
import type { User } from '@/api/types'
import router from '@/router'
import { useAuthStore } from './auth'
import { useMatchesStore } from './matches'
import { useMeStore } from './me'
import { resetUserStores } from './session'
import { useWindowStore } from './window'

/** An admin using the app as a test user ("act as"); see docs/api/admin.md. */
export const useImpersonationStore = defineStore('impersonation', () => {
  const active = ref<StoredImpersonation | null>(null)
  /** Why the last impersonation ended on its own; shown until dismissed. */
  const notice = ref<ImpersonationLoss | null>(null)
  const isActive = computed(() => active.value !== null)
  const target = computed(() => active.value?.user ?? null)
  let timer: ReturnType<typeof setTimeout> | undefined

  function arm() {
    clearTimeout(timer)
    const a = active.value
    if (!a) return
    timer = setTimeout(
      () => void end('expired'),
      Math.max(0, Date.parse(a.expires_at) - Date.now()),
    )
  }

  /** Nothing of the previous identity may linger: its stores, socket and screen. */
  async function switchTo(token: string | null, user: User | null, route: 'nearby' | 'admin') {
    resetUserStores()
    const auth = useAuthStore()
    auth.accessToken = token
    auth.user = user
    // No admin session left (logged out in another tab): the app-level auth watcher takes over.
    if (!token) return
    // Loaded here, not by the route guard: a navigation to the current route skips the guard.
    await useMeStore()
      .load()
      .catch(() => undefined)
    await router.replace({ name: route }).catch(() => undefined)
    void useWindowStore()
      .load()
      .catch(() => undefined)
    void useMatchesStore()
      .loadMatches()
      .catch(() => undefined)
  }

  async function start(userId: string) {
    const imp = await adminApi.impersonate(userId)
    const stored: StoredImpersonation = { ...imp, admin: useAuthStore().user }
    impersonationStorage.set(stored)
    active.value = stored
    notice.value = null
    arm()
    await switchTo(imp.access_token, imp.user, 'nearby')
  }

  /** Back to the admin's own session; `reason` when it ended without the admin asking. */
  async function end(reason: ImpersonationLoss | null = null) {
    const a = active.value
    // The expiry timer and the API client's 401 may both report the same end.
    if (!a) return
    clearTimeout(timer)
    impersonationStorage.clear()
    active.value = null
    notice.value = reason
    await switchTo(tokenStorage.access(), a.admin, 'admin')
  }

  const stop = () => end()

  function dismissNotice() {
    notice.value = null
  }

  /** The admin's own session ended (logout, auth loss); its storage is already gone. */
  function reset() {
    clearTimeout(timer)
    active.value = null
    notice.value = null
  }

  // Restore after a reload; one that ran out meanwhile just falls back to the admin session.
  const stored = impersonationStorage.get()
  if (stored && Date.parse(stored.expires_at) > Date.now()) {
    active.value = stored
    arm()
  } else if (stored) {
    impersonationStorage.clear()
    notice.value = 'expired'
  }
  setImpersonationHook((reason) => void end(reason))

  // The admin logged out in another tab: this tab's sessionStorage would otherwise keep acting
  // on a token the server still honours until it expires.
  window.addEventListener('storage', (e) => {
    const adminGone = e.key === null || (e.key === ADMIN_ACCESS_KEY && e.newValue === null)
    if (!adminGone || !active.value) return
    reset()
    impersonationStorage.clear()
    const auth = useAuthStore()
    auth.accessToken = null
    auth.user = null
  })

  return { active, notice, isActive, target, start, stop, end, dismissNotice, reset }
})
