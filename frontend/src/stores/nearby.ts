import { ref } from 'vue'
import { defineStore } from 'pinia'
import * as windowApi from '@/api/window'
import * as socialApi from '@/api/social'
import { ApiError } from '@/api/client'
import type { NearbyProfile, WaveResult, WaveState } from '@/api/types'
import { byOverlapThenBand } from '@/utils/interests'
import { useWindowStore } from './window'

export const useNearbyStore = defineStore('nearby', () => {
  const people = ref<NearbyProfile[]>([])
  const incoming = ref<NearbyProfile[]>([])
  const loaded = ref(false)

  const find = (userId: string) =>
    people.value.find((p) => p.user_id === userId) ??
    incoming.value.find((p) => p.user_id === userId)

  async function loadIncoming() {
    incoming.value = await socialApi.getIncomingWaves()
  }

  async function refresh() {
    const win = useWindowStore()
    try {
      const [nearby, waves] = await Promise.all([
        windowApi.getNearby(),
        socialApi.getIncomingWaves(),
      ])
      people.value = nearby
      incoming.value = waves
      loaded.value = true
    } catch (e) {
      // Window ended server-side (e.g. on another device): resync instead of showing a dead list.
      if (e instanceof ApiError && e.code === 'no_active_window') win.clear()
      throw e
    }
  }

  function removeUser(userId: string) {
    people.value = people.value.filter((p) => p.user_id !== userId)
    incoming.value = incoming.value.filter((p) => p.user_id !== userId)
  }

  function applyWave(userId: string, r: WaveResult) {
    const state: WaveState = r.matched ? 'matched' : 'sent'
    const patch = (p: NearbyProfile) =>
      p.user_id === userId ? { ...p, wave_state: state, match_id: r.match_id } : p
    people.value = people.value.map(patch)
    // A matched or answered wave no longer belongs in "incoming".
    incoming.value = incoming.value.filter((p) => p.user_id !== userId)
  }

  async function wave(userId: string): Promise<WaveResult> {
    const win = useWindowStore()
    // Resolve from either list now: waving back removes the sender from `incoming`.
    const known = find(userId)
    try {
      const r = await socialApi.sendWave(userId)
      if (known && !people.value.some((p) => p.user_id === userId)) {
        people.value = [...people.value, { ...known }].sort(byOverlapThenBand)
      }
      applyWave(userId, r)
      await win.load().catch(() => undefined)
      return r
    } catch (e) {
      if (e instanceof ApiError && e.code === 'not_visible') removeUser(userId)
      if (e instanceof ApiError && e.code === 'no_active_window') win.clear()
      throw e
    }
  }

  function reset() {
    people.value = []
    incoming.value = []
    loaded.value = false
  }

  return { people, incoming, loaded, find, refresh, loadIncoming, removeUser, wave, reset }
})
