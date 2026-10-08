import { describe, expect, it } from 'vitest'
import { sharedFirst } from './interests'

const i = (id: number) => ({ id, key: `k${id}` })

describe('sharedFirst', () => {
  it('moves shared interests to the front and marks them', () => {
    const out = sharedFirst([i(1), i(2), i(3), i(4)], [4, 2])
    expect(out.map((x) => x.id)).toEqual([2, 4, 1, 3])
    expect(out.map((x) => x.shared)).toEqual([true, true, false, false])
  })
  it('keeps the order when nothing is shared', () => {
    expect(sharedFirst([i(3), i(1)], []).map((x) => x.id)).toEqual([3, 1])
  })
  it('ignores shared ids the person does not list', () => {
    expect(sharedFirst([i(1)], [9])).toEqual([{ id: 1, key: 'k1', shared: false }])
  })
})
