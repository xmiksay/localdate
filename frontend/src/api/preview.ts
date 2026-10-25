import { request } from './client'
import type { Gender, Preview } from './types'

/** Guest preview around the point, for a visitor without an account; `genders` empty = any. */
export const getPreview = (lat: number, lon: number, gender: Gender, genders: Gender[]) =>
  request<Preview>('/preview', {
    anon: true,
    query: { lat, lon, gender, genders: genders.join(',') || undefined },
  })
