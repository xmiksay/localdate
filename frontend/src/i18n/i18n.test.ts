import { describe, expect, it } from 'vitest'
import cs from './cs'
import en from './en'
import { ERROR_CODES } from '@/api/types'

function keys(o: object, prefix = ''): string[] {
  return Object.entries(o).flatMap(([k, v]) =>
    typeof v === 'object' ? keys(v, `${prefix}${k}.`) : [`${prefix}${k}`],
  )
}

describe('i18n', () => {
  it('cs and en have identical key sets', () => {
    expect(keys(en).sort()).toEqual(keys(cs).sort())
  })
  it('has all 40 interests', () => {
    expect(Object.keys(cs.interest)).toHaveLength(40)
  })
  it('translates every error code', () => {
    const errs = Object.keys(cs.error)
    for (const c of ERROR_CODES) expect(errs).toContain(c)
  })
})
