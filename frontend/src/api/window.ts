import { del, get, patch, post } from './client'
import type { NearbyProfile, Window, WindowMinutes } from './types'

export const getWindow = () => get<Window | null>('/me/window')
/** A timed window without `areaId`, an area window with it. */
export const startWindow = (minutes: WindowMinutes, lat: number, lon: number, areaId?: string) =>
  post<Window>(
    '/me/window',
    areaId ? { kind: 'area', area_id: areaId, minutes, lat, lon } : { minutes, lat, lon },
  )
export const extendWindow = (extend_minutes: WindowMinutes) =>
  patch<Window>('/me/window', { extend_minutes })
export const endWindow = () => del('/me/window')
/** `accuracy` (metres) widens the server's area leave margin for a coarse fix. */
export const updateLocation = (lat: number, lon: number, accuracy?: number) =>
  post<void>('/me/location', { lat, lon, accuracy })

export const getNearby = () => get<NearbyProfile[]>('/nearby')
