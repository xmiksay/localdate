# API contract

Base path `/api`, JSON bodies, `Authorization: Bearer <access_token>` on everything except
`/auth/*`, `/interests` and `/push/config`. Timestamps RFC 3339 UTC. IDs are UUID strings unless noted.

Every authenticated request checks the account: a deleted account gets `401 unauthorized`,
a banned one `403 banned`, and an access token issued before the account's last password reset or change
`401 unauthorized` — the 15-minute access token is no grace period. An impersonation token (`act` claim,
[auth](api/auth.md#impersonation-tokens)) also checks the acting admin on every request.

## Sections

- [Auth](api/auth.md) — password, email magic link, password reset, Google / Telegram / Facebook
- [Me / profile](api/profile.md) — profile, photos, filter, identities, password change
- [Discovery](api/discovery.md) — visibility window & location, areas, nearby
- [Social](api/social.md) — waves & matches, safety
- [Admin (moderation)](api/admin.md) — reports, bans, areas, test users, impersonation, audit log
- [WebSocket `/api/ws`](api/realtime.md) — realtime protocol and close codes
- [Push notifications (Web Push)](api/push.md) — config, subscriptions, preferences, payload

## Errors

Every non-2xx response: `{ "error": { "code": "snake_case_code", "message": "human readable" } }`.
Internal/DB errors are logged and returned as `500 internal` with a generic message — never leaked.

| Status | code | When |
|---|---|---|
| 400 | `validation` | body fails validation (`message` says which field) |
| 400 | `invalid_token` | email token unknown, expired, already used, of another purpose, or (link confirm) issued to another account; also password-reset tokens; OAuth code / sign-up token likewise (or exchanged without the browser's flow cookie) |
| 401 | `unauthorized` | missing/invalid/expired access token, or one issued before the last password reset/change |
| 401 | `invalid_credentials` | login failed; `PUT /me/password` with a wrong `current_password` |
| 401 | `invalid_refresh_token` | refresh token unknown, expired or revoked |
| 403 | `forbidden` | not a participant / blocked / match partner banned; `/admin/*` for non-admins |
| 403 | `impersonation_forbidden` | the request uses an impersonation token (`act` claim) on an endpoint an admin acting as someone may not use — see [admin](api/admin.md#users-test-users-and-impersonation-alpha--beta-testing) |
| 403 | `banned` | account suspended: any authenticated request, login (after a correct password), refresh, password reset |
| 404 | `not_found` | |
| 409 | `username_taken` | register, `POST /auth/email/signup`, `POST /auth/oauth/signup` |
| 409 | `last_login_method` | `DELETE /me/identities/{id}` would leave the account with no password and no identity |
| 409 | `no_active_window` | `/nearby`, `/waves`, `/me/location`, `PATCH /me/window` without own active window |
| 409 | `outside_area` | `POST /me/window` with `kind: 'area'` from a point outside the area's circle |
| 409 | `too_close_to_midnight` | `POST /me/window` with `until: 'end_of_day'` less than 30 min before local midnight |
| 409 | `left_area` | `POST /me/location` beyond the area's leave margin (see [location](api/discovery.md#visibility-window--location)): the area window was ended |
| 409 | `area_in_use` | `DELETE /admin/areas/{id}` while a window row (running, or ended < 24 h ago) references it |
| 409 | `not_visible` | wave target is not currently mutually visible |
| 409 | `push_disabled` | `POST /me/push/subscriptions` while the server has no VAPID keys (push off) |
| 409 | `cannot_ban_admin` | `POST /admin/users/{id}/ban` on an admin (incl. yourself) |
| 409 | `cannot_impersonate` | `POST /admin/users/{id}/impersonate` on an admin (incl. yourself) or a banned account |
| 409 | `already_resolved` | `POST /admin/reports/{id}/dismiss` on a resolved report |
| 422 | `underage` | birth date < 18 years ago |
| 422 | `profile_incomplete` | window start without profile + filter + ≥ 1 photo |
| 422 | `photo_limit` | 7th photo |
| 422 | `unsupported_image` | not decodable jpeg/png/webp, > 10 MB, an edge > 10 000 px or > 32 Mi pixels (~33 MP) |
| 429 | `rate_limited` | per-IP throttle: login, register, `/auth/email/start`, `/auth/email/signup`, `/auth/password/forgot`, `POST /me/identities/email`, `PUT /me/password`, `/auth/oauth/{provider}/start` · `/link`, `/auth/oauth/signup` (never the callback: refusing it would waste the provider's code; its flow was counted at start) |
| 429 | `wave_limit` | > 20 waves in one window |
| 503 | `email_disabled` | any email endpoint while the server has no mailer (`GET /auth/providers` → `email: false`); `/auth/password/forgot` with an email address while there is no mailer, or with a username while there is neither a mailer nor a Telegram bot (`password_reset: false`) |
| 503 | `provider_disabled` | `POST /auth/oauth/{provider}/link` while that provider is not configured (`GET /auth/providers` → `google` / `telegram` / `facebook: false`) |

## Shared types

```ts
type Gender = 'male' | 'female' | 'other'
type Reason = 'date' | 'meet'
type DistanceBand = 'lt_200m' | 'lt_500m' | 'lt_1km' | 'lt_2km' | 'lt_5km' | 'lt_10km'
type WaveState = 'none' | 'sent' | 'received' | 'matched'
type WindowKind = 'timed' | 'area'
type AreaKind = 'city_centre' | 'train_station' | 'venue' | 'other'

interface User { id: string; username: string; created_at: string }
interface Photo { id: string; url: string /* "/media/<uuid>.webp" */; position: number }
interface Interest { id: number; key: string /* e.g. "hiking" → i18n "interest.hiking" */ }
interface Profile {
  display_name: string; birth_date: string /* YYYY-MM-DD, own profile only */; age: number
  gender: Gender; bio: string; interests: Interest[]; photos: Photo[]
}
interface Filter {
  max_distance_m: number; genders: Gender[] /* [] = any */; age_min: number; age_max: number
  reasons: Reason[]; default_window_minutes: 30 | 60 | 120 | 240
}
interface AreaRef { id: string; name: string }
interface Area {
  id: string; name: string; kind: AreaKind
  lat: number; lon: number      // centre of a public place, never a user's position
  radius_m: number; active: boolean; created_at: string
}
interface Window {
  id: string; kind: WindowKind; area: AreaRef | null /* set iff kind = 'area' */
  starts_at: string; ends_at: string; waves_left: number
}
interface NearbyProfile {
  user_id: string; display_name: string; age: number; gender: Gender; bio: string
  interests: Interest[]; photos: Photo[]; reasons: Reason[]
  shared_interests: number[] /* Interest ids the viewer has too, ascending; [] = none */
  distance_band: DistanceBand | null /* null for area matches (deliberate: the area is the place) */
  area: AreaRef | null /* the shared area when both windows are area windows, else null */
  wave_state: WaveState; match_id: string | null
}
interface Message { id: string; match_id: string; sender_id: string; body: string; created_at: string }
interface MatchSummary {
  match_id: string; created_at: string
  other: { user_id: string; display_name: string; photo_url: string | null }
  last_message: Message | null
}
interface Tokens { access_token: string; refresh_token: string; user: User }
type IdentityProvider = 'email' | 'google' | 'telegram' | 'facebook'
type OAuthProvider = 'google' | 'telegram' | 'facebook'   // providers signed in through /auth/oauth/{provider}
type PhotoImportOutcome = 'pending' | 'imported' | 'full' | 'none' | 'failed'   // `photo=` on /auth/oauth/done (Facebook)
interface Identity {
  id: string; provider: IdentityProvider
  subject: string            // email: the normalized address (it is the caller's own, shown in full)
                             // google: Google's opaque account id (`sub`) — not meant for display
                             // telegram: the numeric Telegram user id (`id` claim) in decimal — not shown either
                             // facebook: the app-scoped user id from Graph `/me` — not meant for display
  verified_at: string; created_at: string
}
type EmailTokenPurpose = 'login' | 'signup' | 'link'
interface EmailPreview {
  purpose: EmailTokenPurpose
  username: string | null   // login: the account it logs into · link: the account that asked · signup: null
  email: string
}
type MailLang = 'cs' | 'en'   // language of the sent email; anything else / missing → 'cs'
type OAuthExchange =
  | { session: Tokens }                                          // existing account, logged in
  | { signup: { token: string; provider: OAuthProvider; expires_at: string } } // new: pick a username
```

## Interest keys (seeded by migration, ids 1..40 in this order; FE translates `interest.<key>`)

`hiking`, `running`, `cycling`, `climbing`, `swimming`, `yoga`, `gym`, `football`, `tennis`, `skiing`,
`music`, `concerts`, `festivals`, `dancing`, `singing`, `guitar`, `cinema`, `theatre`, `art`, `photography`,
`reading`, `writing`, `gaming`, `board_games`, `tech`, `science`, `travel`, `languages`, `cooking`, `coffee`,
`wine`, `beer`, `food`, `nature`, `animals`, `volunteering`, `fashion`, `meditation`, `history`, `cars`
