import { get, post } from './client'
import type { AdminReport, ReportStatus } from './types'

export const getReports = (status?: ReportStatus) =>
  get<AdminReport[]>('/admin/reports', { status })
export const dismissReport = (id: string) => post<void>(`/admin/reports/${id}/dismiss`)
export const banUser = (id: string) => post<void>(`/admin/users/${id}/ban`)
export const unbanUser = (id: string) => post<void>(`/admin/users/${id}/unban`)
