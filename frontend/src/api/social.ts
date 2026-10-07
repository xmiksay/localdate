import { del, get, post } from './client'
import type {
  BlockedUser,
  Message,
  MatchSummary,
  NearbyProfile,
  ReportReason,
  WaveResult,
} from './types'

export const sendWave = (to_user_id: string) => post<WaveResult>('/waves', { to_user_id })
export const getIncomingWaves = () => get<NearbyProfile[]>('/waves/incoming')

export const getMatches = () => get<MatchSummary[]>('/matches')
export const getMessages = (matchId: string, before?: string, limit = 50) =>
  get<Message[]>(`/matches/${matchId}/messages`, { before, limit })
export const sendMessage = (matchId: string, body: string) =>
  post<Message>(`/matches/${matchId}/messages`, { body })

export const getBlocks = () => get<BlockedUser[]>('/blocks')
export const blockUser = (user_id: string) => post<void>('/blocks', { user_id })
export const unblockUser = (userId: string) => del(`/blocks/${userId}`)
export const reportUser = (user_id: string, reason: ReportReason, note?: string) =>
  post<void>('/reports', { user_id, reason, note })
