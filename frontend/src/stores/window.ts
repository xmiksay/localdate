import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import * as windowApi from '@/api/window'
import type { Window, WindowDuration, WindowMinutes } from '@/api/types'
import type { Coords } from '@/utils/geo'

export const useWindowStore = defineStore('window', () => {
  const current = ref<Window | null>(null)
  const now = ref(Date.now())
  const loaded = ref(false)
  /** Set by the location sharing composable, shown by the nearby view. */
  const locationError = ref<'denied' | 'unavailable' | null>(null)
  /** Why the window ended without the user ending it; shown until dismissed or the next start. */
  const endedNotice = ref<'left_area' | null>(null)
  let timer: ReturnType<typeof setInterval> | undefined

  const isActive = computed(() => current.value !== null)
  const wavesLeft = computed(() => current.value?.waves_left ?? 0)
  const remainingMs = computed(() =>
    current.value ? Math.max(0, Date.parse(current.value.ends_at) - now.value) : 0,
  )

  function stopTick() {
    clearInterval(timer)
    timer = undefined
  }

  function tick() {
    now.value = Date.now()
    if (current.value && remainingMs.value === 0) clear()
  }

  function set(w: Window | null) {
    current.value = w
    now.value = Date.now()
    if (!w) return stopTick()
    if (Date.parse(w.ends_at) <= now.value) return clear()
    timer ??= setInterval(tick, 1000)
  }

  function clear() {
    set(null)
    locationError.value = null
  }

  async function load() {
    set(await windowApi.getWindow())
    loaded.value = true
  }

  /** The server already ended the area window (`409 left_area` on a location update). */
  function leftArea() {
    clear()
    endedNotice.value = 'left_area'
  }

  function dismissNotice() {
    endedNotice.value = null
  }

  /** Session end: nothing of the previous user's window may linger. */
  function reset() {
    clear()
    dismissNotice()
  }

  async function start(duration: WindowDuration, at: Coords, areaId?: string) {
    set(await windowApi.startWindow(duration, at.lat, at.lon, areaId))
    dismissNotice()
  }

  async function extend(minutes: WindowMinutes) {
    set(await windowApi.extendWindow(minutes))
  }

  async function end() {
    await windowApi.endWindow()
    clear()
  }

  return {
    current,
    loaded,
    locationError,
    endedNotice,
    isActive,
    wavesLeft,
    remainingMs,
    load,
    start,
    extend,
    end,
    clear,
    leftArea,
    dismissNotice,
    reset,
  }
})
