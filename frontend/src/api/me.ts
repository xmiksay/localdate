import { del, get, put, request } from './client'
import type { Filter, Interest, MeResponse, Photo, Profile, ProfileInput } from './types'

export const getMe = () => get<MeResponse>('/me')
export const deleteMe = () => del('/me')
export const putProfile = (p: ProfileInput) => put<Profile>('/me/profile', p)
export const getInterests = () => request<Interest[]>('/interests', { anon: true })

export function uploadPhoto(file: File): Promise<Photo> {
  const form = new FormData()
  form.append('file', file)
  return request<Photo>('/me/photos', { method: 'POST', form })
}
export const deletePhoto = (id: string) => del(`/me/photos/${id}`)
export const putPhotoOrder = (photo_ids: string[]) =>
  put<Photo[]>('/me/photos/order', { photo_ids })

export const getFilter = () => get<Filter>('/me/filter')
export const putFilter = (f: Filter) => put<Filter>('/me/filter', f)
