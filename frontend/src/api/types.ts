// Mirrors docs/api.md — change the contract there first.
export type Gender = 'male' | 'female' | 'other'
export type Reason = 'date' | 'meet'
export type DistanceBand = 'lt_200m' | 'lt_500m' | 'lt_1km' | 'lt_2km' | 'lt_5km' | 'lt_10km'
export type WaveState = 'none' | 'sent' | 'received' | 'matched'
export type WindowMinutes = 30 | 60 | 120 | 240
/** How long a new window runs: preset minutes, or until the user's local midnight. */
export type WindowDuration = WindowMinutes | 'end_of_day'
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
  | 'invalid_token'
  | 'unauthorized'
  | 'invalid_credentials'
  | 'invalid_refresh_token'
  | 'forbidden'
  | 'banned'
  | 'not_found'
  | 'username_taken'
  | 'last_login_method'
  | 'no_active_window'
  | 'outside_area'
  | 'left_area'
  | 'too_close_to_midnight'
  | 'area_in_use'
  | 'push_disabled'
  | 'not_visible'
  | 'cannot_ban_admin'
  | 'already_resolved'
  | 'underage'
  | 'profile_incomplete'
  | 'photo_limit'
  | 'unsupported_image'
  | 'rate_limited'
  | 'wave_limit'
  | 'email_disabled'
  | 'provider_disabled'
  | 'internal'

/** Client-side only codes, never sent by the server. */
export type ClientErrorCode = 'network' | 'unknown'

export const ERROR_CODES: (ErrorCode | ClientErrorCode)[] = [
  'validation',
  'invalid_token',
  'unauthorized',
  'invalid_credentials',
  'invalid_refresh_token',
  'forbidden',
  'banned',
  'not_found',
  'username_taken',
  'last_login_method',
  'no_active_window',
  'outside_area',
  'left_area',
  'too_close_to_midnight',
  'area_in_use',
  'push_disabled',
  'not_visible',
  'cannot_ban_admin',
  'already_resolved',
  'underage',
  'profile_incomplete',
  'photo_limit',
  'unsupported_image',
  'rate_limited',
  'wave_limit',
  'email_disabled',
  'provider_disabled',
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
/** Providers signed in through `/auth/oauth/{provider}`; the one list the UI iterates. */
export const OAUTH_PROVIDERS = ['google', 'telegram'] as const
export type OAuthProvider = (typeof OAUTH_PROVIDERS)[number]
export type IdentityProvider = 'email' | OAuthProvider
/** Language of a sent email; the server falls back to 'cs'. */
export type MailLang = 'cs' | 'en'
export interface Identity {
  id: string
  provider: IdentityProvider
  /**
   * email: the caller's own normalized address · google: its opaque `sub` · telegram: the numeric
   * Telegram user id. OAuth subjects are never shown.
   */
  subject: string
  verified_at: string
  created_at: string
}
export interface IdentitiesResponse {
  has_password: boolean
  identities: Identity[]
}
/** Login methods the server offers (`GET /auth/providers`). */
/** `password_reset`: a reset link can be sent (mailer, or the Telegram bot — then by username only). */
export type Providers = { email: boolean; password_reset: boolean } & Record<OAuthProvider, boolean>
/** `POST /auth/oauth/exchange`: a session for a known account, or a token to pick a username. */
export type OAuthExchange =
  { session: Tokens } | { signup: { token: string; provider: OAuthProvider; expires_at: string } }
/** `#error=` codes the OAuth callback hands to `/auth/oauth/done`. */
export const OAUTH_ERRORS = [
  'cancelled',
  'invalid_state',
  'oauth_failed',
  'provider_disabled',
  'banned',
  'identity_taken',
  'unauthorized',
  'rate_limited',
  'internal',
] as const
export type OAuthError = (typeof OAUTH_ERRORS)[number]
export type EmailTokenPurpose = 'login' | 'signup' | 'link'
/** What a mailed link would do (`POST /auth/email/preview`); never consumes the token. */
export interface EmailPreview {
  purpose: EmailTokenPurpose
  /** login: the account it logs into · link: the account that asked · signup: null */
  username: string | null
  email: string
}
/** Who a password-reset link is for (`POST /auth/password/reset/preview`); never consumes it. */
export interface ResetPreview {
  username: string
}
/** `PUT /me/password`; `current_password` only when the account already has a password. */
export interface PasswordChange {
  current_password?: string
  new_password: string
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

export interface PushConfig {
  enabled: boolean
  /** VAPID key, base64url: the `applicationServerKey` to subscribe with */
  public_key: string | null
}
export type PushLang = 'cs' | 'en'
export interface PushSubscriptionBody {
  endpoint: string
  keys: { p256dh: string; auth: string }
  lang?: PushLang
}
export interface PushPrefs {
  waves: boolean
  matches: boolean
  messages: boolean
}
