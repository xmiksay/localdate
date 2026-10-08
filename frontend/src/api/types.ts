// Mirrors docs/api.md — change the contract there first.
export type Gender = 'male' | 'female' | 'other'
export type Reason = 'date' | 'meet'
export type DistanceBand = 'lt_200m' | 'lt_500m' | 'lt_1km' | 'lt_2km' | 'lt_5km' | 'lt_10km'
export type WaveState = 'none' | 'sent' | 'received' | 'matched'
export type WindowMinutes = 30 | 60 | 120 | 240
export type ReportReason = 'spam' | 'harassment' | 'fake' | 'underage' | 'other'
export type WindowKind = 'timed' | 'area'
export type AreaKind = 'city_centre' | 'train_station' | 'venue' | 'other'

export const GENDERS: Gender[] = ['male', 'female', 'other']
export const REASONS: Reason[] = ['date', 'meet']
export const WINDOW_MINUTES: WindowMinutes[] = [30, 60, 120, 240]
export const REPORT_REASONS: ReportReason[] = ['spam', 'harassment', 'fake', 'underage', 'other']
export const AREA_KINDS: AreaKind[] = ['city_centre', 'train_station', 'venue', 'other']

export type ErrorCode =
  | 'validation'
  | 'unauthorized'
  | 'invalid_credentials'
  | 'invalid_refresh_token'
  | 'forbidden'
  | 'banned'
  | 'not_found'
  | 'username_taken'
  | 'no_active_window'
  | 'outside_area'
  | 'left_area'
  | 'area_in_use'
  | 'not_visible'
  | 'cannot_ban_admin'
  | 'already_resolved'
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
  'banned',
  'not_found',
  'username_taken',
  'no_active_window',
  'outside_area',
  'left_area',
  'area_in_use',
  'not_visible',
  'cannot_ban_admin',
  'already_resolved',
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
export interface AreaRef {
  id: string
  name: string
}
export interface Area {
  id: string
  name: string
  kind: AreaKind
  /** Centre of a public place, never a user's position. */
  lat: number
  lon: number
  radius_m: number
  active: boolean
  created_at: string
}
/** Body of `POST /admin/areas` and `PUT /admin/areas/{id}`. */
export interface AreaInput {
  name: string
  kind: AreaKind
  lat: number
  lon: number
  radius_m: number
  active: boolean
}
export interface Window {
  id: string
  kind: WindowKind
  /** Set iff kind = 'area'. */
  area: AreaRef | null
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
  /** null for area matches: the shared area is the only place information. */
  distance_band: DistanceBand | null
  /** The shared area when both windows are area windows, else null. */
  area: AreaRef | null
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
  is_admin: boolean
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

export type ReportStatus = 'open' | 'resolved'
export type Resolution = 'dismissed' | 'banned'
export interface UserRef {
  id: string
  username: string
}
export interface AdminReport {
  id: string
  reason: ReportReason
  note: string | null
  created_at: string
  resolved_at: string | null
  resolution: Resolution | null
  /** null while open, or when that admin's account was deleted */
  resolved_by: UserRef | null
  /** null when the reporter deleted their account */
  reporter: UserRef | null
  subject: {
    id: string
    username: string
    display_name: string | null
    photo_url: string | null
    banned_at: string | null
    is_admin: boolean
    /** open reports against this subject, this one included */
    open_reports: number
  }
}

export type WsEvent =
  | { type: 'ready' }
  | { type: 'message'; message: Message }
  | { type: 'match'; match: MatchSummary }
  | { type: 'wave'; from_user_id: string }
