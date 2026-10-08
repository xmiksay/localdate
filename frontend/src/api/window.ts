import { del, get, patch, post } from './client'
import type { NearbyProfile, Window, WindowDuration, WindowMinutes } from './types'

/** Empty on runtimes that cannot name it; end of day is not offered then. */
export const deviceTimeZone = (): string | undefined =>
  Intl.DateTimeFormat().resolvedOptions().timeZone || undefined

/** End of day is the server's to compute, in the device's time zone. */
const durationBody = (duration: WindowDuration) =>
  duration === 'end_of_day' ? { until: 'end_of_day', tz: deviceTimeZone() } : { minutes: duration }

export const getWindow = () => get<Window | null>('/me/window')
/** A timed window without `areaId`, an area window with it. */
export const startWindow = (duration: WindowDuration, lat: number, lon: number, areaId?: string) =>
  post<Window>('/me/window', {
    ...(areaId ? { kind: 'area', area_id: areaId } : {}),
    ...durationBody(duration),
    lat,
    lon,
  })
export const extendWindow = (extend_minutes: WindowMinutes) =>
  patch<Window>('/me/window', { extend_minutes })
export const endWindow = () => del('/me/window')
/** `accuracy` (metres) widens the server's area leave margin for a coarse fix. */
export const updateLocation = (lat: number, lon: number, accuracy?: number) =>
  post<void>('/me/location', { lat, lon, accuracy })

export const getNearby = () => get<NearbyProfile[]>('/nearby')
