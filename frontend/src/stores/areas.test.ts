import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import * as adminApi from '@/api/admin'
import * as areasApi from '@/api/areas'
import { ApiError } from '@/api/client'
import type { Area, AreaInput } from '@/api/types'
import { useAreasStore } from './areas'

vi.mock('@/api/admin')
vi.mock('@/api/areas')

const area = (id: string, name: string, active = true): Area => ({
  id,
  name,
  kind: 'venue',
  lat: 50,
  lon: 14,
  radius_m: 200,
  active,
  created_at: '2026-10-01T10:00:00Z',
})
const input = (name: string, active = true): AreaInput => ({
  name,
  kind: 'venue',
  lat: 50,
  lon: 14,
  radius_m: 200,
  active,
})
const names = (list: Area[]) => list.map((a) => a.name)

async function loaded(list: Area[]) {
  vi.mocked(adminApi.getAdminAreas).mockResolvedValue(list)
  const s = useAreasStore()
  await s.loadAll()
  return s
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.resetAllMocks()
})

describe('areas store', () => {
  it('loadHere fetches areas around the point', async () => {
    vi.mocked(areasApi.getAreas).mockResolvedValue([area('a1', 'Centrum')])
    const s = useAreasStore()
    await s.loadHere({ lat: 50.1, lon: 14.4 })
    expect(areasApi.getAreas).toHaveBeenCalledWith(50.1, 14.4)
    expect(names(s.here)).toEqual(['Centrum'])
  })

  it('loadAll sorts active first, then by name', async () => {
    const s = await loaded([area('1', 'Zoo'), area('2', 'Bar', false), area('3', 'Andel')])
    expect(names(s.all)).toEqual(['Andel', 'Zoo', 'Bar'])
    expect(s.loading).toBe(false)
  })

  it('a failed loadAll propagates and stops loading', async () => {
    vi.mocked(adminApi.getAdminAreas).mockRejectedValue(new ApiError('internal', 500, ''))
    const s = useAreasStore()
    await expect(s.loadAll()).rejects.toMatchObject({ code: 'internal' })
    expect(s.loading).toBe(false)
  })

  it('save without id creates and inserts in order', async () => {
    const s = await loaded([area('1', 'Andel'), area('2', 'Zoo')])
    vi.mocked(adminApi.createArea).mockResolvedValue(area('3', 'Muzeum'))
    await s.save(input('Muzeum'))
    expect(adminApi.createArea).toHaveBeenCalledWith(input('Muzeum'))
    expect(names(s.all)).toEqual(['Andel', 'Muzeum', 'Zoo'])
  })

  it('save with id replaces the area and re-sorts', async () => {
    const s = await loaded([area('1', 'Andel'), area('2', 'Zoo')])
    vi.mocked(adminApi.updateArea).mockResolvedValue(area('1', 'Andel', false))
    await s.save(input('Andel', false), '1')
    expect(adminApi.updateArea).toHaveBeenCalledWith('1', input('Andel', false))
    expect(s.all.map((a) => [a.name, a.active])).toEqual([
      ['Zoo', true],
      ['Andel', false],
    ])
  })

  it('a failed save keeps the list', async () => {
    const s = await loaded([area('1', 'Andel')])
    vi.mocked(adminApi.updateArea).mockRejectedValue(new ApiError('validation', 400, ''))
    await expect(s.save(input(''), '1')).rejects.toMatchObject({ code: 'validation' })
    expect(names(s.all)).toEqual(['Andel'])
  })

  it('remove drops the area', async () => {
    const s = await loaded([area('1', 'Andel'), area('2', 'Zoo')])
    vi.mocked(adminApi.deleteArea).mockResolvedValue(undefined)
    await s.remove('1')
    expect(adminApi.deleteArea).toHaveBeenCalledWith('1')
    expect(names(s.all)).toEqual(['Zoo'])
  })

  it('area_in_use keeps the area and propagates', async () => {
    const s = await loaded([area('1', 'Andel')])
    vi.mocked(adminApi.deleteArea).mockRejectedValue(new ApiError('area_in_use', 409, ''))
    await expect(s.remove('1')).rejects.toMatchObject({ code: 'area_in_use' })
    expect(names(s.all)).toEqual(['Andel'])
  })

  it('reset empties both lists', async () => {
    const s = await loaded([area('1', 'Andel')])
    s.here = [area('2', 'Zoo')]
    s.reset()
    expect(s.all).toEqual([])
    expect(s.here).toEqual([])
  })
})
