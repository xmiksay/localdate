import { beforeEach, describe, expect, it, vi } from 'vitest'
import { getHealth } from './health'

const fetchMock = vi.fn()
const body = { status: 'ok', version: '0.1.0', commit: 'fa15e9c', built_at: null, db: 'ok' }

beforeEach(() => {
  fetchMock.mockReset()
  vi.stubGlobal('fetch', fetchMock)
})

describe('getHealth', () => {
  it('reads the build identity from GET /api/health', async () => {
    fetchMock.mockResolvedValue(new Response(JSON.stringify(body), { status: 200 }))
    expect(await getHealth()).toEqual(body)
    expect(fetchMock).toHaveBeenCalledWith('/api/health')
  })

  it('still reads it from a 503 (database down)', async () => {
    const degraded = { ...body, status: 'degraded', db: 'unavailable' }
    fetchMock.mockResolvedValue(new Response(JSON.stringify(degraded), { status: 503 }))
    expect(await getHealth()).toEqual(degraded)
  })

  it('is null for a network error, a non-JSON body or a body without version', async () => {
    fetchMock.mockRejectedValueOnce(new TypeError('offline'))
    expect(await getHealth()).toBeNull()
    fetchMock.mockResolvedValueOnce(new Response('ok', { status: 200 }))
    expect(await getHealth()).toBeNull()
    fetchMock.mockResolvedValueOnce(new Response('{"status":"ok"}', { status: 200 }))
    expect(await getHealth()).toBeNull()
  })
})
