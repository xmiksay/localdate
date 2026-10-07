import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import * as windowApi from '@/api/window'
import type { Window, WindowMinutes } from '@/api/types'
import type { Coords } from '@/utils/geo'

export const useWindowStore = defineStore('window', () => {
  const current = ref<Window | null>(null)
  const now = ref(Date.now())
  const loaded = ref(false)
  /** Set by the location sharing composable, shown by the nearby view. */
  const locationError = ref<'denied' | 'unavailable' | null>(null)
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

  async function start(minutes: WindowMinutes, at: Coords) {
    set(await windowApi.startWindow(minutes, at.lat, at.lon))
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
    isActive,
    wavesLeft,
    remainingMs,
    load,
    start,
    extend,
    end,
    clear,
  }
})
