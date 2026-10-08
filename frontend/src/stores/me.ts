import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import * as meApi from '@/api/me'
import type { Filter, Interest, Photo, Profile, ProfileInput } from '@/api/types'
import { useAuthStore } from './auth'

export const DEFAULT_FILTER: Filter = {
  max_distance_m: 2000,
  genders: [],
  age_min: 18,
  age_max: 99,
  reasons: ['date', 'meet'],
  default_window_minutes: 60,
}

export const useMeStore = defineStore('me', () => {
  const profile = ref<Profile | null>(null)
  const filter = ref<Filter | null>(null)
  const photos = ref<Photo[]>([])
  const interests = ref<Interest[]>([])
  const loaded = ref(false)
  const isAdmin = ref(false)

  const isOnboarded = computed(
    () => profile.value !== null && photos.value.length > 0 && filter.value !== null,
  )

  async function load() {
    const me = await meApi.getMe()
    useAuthStore().user = me.user
    profile.value = me.profile
    photos.value = me.profile?.photos ?? []
    filter.value = me.filter
    isAdmin.value = me.is_admin
    loaded.value = true
  }

  async function loadInterests() {
    if (interests.value.length === 0) interests.value = await meApi.getInterests()
  }

  async function saveProfile(input: ProfileInput) {
    const p = await meApi.putProfile(input)
    profile.value = p
    photos.value = p.photos
  }

  async function saveFilter(f: Filter) {
    filter.value = await meApi.putFilter(f)
  }

  async function addPhoto(file: File) {
    photos.value = [...photos.value, await meApi.uploadPhoto(file)]
  }

  async function removePhoto(id: string) {
    await meApi.deletePhoto(id)
    photos.value = photos.value.filter((p) => p.id !== id).map((p, i) => ({ ...p, position: i }))
  }

  async function reorderPhotos(ids: string[]) {
    photos.value = await meApi.putPhotoOrder(ids)
  }

  function reset() {
    profile.value = null
    filter.value = null
    photos.value = []
    isAdmin.value = false
    loaded.value = false
  }

  return {
    profile,
    filter,
    photos,
    interests,
    loaded,
    isAdmin,
    isOnboarded,
    load,
    loadInterests,
    saveProfile,
    saveFilter,
    addPhoto,
    removePhoto,
    reorderPhotos,
    reset,
  }
})
