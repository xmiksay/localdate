import { beforeEach, describe, expect, it, vi } from 'vitest'
import { getPreview } from './preview'
import { tokenStorage } from './tokens'

const fetchMock = vi.fn()

beforeEach(() => {
  localStorage.clear()
  fetchMock.mockReset()
  fetchMock.mockImplementation(async () => new Response('{"total":0,"items":[]}', { status: 200 }))
  vi.stubGlobal('fetch', fetchMock)
})

describe('getPreview', () => {
  it('sends the point and genders as query parameters', async () => {
    await getPreview(50.1, 14.42, 'male', ['female', 'other'])
    expect(fetchMock.mock.calls[0][0]).toBe(
      '/api/preview?lat=50.1&lon=14.42&gender=male&genders=female%2Cother',
    )
  })

  it('leaves genders out for "any" and never sends a token', async () => {
    tokenStorage.set('a', 'r')
    await getPreview(50.1, 14.42, 'female', [])
    const [url, init] = fetchMock.mock.calls[0]
    expect(url).toBe('/api/preview?lat=50.1&lon=14.42&gender=female')
    expect(init.headers.Authorization).toBeUndefined()
  })
})
