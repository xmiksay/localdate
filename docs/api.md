# API contract

Base path `/api`, JSON bodies, `Authorization: Bearer <access_token>` on everything except
`/auth/*` and `/interests`. Timestamps RFC 3339 UTC. IDs are UUID strings unless noted.

Every authenticated request checks the account: a deleted account gets `401 unauthorized`,
a banned one `403 banned` — the 15-minute access token is no grace period.

## Errors

Every non-2xx response: `{ "error": { "code": "snake_case_code", "message": "human readable" } }`.
Internal/DB errors are logged and returned as `500 internal` with a generic message — never leaked.

| Status | code | When |
|---|---|---|
| 400 | `validation` | body fails validation (`message` says which field) |
| 401 | `unauthorized` | missing/invalid/expired access token |
| 401 | `invalid_credentials` | login failed |
| 401 | `invalid_refresh_token` | refresh token unknown, expired or revoked |
| 403 | `forbidden` | not a participant / blocked / match partner banned; `/admin/*` for non-admins |
| 403 | `banned` | account suspended: any authenticated request, login (after a correct password), refresh |
| 404 | `not_found` | |
| 409 | `username_taken` | register |
| 409 | `no_active_window` | `/nearby`, `/waves`, `/me/location`, `PATCH /me/window` without own active window |
| 409 | `not_visible` | wave target is not currently mutually visible |
| 409 | `cannot_ban_admin` | `POST /admin/users/{id}/ban` on an admin (incl. yourself) |
| 409 | `already_resolved` | `POST /admin/reports/{id}/dismiss` on a resolved report |
| 422 | `underage` | birth date < 18 years ago |
| 422 | `profile_incomplete` | window start without profile + filter + ≥ 1 photo |
| 422 | `photo_limit` | 7th photo |
| 422 | `unsupported_image` | not decodable jpeg/png/webp or > 10 MB |
| 429 | `rate_limited` | login/register throttle |
| 429 | `wave_limit` | > 20 waves in one window |

## Shared types

```ts
type Gender = 'male' | 'female' | 'other'
type Reason = 'date' | 'meet'
type DistanceBand = 'lt_200m' | 'lt_500m' | 'lt_1km' | 'lt_2km' | 'lt_5km' | 'lt_10km'
type WaveState = 'none' | 'sent' | 'received' | 'matched'

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
interface Window { id: string; kind: 'timed'; starts_at: string; ends_at: string; waves_left: number }
interface NearbyProfile {
  user_id: string; display_name: string; age: number; gender: Gender; bio: string
  interests: Interest[]; photos: Photo[]; reasons: Reason[]
  shared_interests: number[] /* Interest ids the viewer has too, ascending; [] = none */
  distance_band: DistanceBand; wave_state: WaveState; match_id: string | null
}
interface Message { id: string; match_id: string; sender_id: string; body: string; created_at: string }
interface MatchSummary {
  match_id: string; created_at: string
  other: { user_id: string; display_name: string; photo_url: string | null }
  last_message: Message | null
}
interface Tokens { access_token: string; refresh_token: string; user: User }
```

## Auth

| Method & path | Body | 2xx response |
|---|---|---|
| `POST /auth/register` | `{ username, password }` | `201 Tokens` |
| `POST /auth/login` | `{ username, password }` | `200 Tokens` |
| `POST /auth/refresh` | `{ refresh_token }` | `200 Tokens` (old token revoked) |
| `POST /auth/logout` | `{ refresh_token }` | `204` |

Username: trimmed, lowercased, `[a-z0-9_]{3,32}`. Password: 10–128 chars.

## Me / profile

