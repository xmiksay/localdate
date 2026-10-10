import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as admin from './admin'
import { tokenStorage } from './tokens'

const fetchMock = vi.fn()
const ok = (body: unknown, status = 200) =>
  new Response(body === undefined ? null : JSON.stringify(body), { status })
const call = (i = 0) => {
  const [url, init] = fetchMock.mock.calls[i]
  return { url: url as string, method: init.method as string, init }
}

beforeEach(() => {
  localStorage.clear()
  sessionStorage.clear()
  fetchMock.mockReset()
  vi.stubGlobal('fetch', fetchMock)
  tokenStorage.set('a', 'r')
})

describe('admin api', () => {
  it('reads the settings', async () => {
    fetchMock.mockResolvedValue(ok({ impersonation: true }))
    expect(await admin.getSettings()).toEqual({ impersonation: true })
    expect(call().url).toBe('/api/admin/settings')
  })

  it('searches users, an empty query lists the newest', async () => {
    fetchMock.mockImplementation(async () => ok([]))
    await admin.searchUsers('  ann ')
    await admin.searchUsers('   ')
    expect(call(0).url).toBe('/api/admin/users?q=ann')
    expect(call(1).url).toBe('/api/admin/users')
  })

  it('lists, creates and deletes test users', async () => {
    fetchMock.mockImplementation(async () => ok([]))
    await admin.listTestUsers()
    expect(call(0)).toMatchObject({ url: '/api/admin/test-users', method: 'GET' })

    fetchMock.mockResolvedValueOnce(ok({ id: 'u1' }, 201))
    const body = {
      username: 'tester',
      display_name: 'Tess',
      gender: 'female' as const,
      birth_date: '2000-01-01',
      interest_ids: [1, 2],
      placeholder_photo: true,
    }
    expect(await admin.createTestUser(body)).toEqual({ id: 'u1' })
    expect(call(1)).toMatchObject({ url: '/api/admin/test-users', method: 'POST' })
    expect(JSON.parse(call(1).init.body)).toEqual(body)

    fetchMock.mockResolvedValueOnce(ok(undefined, 204))
    await admin.deleteTestUser('u1')
    expect(call(2)).toMatchObject({ url: '/api/admin/test-users/u1', method: 'DELETE' })
  })

  it('uploads a test user photo as multipart `file`', async () => {
    fetchMock.mockResolvedValue(ok({ id: 'p1', url: '/media/p1.webp', position: 0 }, 201))
    const file = new File(['x'], 'a.jpg', { type: 'image/jpeg' })
    expect(await admin.uploadTestUserPhoto('u1', file)).toMatchObject({ id: 'p1' })
    const { url, method, init } = call()
    expect(url).toBe('/api/admin/test-users/u1/photos')
    expect(method).toBe('POST')
    expect((init.body as FormData).get('file')).toBeInstanceOf(File)
    expect(init.headers['Content-Type']).toBeUndefined()
  })

  it('impersonates and reads the audit log', async () => {
    fetchMock.mockResolvedValueOnce(ok({ access_token: 't', expires_at: '', user: {} }))
    await admin.impersonate('u1')
    expect(call(0)).toMatchObject({ url: '/api/admin/users/u1/impersonate', method: 'POST' })
    fetchMock.mockResolvedValueOnce(ok([]))
    await admin.listAudit()
    expect(call(1)).toMatchObject({ url: '/api/admin/audit', method: 'GET' })
  })
})
