import { beforeEach, describe, expect, it, vi } from 'vitest'
import { tokenStorage } from './tokens'
import { startWindow } from './window'

const fetchMock = vi.fn()
const body = () => JSON.parse(fetchMock.mock.calls[0][1].body)

beforeEach(() => {
  localStorage.clear()
  fetchMock.mockReset()
  fetchMock.mockImplementation(async () => new Response('{}', { status: 200 }))
  vi.stubGlobal('fetch', fetchMock)
  tokenStorage.set('a', 'r')
})

describe('startWindow', () => {
  it('posts a timed window with minutes', async () => {
    await startWindow(60, 50.1, 14.4)
    const [url, init] = fetchMock.mock.calls[0]
    expect([url, init.method]).toEqual(['/api/me/window', 'POST'])
    expect(body()).toEqual({ minutes: 60, lat: 50.1, lon: 14.4 })
  })

  it('sends kind and area_id for an area window', async () => {
    await startWindow(120, 50.1, 14.4, 'a1')
    expect(body()).toEqual({ kind: 'area', area_id: 'a1', minutes: 120, lat: 50.1, lon: 14.4 })
  })

  it('sends until and the device time zone instead of minutes for end of day', async () => {
    vi.spyOn(Intl.DateTimeFormat.prototype, 'resolvedOptions').mockReturnValue({
      ...new Intl.DateTimeFormat().resolvedOptions(),
      timeZone: 'Asia/Tokyo',
    })
    await startWindow('end_of_day', 50.1, 14.4)
    expect(body()).toEqual({ until: 'end_of_day', tz: 'Asia/Tokyo', lat: 50.1, lon: 14.4 })
    await startWindow('end_of_day', 50.1, 14.4, 'a1')
    expect(JSON.parse(fetchMock.mock.calls[1][1].body)).toEqual({
      kind: 'area',
      area_id: 'a1',
      until: 'end_of_day',
      tz: 'Asia/Tokyo',
      lat: 50.1,
      lon: 14.4,
    })
    vi.restoreAllMocks()
  })
})
