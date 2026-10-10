import { ref } from 'vue'
import { defineStore } from 'pinia'
import * as adminApi from '@/api/admin'
import type { AdminSettings, AdminUserRow, AuditEntry, TestUserInput } from '@/api/types'

/** Accounts, test users and the impersonation audit log, for the admin. */
export const useAdminUsersStore = defineStore('adminUsers', () => {
  const settings = ref<AdminSettings | null>(null)
  const testUsers = ref<AdminUserRow[]>([])
  const found = ref<AdminUserRow[]>([])
  const audit = ref<AuditEntry[]>([])
  let latestSearch = 0

  async function loadSettings() {
    settings.value ??= await adminApi.getSettings()
  }

  async function loadTestUsers() {
    testUsers.value = await adminApi.listTestUsers()
  }

  async function search(q: string) {
    // Typing fires overlapping searches: only the newest may land.
    const seq = ++latestSearch
    const rows = await adminApi.searchUsers(q)
    if (seq === latestSearch) found.value = rows
  }

  async function loadAudit() {
    audit.value = await adminApi.listAudit()
  }

  /**
   * Creates the account, then uploads `photo` when given (else the server generates an avatar).
   * The account exists even when the upload fails, so that failure is returned, not thrown.
   */
  async function createTestUser(input: TestUserInput, photo: File | null) {
    const row = await adminApi.createTestUser({ ...input, placeholder_photo: photo === null })
    testUsers.value = [row, ...testUsers.value]
    if (!photo) return { row, photoError: null }
    try {
      const p = await adminApi.uploadTestUserPhoto(row.id, photo)
      testUsers.value = testUsers.value.map((u) =>
        u.id === row.id ? { ...u, photo_url: p.url } : u,
      )
      return { row, photoError: null }
    } catch (e) {
      return { row, photoError: e }
    }
  }

  async function deleteTestUser(id: string) {
    await adminApi.deleteTestUser(id)
    testUsers.value = testUsers.value.filter((u) => u.id !== id)
    found.value = found.value.filter((u) => u.id !== id)
  }

  function reset() {
    latestSearch++
    settings.value = null
    testUsers.value = []
    found.value = []
    audit.value = []
  }

  return {
    settings,
    testUsers,
    found,
    audit,
    loadSettings,
    loadTestUsers,
    search,
    loadAudit,
    createTestUser,
    deleteTestUser,
    reset,
  }
})
