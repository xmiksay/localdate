import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import * as adminApi from '@/api/admin'
import type { AdminUserRow, TestUserInput } from '@/api/types'
import { useAdminUsersStore } from './adminUsers'

vi.mock('@/api/admin')

const row = (id: string): AdminUserRow => ({
  id,
  username: id,
  display_name: null,
  photo_url: null,
  gender: 'female',
  age: 25,
  is_test: true,
  is_admin: false,
  banned_at: null,
  created_at: '2026-10-10T10:00:00Z',
})
const input: TestUserInput = {
  username: 'tess',
  display_name: 'Tess',
  gender: 'female',
  birth_date: '2000-01-01',
  interest_ids: [],
}
const photo = new File(['x'], 'a.jpg', { type: 'image/jpeg' })

beforeEach(() => {
  setActivePinia(createPinia())
  vi.resetAllMocks()
})

describe('admin users store', () => {
  it('creates with a placeholder avatar when no photo is given', async () => {
    vi.mocked(adminApi.createTestUser).mockResolvedValue(row('u1'))
    const s = useAdminUsersStore()
    const r = await s.createTestUser(input, null)
    expect(adminApi.createTestUser).toHaveBeenCalledWith({ ...input, placeholder_photo: true })
    expect(adminApi.uploadTestUserPhoto).not.toHaveBeenCalled()
    expect(r.photoError).toBeNull()
    expect(s.testUsers.map((u) => u.id)).toEqual(['u1'])
  })

  it('creates without a placeholder, then uploads the chosen photo', async () => {
    vi.mocked(adminApi.createTestUser).mockResolvedValue(row('u1'))
    vi.mocked(adminApi.uploadTestUserPhoto).mockResolvedValue({
      id: 'p1',
      url: '/media/p1.webp',
      position: 0,
    })
    const s = useAdminUsersStore()
    await s.createTestUser(input, photo)
    expect(adminApi.createTestUser).toHaveBeenCalledWith({ ...input, placeholder_photo: false })
    expect(adminApi.uploadTestUserPhoto).toHaveBeenCalledWith('u1', photo)
    expect(s.testUsers[0].photo_url).toBe('/media/p1.webp')
  })

  it('a failed upload still keeps the created account', async () => {
    vi.mocked(adminApi.createTestUser).mockResolvedValue(row('u1'))
    vi.mocked(adminApi.uploadTestUserPhoto).mockRejectedValue(new Error('413'))
    const s = useAdminUsersStore()
    const r = await s.createTestUser(input, photo)
    expect(r.photoError).toBeInstanceOf(Error)
    expect(s.testUsers).toHaveLength(1)
  })

  it('delete drops the user from both lists', async () => {
    vi.mocked(adminApi.listTestUsers).mockResolvedValue([row('u1'), row('u2')])
    vi.mocked(adminApi.searchUsers).mockResolvedValue([row('u1')])
    const s = useAdminUsersStore()
    await s.loadTestUsers()
    await s.search('u')
    await s.deleteTestUser('u1')
    expect(adminApi.deleteTestUser).toHaveBeenCalledWith('u1')
    expect(s.testUsers.map((u) => u.id)).toEqual(['u2'])
    expect(s.found).toEqual([])
  })

  it('only the newest search lands', async () => {
    let resolveOld: (v: AdminUserRow[]) => void = () => undefined
    vi.mocked(adminApi.searchUsers)
      .mockReturnValueOnce(new Promise((r) => (resolveOld = r)))
      .mockResolvedValueOnce([row('new')])
    const s = useAdminUsersStore()
    const old = s.search('a')
    await s.search('ab')
    resolveOld([row('old')])
    await old
    expect(s.found.map((u) => u.id)).toEqual(['new'])
  })

  it('loads settings once', async () => {
    vi.mocked(adminApi.getSettings).mockResolvedValue({ impersonation: true })
    const s = useAdminUsersStore()
    await s.loadSettings()
    await s.loadSettings()
    expect(adminApi.getSettings).toHaveBeenCalledTimes(1)
    expect(s.settings?.impersonation).toBe(true)
  })
})
