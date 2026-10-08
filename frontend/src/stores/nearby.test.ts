import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import * as socialApi from '@/api/social'
import * as windowApi from '@/api/window'
import { ApiError } from '@/api/client'
import type { NearbyProfile } from '@/api/types'
import { useNearbyStore } from './nearby'

vi.mock('@/api/social')
vi.mock('@/api/window')

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

beforeEach(() => {
  setActivePinia(createPinia())
  vi.resetAllMocks()
  vi.mocked(windowApi.getWindow).mockResolvedValue(null)
})

describe('nearby store waves', () => {
  it('marks the person as waved', async () => {
    const s = useNearbyStore()
    s.people = [person('a')]
    vi.mocked(socialApi.sendWave).mockResolvedValue({ matched: false, match_id: null })
    await s.wave('a')
    expect(s.people[0].wave_state).toBe('sent')
  })

  it('records the match and drops the sender from incoming', async () => {
    const s = useNearbyStore()
    s.people = [{ ...person('a'), wave_state: 'received' }]
    s.incoming = [person('a')]
    vi.mocked(socialApi.sendWave).mockResolvedValue({ matched: true, match_id: 'm1' })
    await s.wave('a')
    expect(s.people[0]).toMatchObject({ wave_state: 'matched', match_id: 'm1' })
    expect(s.incoming).toEqual([])
  })

  it('slots a waved-back sender into display order, not at the end', async () => {
    const s = useNearbyStore()
    const sender = { ...person('b'), shared_interests: [1], wave_state: 'received' as const }
    s.people = [{ ...person('a'), shared_interests: [1, 2] }, person('c')]
    s.incoming = [sender]
    vi.mocked(socialApi.sendWave).mockResolvedValue({ matched: true, match_id: 'm1' })
    await s.wave('b')
    expect(s.people.map((p) => p.user_id)).toEqual(['a', 'b', 'c'])
  })

  it('removes a person who is no longer visible', async () => {
    const s = useNearbyStore()
    s.people = [person('a')]
    vi.mocked(socialApi.sendWave).mockRejectedValue(new ApiError('not_visible', 409, ''))
    await expect(s.wave('a')).rejects.toMatchObject({ code: 'not_visible' })
    expect(s.people).toEqual([])
  })
})
