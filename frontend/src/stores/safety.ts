import { ref } from 'vue'
import { defineStore } from 'pinia'
import * as socialApi from '@/api/social'
import * as meApi from '@/api/me'
import type { BlockedUser, ReportReason } from '@/api/types'
import { useAuthStore } from './auth'
import { useMatchesStore } from './matches'
import { useNearbyStore } from './nearby'

export const useSafetyStore = defineStore('safety', () => {
  const blocked = ref<BlockedUser[]>([])

  /** Both block and report hide the user server-side; mirror that locally right away. */
  function dropUser(userId: string) {
    useNearbyStore().removeUser(userId)
    useMatchesStore().removeUser(userId)
  }

  async function loadBlocked() {
    blocked.value = await socialApi.getBlocks()
  }

  async function block(userId: string) {
    await socialApi.blockUser(userId)
    dropUser(userId)
  }

  async function report(userId: string, reason: ReportReason, note?: string) {
    await socialApi.reportUser(userId, reason, note || undefined)
    dropUser(userId)
  }

  async function unblock(userId: string) {
    await socialApi.unblockUser(userId)
    blocked.value = blocked.value.filter((b) => b.user_id !== userId)
  }

  async function deleteAccount() {
    await meApi.deleteMe()
    useAuthStore().clear()
  }

  return { blocked, loadBlocked, block, report, unblock, deleteAccount }
})
