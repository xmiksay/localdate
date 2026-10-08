import { ref } from 'vue'
import { defineStore } from 'pinia'
import * as adminApi from '@/api/admin'
import type { AdminReport, ReportStatus } from '@/api/types'

export const useAdminStore = defineStore('admin', () => {
  const reports = ref<AdminReport[]>([])
  const status = ref<ReportStatus>('open')
  const loading = ref(false)
  let latest = 0

  async function load(s: ReportStatus) {
    // Quick tab switches: only the newest request may land, clear `loading` or report a failure.
    const seq = ++latest
    loading.value = true
    try {
      const list = await adminApi.getReports(s)
      if (seq !== latest) return
      status.value = s
      reports.value = list
    } catch (e) {
      if (seq === latest) throw e
    } finally {
      if (seq === latest) loading.value = false
    }
  }

  function setBanned(userId: string, bannedAt: string | null) {
    for (const r of reports.value) if (r.subject.id === userId) r.subject.banned_at = bannedAt
  }

  async function dismiss(id: string) {
    await adminApi.dismissReport(id)
    const gone = reports.value.find((r) => r.id === id)
    if (!gone || status.value !== 'open') return
    reports.value = reports.value.filter((r) => r.id !== id)
    for (const r of reports.value) if (r.subject.id === gone.subject.id) r.subject.open_reports--
  }

  async function ban(userId: string) {
    await adminApi.banUser(userId)
    // The server resolves every open report against the subject as part of the ban.
    if (status.value === 'open')
      reports.value = reports.value.filter((r) => r.subject.id !== userId)
    setBanned(userId, new Date().toISOString())
  }

  async function unban(userId: string) {
    await adminApi.unbanUser(userId)
    setBanned(userId, null)
  }

  function reset() {
    latest++
    loading.value = false
    reports.value = []
    status.value = 'open'
  }

  return { reports, status, loading, load, dismiss, ban, unban, reset }
})
