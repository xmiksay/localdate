import { ref } from 'vue'
import { defineStore } from 'pinia'
import { ApiError } from '@/api/client'
import { updateLocation } from '@/api/window'
import type { WindowDuration } from '@/api/types'
import type { Coords } from '@/utils/geo'
import { useWindowStore } from './window'

/**
 * A position set by hand while impersonating (device location sharing is paused then). It stays
 * where it was put until set again.
 */
export const usePinnedLocationStore = defineStore('pinnedLocation', () => {
  const at = ref<Coords | null>(null)

  /** Moves the running window there, or starts a timed one there when none runs. */
  async function pin(next: Coords, duration: WindowDuration) {
    const win = useWindowStore()
    if (!win.isActive) {
      await win.start(duration, next)
      at.value = next
      return
    }
    const windowId = win.current?.id
    try {
      await updateLocation(next.lat, next.lon)
    } catch (e) {
      // A reply about a window since replaced must not end the new one.
      if (win.current?.id !== windowId || !(e instanceof ApiError)) throw e
      if (e.code === 'left_area') {
        win.leftArea()
        at.value = next
        throw e
      }
      if (e.code !== 'no_active_window') throw e
      // Ended server-side meanwhile: what was asked for is a window at this point.
      win.clear()
      await win.start(duration, next)
    }
    at.value = next
  }

  function reset() {
    at.value = null
  }

  return { at, pin, reset }
})
