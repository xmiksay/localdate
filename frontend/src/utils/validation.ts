export const USERNAME_RE = /^[a-z0-9_]{3,32}$/
export const MAX_PHOTO_BYTES = 10 * 1024 * 1024
export const PHOTO_TYPES = ['image/jpeg', 'image/png', 'image/webp']
export const MAX_PHOTOS = 6
export const MAX_INTERESTS = 10
export const MAX_BIO = 500
export const MAX_NAME = 40

export const normalizeUsername = (s: string) => s.trim().toLowerCase()
export const isValidUsername = (s: string) => USERNAME_RE.test(normalizeUsername(s))
export const isValidPassword = (s: string) => s.length >= 10 && s.length <= 128

/** Whole years between an ISO date (YYYY-MM-DD) and `now`; null for malformed input. */
export function ageFromBirthDate(iso: string, now: Date = new Date()): number | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso)
  if (!m) return null
  const [y, mo, d] = [Number(m[1]), Number(m[2]), Number(m[3])]
  const probe = new Date(Date.UTC(y, mo - 1, d))
  if (probe.getUTCFullYear() !== y || probe.getUTCMonth() !== mo - 1 || probe.getUTCDate() !== d) {
    return null
  }
  let age = now.getFullYear() - y
  const month = now.getMonth() + 1
  if (month < mo || (month === mo && now.getDate() < d)) age -= 1
  return age
}

export const isAdult = (iso: string, now: Date = new Date()) => {
  const a = ageFromBirthDate(iso, now)
  return a !== null && a >= 18
}

export type PhotoCheck = 'ok' | 'type' | 'size'
export function checkPhotoFile(f: { type: string; size: number }): PhotoCheck {
  if (!PHOTO_TYPES.includes(f.type)) return 'type'
  if (f.size > MAX_PHOTO_BYTES) return 'size'
  return 'ok'
}

export function moveItem<T>(list: T[], from: number, to: number): T[] {
  if (to < 0 || to >= list.length || from === to) return list
  const copy = [...list]
  const [item] = copy.splice(from, 1)
  copy.splice(to, 0, item)
  return copy
}

export const AREA_NAME_MAX = 80
export const AREA_RADIUS_MIN = 50
export const AREA_RADIUS_MAX = 5000

export type AreaField = 'name' | 'lat' | 'lon' | 'radius_m'

const inRange = (n: number, min: number, max: number) => Number.isFinite(n) && n >= min && n <= max

/** Fields of an area form that the server would reject (docs/api.md, Admin). */
export function invalidAreaFields(a: {
  name: string
  lat: number
  lon: number
  radius_m: number
}): AreaField[] {
  const bad: AreaField[] = []
  const name = a.name.trim()
  // Code points, like the server's `chars().count()`: an emoji is one character, not two.
  const length = [...name].length
  if (length < 1 || length > AREA_NAME_MAX) bad.push('name')
  if (!inRange(a.lat, -90, 90)) bad.push('lat')
  if (!inRange(a.lon, -180, 180)) bad.push('lon')
  if (!Number.isInteger(a.radius_m) || !inRange(a.radius_m, AREA_RADIUS_MIN, AREA_RADIUS_MAX)) {
    bad.push('radius_m')
  }
  return bad
}
