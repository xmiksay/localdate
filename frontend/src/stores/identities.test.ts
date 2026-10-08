import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import * as identitiesApi from '@/api/identities'
import { ApiError } from '@/api/client'
import type { Identity } from '@/api/types'
import { useIdentitiesStore } from './identities'

vi.mock('@/api/identities')

const identity = (id: string, subject = `${id}@example.cz`): Identity => ({
  id,
  provider: 'email',
  subject,
  verified_at: '2026-10-01T10:00:00Z',
  created_at: '2026-10-01T10:00:00Z',
})

beforeEach(() => {
  setActivePinia(createPinia())
  vi.resetAllMocks()
})

describe('identities store', () => {
  it('load takes identities and the password flag', async () => {
    vi.mocked(identitiesApi.getIdentities).mockResolvedValue({
      has_password: false,
      identities: [identity('a')],
    })
    const s = useIdentitiesStore()
    await s.load()
    expect(s.hasPassword).toBe(false)
    expect(s.identities.map((i) => i.id)).toEqual(['a'])
  })

  it('linkEmail normalizes the address and sends the UI language', async () => {
    vi.mocked(identitiesApi.linkEmail).mockResolvedValue(undefined)
    await useIdentitiesStore().linkEmail(' Eva@Example.CZ ')
    expect(identitiesApi.linkEmail).toHaveBeenCalledWith({ email: 'eva@example.cz', lang: 'cs' })
  })

  it('confirm appends the new identity once and remembers the address', async () => {
    vi.mocked(identitiesApi.confirmEmailLink).mockResolvedValue(identity('b', 'eva@example.cz'))
    const s = useIdentitiesStore()
    s.identities = [identity('a')]
    await s.confirm('tok')
    await s.confirm('tok')
    expect(identitiesApi.confirmEmailLink).toHaveBeenCalledWith('tok')
    expect(s.identities.map((i) => i.id)).toEqual(['a', 'b'])
    expect(s.justLinked).toBe('eva@example.cz')
  })

  it('remove drops the identity only after the server agrees', async () => {
    const s = useIdentitiesStore()
    s.identities = [identity('a'), identity('b')]
    vi.mocked(identitiesApi.deleteIdentity).mockRejectedValueOnce(
      new ApiError('last_login_method', 409, 'last'),
    )
    await expect(s.remove('a')).rejects.toMatchObject({ code: 'last_login_method' })
    expect(s.identities).toHaveLength(2)
    vi.mocked(identitiesApi.deleteIdentity).mockResolvedValue(undefined)
    await s.remove('a')
    expect(s.identities.map((i) => i.id)).toEqual(['b'])
  })

  it('reset forgets everything', () => {
    const s = useIdentitiesStore()
    s.identities = [identity('a')]
    s.hasPassword = false
    s.justLinked = 'x@y.cz'
    s.reset()
    expect(s.identities).toEqual([])
    expect(s.hasPassword).toBe(true)
    expect(s.justLinked).toBeNull()
  })
})
