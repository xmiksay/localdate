import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import * as adminApi from '@/api/admin'
import type { AdminReport } from '@/api/types'
import { useAdminStore } from './admin'

vi.mock('@/api/admin')

const report = (id: string, subjectId: string, open = true): AdminReport => ({
  id,
  reason: 'spam',
  note: null,
  created_at: '2026-10-01T10:00:00Z',
  resolved_at: open ? null : '2026-10-02T10:00:00Z',
  resolution: open ? null : 'dismissed',
  resolved_by: open ? null : { id: 'adm', username: 'admin' },
  reporter: { id: 'rep', username: 'reporter' },
  subject: {
    id: subjectId,
    username: subjectId,
    display_name: null,
    photo_url: null,
    banned_at: null,
    is_admin: false,
    is_test: false,
    open_reports: 2,
  },
})

async function loaded(status: 'open' | 'resolved', list: AdminReport[]) {
  vi.mocked(adminApi.getReports).mockResolvedValue(list)
  const s = useAdminStore()
  await s.load(status)
  return s
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.resetAllMocks()
})

describe('admin store', () => {
  it('load fetches the requested tab', async () => {
    const s = await loaded('resolved', [report('r1', 'u1', false)])
    expect(adminApi.getReports).toHaveBeenCalledWith('resolved')
    expect(s.status).toBe('resolved')
    expect(s.reports.map((r) => r.id)).toEqual(['r1'])
  })

  it('a failed load keeps the current list and tab', async () => {
    const s = await loaded('open', [report('r1', 'u1')])
    vi.mocked(adminApi.getReports).mockRejectedValue(new Error('500'))
    await expect(s.load('resolved')).rejects.toThrow()
    expect(s.status).toBe('open')
    expect(s.reports).toHaveLength(1)
  })

  it('ignores a slower, older load', async () => {
    const s = useAdminStore()
    let resolveOld: (v: AdminReport[]) => void = () => undefined
    vi.mocked(adminApi.getReports)
      .mockReturnValueOnce(new Promise((r) => (resolveOld = r)))
      .mockResolvedValueOnce([report('new', 'u1', false)])
    const old = s.load('open')
    await s.load('resolved')
    resolveOld([report('old', 'u1')])
    await old
    expect(s.status).toBe('resolved')
    expect(s.reports.map((r) => r.id)).toEqual(['new'])
  })

  it('loading tracks only the newest load; an older failure is ignored', async () => {
    const s = useAdminStore()
    let rejectOld: (e: Error) => void = () => undefined
    let resolveNew: (v: AdminReport[]) => void = () => undefined
    vi.mocked(adminApi.getReports)
      .mockReturnValueOnce(new Promise((_, rej) => (rejectOld = rej)))
      .mockReturnValueOnce(new Promise((r) => (resolveNew = r)))
    const old = s.load('open')
    const next = s.load('resolved')
    expect(s.loading).toBe(true)
    rejectOld(new Error('500'))
    await expect(old).resolves.toBeUndefined()
    expect(s.loading).toBe(true)
    resolveNew([report('new', 'u1', false)])
    await next
    expect(s.loading).toBe(false)
    expect(s.status).toBe('resolved')
  })

  it('dismiss removes the report and decrements the subject count', async () => {
    const s = await loaded('open', [report('r1', 'u1'), report('r2', 'u1'), report('r3', 'u2')])
    vi.mocked(adminApi.dismissReport).mockResolvedValue(undefined)
    await s.dismiss('r1')
    expect(adminApi.dismissReport).toHaveBeenCalledWith('r1')
    expect(s.reports.map((r) => r.id)).toEqual(['r2', 'r3'])
    expect(s.reports[0].subject.open_reports).toBe(1)
    expect(s.reports[1].subject.open_reports).toBe(2)
  })

  it('a failed dismiss keeps the report', async () => {
    const s = await loaded('open', [report('r1', 'u1')])
    vi.mocked(adminApi.dismissReport).mockRejectedValue(new Error('409'))
    await expect(s.dismiss('r1')).rejects.toThrow()
    expect(s.reports).toHaveLength(1)
  })

  it('ban removes every open report of the subject', async () => {
    const s = await loaded('open', [report('r1', 'u1'), report('r2', 'u2'), report('r3', 'u1')])
    vi.mocked(adminApi.banUser).mockResolvedValue(undefined)
    await s.ban('u1')
    expect(adminApi.banUser).toHaveBeenCalledWith('u1')
    expect(s.reports.map((r) => r.id)).toEqual(['r2'])
  })

  it('ban marks the subject banned on resolved entries', async () => {
    const s = await loaded('resolved', [report('r1', 'u1', false), report('r2', 'u2', false)])
    vi.mocked(adminApi.banUser).mockResolvedValue(undefined)
    await s.ban('u1')
    expect(s.reports).toHaveLength(2)
    expect(s.reports[0].subject.banned_at).not.toBeNull()
    expect(s.reports[1].subject.banned_at).toBeNull()
  })

  it('a failed ban keeps the reports', async () => {
    const s = await loaded('open', [report('r1', 'u1')])
    vi.mocked(adminApi.banUser).mockRejectedValue(new Error('409'))
    await expect(s.ban('u1')).rejects.toThrow()
    expect(s.reports).toHaveLength(1)
    expect(s.reports[0].subject.banned_at).toBeNull()
  })

  it('unban clears banned_at on loaded entries', async () => {
    const r = report('r1', 'u1', false)
    r.subject.banned_at = '2026-10-02T10:00:00Z'
    const s = await loaded('resolved', [r])
    vi.mocked(adminApi.unbanUser).mockResolvedValue(undefined)
    await s.unban('u1')
    expect(adminApi.unbanUser).toHaveBeenCalledWith('u1')
    expect(s.reports[0].subject.banned_at).toBeNull()
  })

  it('a failed unban keeps the ban', async () => {
    const r = report('r1', 'u1', false)
    r.subject.banned_at = '2026-10-02T10:00:00Z'
    const s = await loaded('resolved', [r])
    vi.mocked(adminApi.unbanUser).mockRejectedValue(new Error('500'))
    await expect(s.unban('u1')).rejects.toThrow()
    expect(s.reports[0].subject.banned_at).not.toBeNull()
  })
})
