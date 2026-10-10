import { del, get, post, put, request } from './client'
import type {
  AdminReport,
  AdminSettings,
  AdminUserRow,
  Area,
  AreaInput,
  AuditEntry,
  Impersonation,
  Photo,
  ReportStatus,
  TestUserInput,
} from './types'

export const getReports = (status?: ReportStatus) =>
  get<AdminReport[]>('/admin/reports', { status })
export const dismissReport = (id: string) => post<void>(`/admin/reports/${id}/dismiss`)
export const banUser = (id: string) => post<void>(`/admin/users/${id}/ban`)
export const unbanUser = (id: string) => post<void>(`/admin/users/${id}/unban`)

export const getAdminAreas = () => get<Area[]>('/admin/areas')
export const createArea = (body: AreaInput) => post<Area>('/admin/areas', body)
export const updateArea = (id: string, body: AreaInput) => put<Area>(`/admin/areas/${id}`, body)
export const deleteArea = (id: string) => del(`/admin/areas/${id}`)

export const getSettings = () => get<AdminSettings>('/admin/settings')
/** An empty `q` lists the newest accounts. */
export const searchUsers = (q: string) =>
  get<AdminUserRow[]>('/admin/users', { q: q.trim() || undefined })
export const listTestUsers = () => get<AdminUserRow[]>('/admin/test-users')
export const createTestUser = (body: TestUserInput) => post<AdminUserRow>('/admin/test-users', body)
export function uploadTestUserPhoto(id: string, file: File): Promise<Photo> {
  const form = new FormData()
  form.append('file', file)
  return request<Photo>(`/admin/test-users/${id}/photos`, { method: 'POST', form })
}
export const deleteTestUser = (id: string) => del(`/admin/test-users/${id}`)
export const impersonate = (id: string) => post<Impersonation>(`/admin/users/${id}/impersonate`)
export const listAudit = () => get<AuditEntry[]>('/admin/audit')
