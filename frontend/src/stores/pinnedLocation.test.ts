import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { ApiError } from '@/api/client'
import * as windowApi from '@/api/window'
import type { Window } from '@/api/types'
import { usePinnedLocationStore } from './pinnedLocation'
import { useWindowStore } from './window'

vi.mock('@/api/window')

const win = (id: string): Window => ({
  id,
  kind: 'timed',
  area: null,
  starts_at: new Date().toISOString(),
  ends_at: new Date(Date.now() + 3_600_000).toISOString(),
  waves_left: 20,
})
const here = { lat: 50.08, lon: 14.42 }

async function withWindow() {
  vi.mocked(windowApi.getWindow).mockResolvedValue(win('w1'))
  const w = useWindowStore()
  await w.load()
  return w
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.resetAllMocks()
})

describe('pinned location', () => {
  it('starts a timed window at the point when none runs', async () => {
    vi.mocked(windowApi.startWindow).mockResolvedValue(win('w1'))
    const s = usePinnedLocationStore()
    await s.pin(here, 120)
    expect(windowApi.startWindow).toHaveBeenCalledWith(120, 50.08, 14.42, undefined)
    expect(windowApi.updateLocation).not.toHaveBeenCalled()
    expect(useWindowStore().isActive).toBe(true)
    expect(s.at).toEqual(here)
  })

  it('moves the running window with /me/location', async () => {
    await withWindow()
    vi.mocked(windowApi.updateLocation).mockResolvedValue(undefined)
    const s = usePinnedLocationStore()
    await s.pin(here, 60)
    expect(windowApi.updateLocation).toHaveBeenCalledWith(50.08, 14.42)
    expect(windowApi.startWindow).not.toHaveBeenCalled()
    expect(s.at).toEqual(here)
  })

  it('starts a new window when the old one ended server-side', async () => {
    await withWindow()
    vi.mocked(windowApi.updateLocation).mockRejectedValue(new ApiError('no_active_window', 409, ''))
    vi.mocked(windowApi.startWindow).mockResolvedValue(win('w2'))
    const s = usePinnedLocationStore()
    await s.pin(here, 30)
    expect(windowApi.startWindow).toHaveBeenCalledWith(30, 50.08, 14.42, undefined)
    expect(useWindowStore().current?.id).toBe('w2')
  })

  it('leaving an area ends the window with its notice and reports the error', async () => {
    const w = await withWindow()
    vi.mocked(windowApi.updateLocation).mockRejectedValue(new ApiError('left_area', 409, ''))
    const s = usePinnedLocationStore()
    await expect(s.pin(here, 60)).rejects.toMatchObject({ code: 'left_area' })
    expect(w.isActive).toBe(false)
    expect(w.endedNotice).toBe('left_area')
    expect(windowApi.startWindow).not.toHaveBeenCalled()
  })

  it('keeps the old point when the move fails otherwise', async () => {
    await withWindow()
    const s = usePinnedLocationStore()
    s.at = { lat: 1, lon: 2 }
    vi.mocked(windowApi.updateLocation).mockRejectedValue(new ApiError('network', 0, ''))
    await expect(s.pin(here, 60)).rejects.toBeInstanceOf(ApiError)
    expect(s.at).toEqual({ lat: 1, lon: 2 })
  })
})
