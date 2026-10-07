import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import * as meApi from '@/api/me'
import type { MeResponse, Profile } from '@/api/types'
import { DEFAULT_FILTER, useMeStore } from './me'

vi.mock('@/api/me')

const user = { id: 'u1', username: 'bob', created_at: '2026-01-01T00:00:00Z' }
const photo = { id: 'p1', url: '/media/p1.webp', position: 0 }
const profile: Profile = {
  display_name: 'Bob',
  birth_date: '1990-01-01',
  age: 36,
  gender: 'male',
  bio: '',
  interests: [],
  photos: [photo],
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.resetAllMocks()
})

async function loadWith(me: MeResponse) {
  vi.mocked(meApi.getMe).mockResolvedValue(me)
  const s = useMeStore()
  await s.load()
  return s
}

describe('me store onboarding status', () => {
  it('not onboarded without profile', async () => {
    expect((await loadWith({ user, profile: null, filter: null })).isOnboarded).toBe(false)
  })
  it('not onboarded without photos', async () => {
    const s = await loadWith({
      user,
      profile: { ...profile, photos: [] },
      filter: DEFAULT_FILTER,
    })
    expect(s.isOnboarded).toBe(false)
  })
  it('not onboarded without a saved filter', async () => {
    expect((await loadWith({ user, profile, filter: null })).isOnboarded).toBe(false)
  })
  it('onboarded with profile, photo and filter', async () => {
    expect((await loadWith({ user, profile, filter: DEFAULT_FILTER })).isOnboarded).toBe(true)
  })
  it('flips once the filter is saved', async () => {
    const s = await loadWith({ user, profile, filter: null })
    vi.mocked(meApi.putFilter).mockResolvedValue(DEFAULT_FILTER)
    await s.saveFilter(DEFAULT_FILTER)
    expect(s.isOnboarded).toBe(true)
  })
})

describe('me store photos', () => {
  it('compacts positions after removal', async () => {
    const s = await loadWith({
      user,
      profile: {
        ...profile,
        photos: [photo, { ...photo, id: 'p2', position: 1 }, { ...photo, id: 'p3', position: 2 }],
      },
      filter: null,
    })
    vi.mocked(meApi.deletePhoto).mockResolvedValue(undefined)
    await s.removePhoto('p1')
    expect(s.photos.map((p) => [p.id, p.position])).toEqual([
      ['p2', 0],
      ['p3', 1],
    ])
  })
})
