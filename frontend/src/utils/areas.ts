import type { Area, AreaKind } from '@/api/types'

export const AREA_KIND_ICON: Record<AreaKind, string> = {
  city_centre: '🏙️',
  train_station: '🚉',
  venue: '🎟️',
  other: '📍',
}

/** Same order as `GET /admin/areas`: active first, then by name. */
export function byActiveThenName(a: Area, b: Area): number {
  if (a.active !== b.active) return a.active ? -1 : 1
  return a.name.localeCompare(b.name)
}