| Method & path | Body | 2xx response |
|---|---|---|
| `GET /me` | — | `{ user: User, profile: Profile \| null, filter: Filter \| null, is_admin: boolean }` |
| `DELETE /me` | — | `204` — hard delete everything incl. photo files |
| `PUT /me/profile` | `{ display_name, birth_date, gender, bio, interest_ids: number[] }` | `200 Profile` |
| `GET /interests` | — | `200 Interest[]` |
| `POST /me/photos` | multipart field `file` | `201 Photo` (appended at last position) |
| `DELETE /me/photos/{id}` | — | `204` (positions compacted) |
| `PUT /me/photos/order` | `{ photo_ids: string[] }` (must be exactly the user's photos) | `200 Photo[]` |
| `GET /me/filter` | — | `200 Filter` (defaults if never saved: 2000 m, [], 18, 99, [date, meet], 60) |
| `PUT /me/filter` | `Filter` | `200 Filter` |

Validation: display_name 1–40, bio ≤ 500, interest_ids ≤ 10 and existing, age_min ≤ age_max,
reasons non-empty.

## Visibility window & location

| Method & path | Body | 2xx response |
|---|---|---|
| `GET /me/window` | — | `200 Window \| null` |
| `POST /me/window` | `{ minutes: 30\|60\|120\|240, lat, lon }` | `201 Window` (ends any previous active window) |
| `PATCH /me/window` | `{ extend_minutes: 30\|60\|120\|240 }` | `200 Window` (max total 12 h from now) |
| `DELETE /me/window` | — | `204` (sets `ended_at`, deletes its pending waves) |
| `POST /me/location` | `{ lat, lon }` | `204` |

lat ∈ [-90, 90], lon ∈ [-180, 180]; stored rounded to 3 decimals.
Client updates location every 2 min or after moving > 100 m while a window is active.

## Nearby

`GET /nearby` → `200 NearbyProfile[]`, sorted by number of `shared_interests` (most first), then
distance band (nearest first), then most recent window start, then `user_id`.
Shared interests only rank and highlight people — they never affect who is visible.
Applies every rule in architecture.md "Mutual filters". Excludes self.

Band = smallest of 200 / 500 / 1000 / 2000 / 5000 / 10000 m that the real distance is below.

## Waves & matches

| Method & path | Body | 2xx response |
|---|---|---|
| `POST /waves` | `{ to_user_id }` | `200 { matched: boolean, match_id: string \| null }` |
| `GET /waves/incoming` | — | `200 NearbyProfile[]` — senders with a pending (unexpired) wave to me who are still visible, same order as `/nearby` |
| `GET /matches` | — | `200 MatchSummary[]` newest activity first, excluding blocked and banned partners |
| `GET /matches/{id}/messages?before=<message_id>&limit=50` | — | `200 Message[]` newest first, limit ≤ 100 |
| `POST /matches/{id}/messages` | `{ body }` | `201 Message` |

Waving twice at the same person within a window is idempotent (no extra count).
If the target already has a pending wave to the sender, a match is created (idempotent on the pair)
and both waves are deleted.

## Safety

| Method & path | Body | 2xx response |
|---|---|---|
| `GET /blocks` | — | `200 { user_id, display_name, created_at }[]` — users I blocked, newest first |
| `POST /blocks` | `{ user_id }` | `204` (idempotent) |
| `DELETE /blocks/{user_id}` | — | `204` |
| `POST /reports` | `{ user_id, reason: 'spam'\|'harassment'\|'fake'\|'underage'\|'other', note?: string ≤ 1000 }` | `201` empty body — also blocks; blank note stored as null |

## Admin (moderation)

Admins only (`is_admin`, granted with `localdate-api admin grant <username>`); everyone else gets
`403 forbidden`. Coordinates and birth dates are never exposed here either.

```ts
type ReportStatus = 'open' | 'resolved'
type Resolution = 'dismissed' | 'banned'
interface UserRef { id: string; username: string }
interface AdminReport {
  id: string; reason: ReportReason; note: string | null; created_at: string
  resolved_at: string | null; resolution: Resolution | null
  resolved_by: UserRef | null   // null while open, or when that admin's account was deleted
  reporter: UserRef | null      // null when the reporter deleted their account
  subject: {
    id: string; username: string; display_name: string | null; photo_url: string | null
    banned_at: string | null; is_admin: boolean
    open_reports: number        // open reports against this subject, this one included
  }
}
```

| Method & path | Body | 2xx response |
|---|---|---|
| `GET /admin/reports?status=open\|resolved` | — | `200 AdminReport[]`, at most 200. No `status` = both, open first. Open: oldest first (queue order); resolved: most recently resolved first |
| `POST /admin/reports/{id}/dismiss` | — | `204` — resolves this one report as `dismissed`; `409 already_resolved`, `404` unknown |
| `POST /admin/users/{id}/ban` | — | `204` — soft ban (idempotent): see below; `409 cannot_ban_admin`, `404` unknown |
| `POST /admin/users/{id}/unban` | — | `204` — clears `banned_at` only (idempotent); `404` unknown |

Ban: sets `banned_at` (kept if already banned), revokes every refresh token, ends the active window
(coordinates wiped, its waves deleted), resolves all open reports against the user as `banned`
(`resolved_by` = the admin), and closes the user's WebSockets with `4403`. A banned user is invisible in
`/nearby` and `/waves/incoming`, missing from other users' `/matches`, and messaging them
(either direction) is `403 forbidden`. Reports against an account that is deleted disappear with it.

## WebSocket `/api/ws`

Client connects, then sends `{ "type": "auth", "token": "<access_token>" }` within 10 s.
Invalid/expired token or timeout → server closes with code `4401` (client refreshes the token before reconnecting).
A banned account → `4403`, both at auth time and for open sockets when the ban happens (client logs out,
no reconnect). A server-side failure while checking the account → `1011` (client retries with backoff).
Server → client events:

```ts
{ type: 'ready' }
{ type: 'message', message: Message }
{ type: 'match', match: MatchSummary }
{ type: 'wave', from_user_id: string }
```

Client reconnects with backoff and re-authenticates with a fresh token.

## Interest keys (seeded by migration, ids 1..40 in this order; FE translates `interest.<key>`)

`hiking`, `running`, `cycling`, `climbing`, `swimming`, `yoga`, `gym`, `football`, `tennis`, `skiing`,
`music`, `concerts`, `festivals`, `dancing`, `singing`, `guitar`, `cinema`, `theatre`, `art`, `photography`,
`reading`, `writing`, `gaming`, `board_games`, `tech`, `science`, `travel`, `languages`, `cooking`, `coffee`,
`wine`, `beer`, `food`, `nature`, `animals`, `volunteering`, `fashion`, `meditation`, `history`, `cars`
