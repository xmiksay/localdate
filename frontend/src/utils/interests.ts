import type { Interest } from '@/api/types'

export interface MarkedInterest extends Interest {
  shared: boolean
}

/** Marks interests the viewer shares and moves them to the front, keeping the order otherwise. */
export function sharedFirst(interests: Interest[], sharedIds: number[]): MarkedInterest[] {
  const shared = new Set(sharedIds)
  const marked = interests.map((i) => ({ ...i, shared: shared.has(i.id) }))
  return [...marked.filter((i) => i.shared), ...marked.filter((i) => !i.shared)]
}
