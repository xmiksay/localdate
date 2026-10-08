// Mirrors docs/api.md — change the contract there first.
export type Gender = 'male' | 'female' | 'other'
export type Reason = 'date' | 'meet'
export type DistanceBand = 'lt_200m' | 'lt_500m' | 'lt_1km' | 'lt_2km' | 'lt_5km' | 'lt_10km'
export type WaveState = 'none' | 'sent' | 'received' | 'matched'
export type WindowMinutes = 30 | 60 | 120 | 240
export type ReportReason = 'spam' | 'harassment' | 'fake' | 'underage' | 'other'

export const GENDERS: Gender[] = ['male', 'female', 'other']
export const REASONS: Reason[] = ['date', 'meet']
export const WINDOW_MINUTES: WindowMinutes[] = [30, 60, 120, 240]
export const REPORT_REASONS: ReportReason[] = ['spam', 'harassment', 'fake', 'underage', 'other']

export type ErrorCode =
  | 'validation'
  | 'unauthorized'
  | 'invalid_credentials'
  | 'invalid_refresh_token'
  | 'forbidden'
  | 'not_found'
  | 'username_taken'
  | 'no_active_window'
  | 'not_visible'
  | 'underage'
  | 'profile_incomplete'
  | 'photo_limit'
  | 'unsupported_image'
  | 'rate_limited'
  | 'wave_limit'
  | 'internal'

/** Client-side only codes, never sent by the server. */
export type ClientErrorCode = 'network' | 'unknown'

export const ERROR_CODES: (ErrorCode | ClientErrorCode)[] = [
  'validation',
  'unauthorized',
  'invalid_credentials',
  'invalid_refresh_token',
  'forbidden',
  'not_found',
  'username_taken',
  'no_active_window',
  'not_visible',
  'underage',
  'profile_incomplete',
  'photo_limit',
  'unsupported_image',
  'rate_limited',
  'wave_limit',
  'internal',
  'network',
  'unknown',
]

export interface User {
  id: string
  username: string
  created_at: string
}
export interface Photo {
  id: string
  url: string
  position: number
}
export interface Interest {
  id: number
  key: string
}
export interface Profile {
  display_name: string
  birth_date: string
  age: number
  gender: Gender
  bio: string
  interests: Interest[]
  photos: Photo[]
}
export interface ProfileInput {
  display_name: string
  birth_date: string
  gender: Gender
  bio: string
  interest_ids: number[]
}
export interface Filter {
  max_distance_m: number
  genders: Gender[]
  age_min: number
  age_max: number
  reasons: Reason[]
  default_window_minutes: WindowMinutes
}
export interface Window {
  id: string
  kind: 'timed'
  starts_at: string
  ends_at: string
  waves_left: number
}
export interface NearbyProfile {
  user_id: string
  display_name: string
  age: number
  gender: Gender
  bio: string
  interests: Interest[]
  /** Interest ids the viewer has too, ascending. */
  shared_interests: number[]
  photos: Photo[]
  reasons: Reason[]
  distance_band: DistanceBand
  wave_state: WaveState
  match_id: string | null
}
export interface Message {
  id: string
  match_id: string
  sender_id: string
  body: string
  created_at: string
}
export interface MatchSummary {
  match_id: string
  created_at: string
  other: { user_id: string; display_name: string; photo_url: string | null }
  last_message: Message | null
}
export interface Tokens {
  access_token: string
  refresh_token: string
  user: User
}
export interface MeResponse {
  user: User
  profile: Profile | null
  filter: Filter | null
}
export interface Credentials {
  username: string
  password: string
}
export interface BlockedUser {
  user_id: string
  display_name: string
  created_at: string
}
export interface WaveResult {
  matched: boolean
  match_id: string | null
}

export type WsEvent =
  | { type: 'ready' }
  | { type: 'message'; message: Message }
  | { type: 'match'; match: MatchSummary }
  | { type: 'wave'; from_user_id: string }
