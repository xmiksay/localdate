import { ref } from 'vue'
import { defineStore } from 'pinia'
import * as adminApi from '@/api/admin'
import * as areasApi from '@/api/areas'
import type { Area, AreaInput } from '@/api/types'
import { byActiveThenName } from '@/utils/areas'
import type { Coords } from '@/utils/geo'

export const useAreasStore = defineStore('areas', () => {
  /** Active areas containing the user's last fix, nearest first, for starting an area window. */
  const here = ref<Area[]>([])
  /** Every area, for the admin. */
  const all = ref<Area[]>([])
  const loading = ref(false)

  async function loadHere(at: Coords) {
    here.value = await areasApi.getAreas(at.lat, at.lon)
  }

  async function loadAll() {
    loading.value = true
    try {
      all.value = (await adminApi.getAdminAreas()).sort(byActiveThenName)
    } finally {
      loading.value = false
    }
  }

  /** Creates without `id`, replaces the area with it. */
  async function save(input: AreaInput, id?: string) {
    const area = id ? await adminApi.updateArea(id, input) : await adminApi.createArea(input)
    all.value = [...all.value.filter((a) => a.id !== area.id), area].sort(byActiveThenName)
    return area
  }

  async function remove(id: string) {
    await adminApi.deleteArea(id)
    all.value = all.value.filter((a) => a.id !== id)
  }

  function reset() {
    here.value = []
    all.value = []
    loading.value = false
  }

  return { here, all, loading, loadHere, loadAll, save, remove, reset }
})
