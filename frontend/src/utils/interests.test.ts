import { describe, expect, it } from 'vitest'
import type { DistanceBand, NearbyProfile } from '@/api/types'
import { byOverlapThenBand, sharedFirst } from './interests'

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

const p = (id: string, shared: number[], band: DistanceBand) =>
  ({ user_id: id, shared_interests: shared, distance_band: band }) as NearbyProfile

describe('byOverlapThenBand', () => {
  it('orders by shared count desc, then nearest band, keeping ties in place', () => {
    const list = [
      p('none-near', [], 'lt_200m'),
      p('one-far', [1], 'lt_5km'),
      p('two-mid', [1, 2], 'lt_1km'),
      p('one-near-a', [3], 'lt_200m'),
      p('one-near-b', [4], 'lt_200m'),
      p('none-far', [], 'lt_10km'),
    ]
    expect(list.sort(byOverlapThenBand).map((x) => x.user_id)).toEqual([
      'two-mid',
      'one-near-a',
      'one-near-b',
      'one-far',
      'none-near',
      'none-far',
    ])
  })
})
