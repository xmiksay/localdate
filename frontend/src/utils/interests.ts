import type { DistanceBand, Interest, NearbyProfile } from '@/api/types'

export interface MarkedInterest extends Interest {
  shared: boolean
}

/** Marks interests the viewer shares and moves them to the front, keeping the order otherwise. */
export function sharedFirst(interests: Interest[], sharedIds: number[]): MarkedInterest[] {
  const shared = new Set(sharedIds)
  const marked = interests.map((i) => ({ ...i, shared: shared.has(i.id) }))
  return [...marked.filter((i) => i.shared), ...marked.filter((i) => !i.shared)]
}

const BAND_ORDER: DistanceBand[] = ['lt_200m', 'lt_500m', 'lt_1km', 'lt_2km', 'lt_5km', 'lt_10km']

/**
 * The backend's nearby order minus its window-start and id tiebreaks, which the client can't see.
 * Use with the stable `Array.prototype.sort` so ties keep the server's order.
 */
export function byOverlapThenBand(a: NearbyProfile, b: NearbyProfile): number {
  return (
    b.shared_interests.length - a.shared_interests.length ||
    BAND_ORDER.indexOf(a.distance_band) - BAND_ORDER.indexOf(b.distance_band)
  )
}
