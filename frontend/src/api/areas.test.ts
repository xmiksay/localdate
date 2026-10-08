import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createArea, deleteArea, getAdminAreas, updateArea } from './admin'
import { getAreas } from './areas'
import { tokenStorage } from './tokens'
import type { AreaInput } from './types'
import { startWindow } from './window'

const fetchMock = vi.fn()
const call = () => {
  const [url, init] = fetchMock.mock.calls[0]
  return { url, method: init.method, body: init.body ? JSON.parse(init.body) : undefined }
}
const input: AreaInput = {
  name: 'Hlavní nádraží',
  kind: 'train_station',
  lat: 50.083,
  lon: 14.435,
  radius_m: 300,
  active: true,
}

beforeEach(() => {
  localStorage.clear()
  fetchMock.mockReset()
  fetchMock.mockImplementation(async () => new Response('{}', { status: 200 }))
  vi.stubGlobal('fetch', fetchMock)
  tokenStorage.set('a', 'r')
})

describe('area endpoints', () => {
  it('getAreas sends the point as query parameters', async () => {
    await getAreas(50.1, 14.42)
    expect(call()).toMatchObject({ url: '/api/areas?lat=50.1&lon=14.42', method: 'GET' })
  })

  it('startWindow keeps the timed body without an area', async () => {
    await startWindow(60, 50.1, 14.4)
    expect(call()).toEqual({
      url: '/api/me/window',
      method: 'POST',
      body: { minutes: 60, lat: 50.1, lon: 14.4 },
    })
  })

  it('startWindow sends kind and area_id for an area window', async () => {
    await startWindow(120, 50.1, 14.4, 'a1')
    expect(call().body).toEqual({ kind: 'area', area_id: 'a1', minutes: 120, lat: 50.1, lon: 14.4 })
  })

  it('admin area CRUD hits the documented routes', async () => {
    await getAdminAreas()
    await createArea(input)
    await updateArea('a1', input)
    fetchMock.mockImplementationOnce(async () => new Response(null, { status: 204 }))
    await deleteArea('a1')
    const calls = fetchMock.mock.calls.map(([url, init]) => [init.method, url])
    expect(calls).toEqual([
      ['GET', '/api/admin/areas'],
      ['POST', '/api/admin/areas'],
      ['PUT', '/api/admin/areas/a1'],
      ['DELETE', '/api/admin/areas/a1'],
    ])
    expect(JSON.parse(fetchMock.mock.calls[2][1].body)).toEqual(input)
  })
})
