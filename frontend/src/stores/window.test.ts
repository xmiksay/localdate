import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import * as windowApi from '@/api/window'
import type { Window } from '@/api/types'
import { useWindowStore } from './window'

vi.mock('@/api/window')

const T0 = new Date('2026-06-01T12:00:00Z')
const win = (minutes: number, wavesLeft = 20): Window => ({
  id: 'w1',
  kind: 'timed',
  starts_at: T0.toISOString(),
  ends_at: new Date(T0.getTime() + minutes * 60_000).toISOString(),
  waves_left: wavesLeft,
})

beforeEach(() => {
  vi.useFakeTimers()
  vi.setSystemTime(T0)
  setActivePinia(createPinia())
  vi.resetAllMocks()
})
afterEach(() => vi.useRealTimers())

describe('window store', () => {
  it('has no window until loaded', async () => {
    vi.mocked(windowApi.getWindow).mockResolvedValue(null)
    const s = useWindowStore()
    await s.load()
    expect(s.isActive).toBe(false)
    expect(s.remainingMs).toBe(0)
  })

  it('counts down and auto-clears when ends_at passes', async () => {
    vi.mocked(windowApi.getWindow).mockResolvedValue(win(1, 7))
    const s = useWindowStore()
    await s.load()
    expect(s.isActive).toBe(true)
    expect(s.wavesLeft).toBe(7)
    expect(s.remainingMs).toBe(60_000)
    vi.advanceTimersByTime(30_000)
    expect(s.remainingMs).toBe(30_000)
    vi.advanceTimersByTime(30_000)
    expect(s.isActive).toBe(false)
    expect(s.wavesLeft).toBe(0)
  })

  it('drops an already-expired window straight away', async () => {
    vi.mocked(windowApi.getWindow).mockResolvedValue(win(-1))
    const s = useWindowStore()
    await s.load()
    expect(s.isActive).toBe(false)
  })

  it('start posts minutes and coordinates', async () => {
    vi.mocked(windowApi.startWindow).mockResolvedValue(win(60))
    const s = useWindowStore()
    await s.start(60, { lat: 50.1, lon: 14.4 })
    expect(windowApi.startWindow).toHaveBeenCalledWith(60, 50.1, 14.4)
    expect(s.isActive).toBe(true)
  })

  it('extend replaces the window with the longer one', async () => {
    vi.mocked(windowApi.startWindow).mockResolvedValue(win(30))
    vi.mocked(windowApi.extendWindow).mockResolvedValue(win(90))
    const s = useWindowStore()
    await s.start(30, { lat: 0, lon: 0 })
    await s.extend(60)
    expect(windowApi.extendWindow).toHaveBeenCalledWith(60)
    expect(s.remainingMs).toBe(90 * 60_000)
  })

  it('end calls the API and clears state', async () => {
    vi.mocked(windowApi.startWindow).mockResolvedValue(win(30))
    vi.mocked(windowApi.endWindow).mockResolvedValue(undefined)
    const s = useWindowStore()
    await s.start(30, { lat: 0, lon: 0 })
    await s.end()
    expect(s.isActive).toBe(false)
    expect(vi.getTimerCount()).toBe(0)
  })

  it('failed start leaves no window', async () => {
    vi.mocked(windowApi.startWindow).mockRejectedValue(new Error('422'))
    const s = useWindowStore()
    await expect(s.start(30, { lat: 0, lon: 0 })).rejects.toThrow()
    expect(s.isActive).toBe(false)
  })
})
