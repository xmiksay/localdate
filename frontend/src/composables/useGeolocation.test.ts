import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { effectScope, nextTick } from 'vue'
import { createPinia, setActivePinia } from 'pinia'
import { ApiError } from '@/api/client'
import * as windowApi from '@/api/window'
import type { Window } from '@/api/types'
import { useWindowStore } from '@/stores/window'
import { useLocationSharing } from './useGeolocation'

vi.mock('@/api/window')

const areaWindow: Window = {
  id: 'w1',
  kind: 'area',
  area: { id: 'a1', name: 'Centrum' },
  starts_at: new Date().toISOString(),
  ends_at: new Date(Date.now() + 3_600_000).toISOString(),
  waves_left: 20,
}
let onFix: PositionCallback = () => undefined
const geo = {
  watchPosition: vi.fn(),
  clearWatch: vi.fn(),
  getCurrentPosition: vi.fn(),
}
const fix = (lat: number, lon: number) =>
  onFix({ coords: { latitude: lat, longitude: lon, accuracy: 25 } } as GeolocationPosition)
const flush = () => new Promise((r) => setTimeout(r, 0))

async function sharing() {
  const win = useWindowStore()
  vi.mocked(windowApi.startWindow).mockResolvedValue(areaWindow)
  await win.start(60, { lat: 50, lon: 14 }, 'a1')
  const scope = effectScope()
  scope.run(useLocationSharing)
  await nextTick()
  return { win, scope }
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.resetAllMocks()
  geo.watchPosition.mockImplementation((cb: PositionCallback) => {
    onFix = cb
    return 1
  })
  vi.stubGlobal('navigator', { geolocation: geo })
})
afterEach(() => vi.unstubAllGlobals())

describe('location sharing', () => {
  it('sends fixes while a window is active', async () => {
    const { win, scope } = await sharing()
    vi.mocked(windowApi.updateLocation).mockResolvedValue(undefined)
    fix(50.001, 14.001)
    await flush()
    expect(windowApi.updateLocation).toHaveBeenCalledWith(50.001, 14.001, 25)
    expect(win.isActive).toBe(true)
    scope.stop()
  })

  it('left_area ends the window locally with a notice and stops watching', async () => {
    const { win, scope } = await sharing()
    vi.mocked(windowApi.updateLocation).mockRejectedValue(new ApiError('left_area', 409, ''))
    fix(51, 15)
    await flush()
    expect(win.isActive).toBe(false)
    expect(win.endedNotice).toBe('left_area')
    expect(windowApi.endWindow).not.toHaveBeenCalled()
    await nextTick()
    expect(geo.clearWatch).toHaveBeenCalledWith(1)
    scope.stop()
  })

  it('ignores a 409 that answers a fix sent for a window since replaced', async () => {
    const { win, scope } = await sharing()
    let reject: (e: unknown) => void = () => undefined
    vi.mocked(windowApi.updateLocation).mockReturnValue(new Promise((_, r) => (reject = r)))
    fix(51, 15)
    await flush()
    vi.mocked(windowApi.startWindow).mockResolvedValue({ ...areaWindow, id: 'w2' })
    await win.start(60, { lat: 50, lon: 14 }, 'a1')
    reject(new ApiError('left_area', 409, ''))
    await flush()
    expect(win.current?.id).toBe('w2')
    expect(win.endedNotice).toBeNull()
    scope.stop()
  })

  it('no_active_window clears the window without a notice', async () => {
    const { win, scope } = await sharing()
    vi.mocked(windowApi.updateLocation).mockRejectedValue(new ApiError('no_active_window', 409, ''))
    fix(50.001, 14.001)
    await flush()
    expect(win.isActive).toBe(false)
    expect(win.endedNotice).toBeNull()
    scope.stop()
  })

  it('other failures keep the window and retry on the next fix', async () => {
    const { win, scope } = await sharing()
    vi.mocked(windowApi.updateLocation).mockRejectedValueOnce(new ApiError('network', 0, ''))
    fix(50.001, 14.001)
    await flush()
    expect(win.isActive).toBe(true)
    vi.mocked(windowApi.updateLocation).mockResolvedValue(undefined)
    fix(50.001, 14.001)
    await flush()
    expect(windowApi.updateLocation).toHaveBeenCalledTimes(2)
    scope.stop()
  })
})
