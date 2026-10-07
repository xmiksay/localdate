import { del, get, patch, post } from './client'
import type { NearbyProfile, Window, WindowMinutes } from './types'

export const getWindow = () => get<Window | null>('/me/window')
export const startWindow = (minutes: WindowMinutes, lat: number, lon: number) =>
  post<Window>('/me/window', { minutes, lat, lon })
export const extendWindow = (extend_minutes: WindowMinutes) =>
  patch<Window>('/me/window', { extend_minutes })
export const endWindow = () => del('/me/window')
export const updateLocation = (lat: number, lon: number) => post<void>('/me/location', { lat, lon })

export const getNearby = () => get<NearbyProfile[]>('/nearby')
