import { get } from './client'
import type { Area } from './types'

/** Active areas whose circle contains the point, nearest centre first. */
export const getAreas = (lat: number, lon: number) => get<Area[]>('/areas', { lat, lon })
