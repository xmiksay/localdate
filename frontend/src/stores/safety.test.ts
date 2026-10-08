import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import * as socialApi from '@/api/social'
import * as meApi from '@/api/me'
import { tokenStorage } from '@/api/tokens'
import type { MatchSummary, NearbyProfile } from '@/api/types'
import { useAuthStore } from './auth'
import { useMatchesStore } from './matches'
import { useNearbyStore } from './nearby'
import { useSafetyStore } from './safety'

vi.mock('@/api/social')
vi.mock('@/api/me')

const person = (id: string): NearbyProfile => ({
  user_id: id,
  display_name: id,
  age: 30,
  gender: 'other',
  bio: '',
  interests: [],
  shared_interests: [],
  photos: [],
  reasons: ['meet'],
  distance_band: 'lt_200m',
  area: null,
  wave_state: 'none',
  match_id: null,
})
const match = (id: string, userId: string): MatchSummary => ({
  match_id: id,
  created_at: '',
  other: { user_id: userId, display_name: userId, photo_url: null },
  last_message: null,
})

function seed() {
  const nearby = useNearbyStore()
  nearby.people = [person('bad'), person('ok')]
  nearby.incoming = [person('bad')]
  const matches = useMatchesStore()
  matches.matches = [match('m1', 'bad'), match('m2', 'ok')]
  return { nearby, matches }
}

beforeEach(() => {
  localStorage.clear()
  setActivePinia(createPinia())
  vi.resetAllMocks()
})

describe('safety store', () => {
  it('block removes the user from nearby, incoming and matches', async () => {
    const { nearby, matches } = seed()
    vi.mocked(socialApi.blockUser).mockResolvedValue(undefined)
    await useSafetyStore().block('bad')
    expect(nearby.people.map((p) => p.user_id)).toEqual(['ok'])
    expect(nearby.incoming).toEqual([])
    expect(matches.matches.map((m) => m.match_id)).toEqual(['m2'])
  })

  it('report sends reason and note, then drops the user', async () => {
    const { nearby } = seed()
    vi.mocked(socialApi.reportUser).mockResolvedValue(undefined)
    await useSafetyStore().report('bad', 'spam', 'ads')
    expect(socialApi.reportUser).toHaveBeenCalledWith('bad', 'spam', 'ads')
    expect(nearby.people.map((p) => p.user_id)).toEqual(['ok'])
  })

  it('an empty note is not sent', async () => {
    vi.mocked(socialApi.reportUser).mockResolvedValue(undefined)
    await useSafetyStore().report('bad', 'fake', '')
    expect(socialApi.reportUser).toHaveBeenCalledWith('bad', 'fake', undefined)
  })

  it('keeps the user locally when the server call fails', async () => {
    const { nearby } = seed()
    vi.mocked(socialApi.blockUser).mockRejectedValue(new Error('500'))
    await expect(useSafetyStore().block('bad')).rejects.toThrow()
    expect(nearby.people).toHaveLength(2)
  })

  it('unblock removes the entry from the blocked list', async () => {
    const s = useSafetyStore()
    s.blocked = [
      { user_id: 'a', display_name: 'A', created_at: '' },
      { user_id: 'b', display_name: 'B', created_at: '' },
    ]
    vi.mocked(socialApi.unblockUser).mockResolvedValue(undefined)
    await s.unblock('a')
    expect(s.blocked.map((b) => b.user_id)).toEqual(['b'])
  })

  it('deleting the account clears the session', async () => {
    tokenStorage.set('a', 'r')
    const auth = useAuthStore()
    auth.accessToken = 'a'
    vi.mocked(meApi.deleteMe).mockResolvedValue(undefined)
    await useSafetyStore().deleteAccount()
    expect(auth.isAuthed).toBe(false)
    expect(tokenStorage.access()).toBeNull()
  })

  it('keeps the session when deletion fails', async () => {
    tokenStorage.set('a', 'r')
    const auth = useAuthStore()
    auth.accessToken = 'a'
    vi.mocked(meApi.deleteMe).mockRejectedValue(new Error('500'))
    await expect(useSafetyStore().deleteAccount()).rejects.toThrow()
    expect(auth.isAuthed).toBe(true)
  })
})
