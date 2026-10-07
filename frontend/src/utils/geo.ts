export interface Coords {
  lat: number
  lon: number
}

const EARTH_RADIUS_M = 6_371_000
const toRad = (deg: number) => (deg * Math.PI) / 180

export function haversineMeters(a: Coords, b: Coords): number {
  const dLat = toRad(b.lat - a.lat)
  const dLon = toRad(b.lon - a.lon)
  const h =
    Math.sin(dLat / 2) ** 2 +
    Math.cos(toRad(a.lat)) * Math.cos(toRad(b.lat)) * Math.sin(dLon / 2) ** 2
  return 2 * EARTH_RADIUS_M * Math.asin(Math.min(1, Math.sqrt(h)))
}

export const LOCATION_INTERVAL_MS = 2 * 60_000
export const LOCATION_MOVE_M = 100

/** Contract: update every 2 min, or immediately after moving more than 100 m. */
export function shouldSendLocation(
  prev: Coords | null,
  next: Coords,
  lastSentAt: number | null,
  now: number,
): boolean {
  if (prev === null || lastSentAt === null) return true
  if (now - lastSentAt >= LOCATION_INTERVAL_MS) return true
  return haversineMeters(prev, next) > LOCATION_MOVE_M
}
