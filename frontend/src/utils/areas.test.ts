import { describe, expect, it } from 'vitest'
import type { Area } from '@/api/types'
import { byActiveThenName } from './areas'

const a = (name: string, active: boolean) => ({ name, active }) as Area

describe('byActiveThenName', () => {
  it('puts active areas first, each group by name', () => {
    const list = [a('Zoo', false), a('Pivnice', true), a('Andel', false), a('Kino', true)]
    expect(list.sort(byActiveThenName).map((x) => x.name)).toEqual([
      'Kino',
      'Pivnice',
      'Andel',
      'Zoo',
    ])
  })
})
