import { del, get, post, put } from './client'
import type { AdminReport, Area, AreaInput, ReportStatus } from './types'

export const getReports = (status?: ReportStatus) =>
  get<AdminReport[]>('/admin/reports', { status })
export const dismissReport = (id: string) => post<void>(`/admin/reports/${id}/dismiss`)
export const banUser = (id: string) => post<void>(`/admin/users/${id}/ban`)
export const unbanUser = (id: string) => post<void>(`/admin/users/${id}/unban`)

export const getAdminAreas = () => get<Area[]>('/admin/areas')
export const createArea = (body: AreaInput) => post<Area>('/admin/areas', body)
export const updateArea = (id: string, body: AreaInput) => put<Area>(`/admin/areas/${id}`, body)
export const deleteArea = (id: string) => del(`/admin/areas/${id}`)
