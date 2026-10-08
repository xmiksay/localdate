# API contract

Base path `/api`, JSON bodies, `Authorization: Bearer <access_token>` on everything except
`/auth/*`, `/interests` and `/push/config`. Timestamps RFC 3339 UTC. IDs are UUID strings unless noted.

Every authenticated request checks the account: a deleted account gets `401 unauthorized`,
a banned one `403 banned`, and an access token issued before the account's last password reset or change
`401 unauthorized` — the 15-minute access token is no grace period.

## Errors

Every non-2xx response: `{ "error": { "code": "snake_case_code", "message": "human readable" } }`.
Internal/DB errors are logged and returned as `500 internal` with a generic message — never leaked.

| Status | code | When |
|---|---|---|
| 400 | `validation` | body fails validation (`message` says which field) |
| 400 | `invalid_token` | email token unknown, expired, already used, of another purpose, or (link confirm) issued to another account; also password-reset tokens |
| 401 | `unauthorized` | missing/invalid/expired access token, or one issued before the last password reset/change |
| 401 | `invalid_credentials` | login failed; `PUT /me/password` with a wrong `current_password` |
| 401 | `invalid_refresh_token` | refresh token unknown, expired or revoked |
| 403 | `forbidden` | not a participant / blocked / match partner banned; `/admin/*` for non-admins |
| 403 | `banned` | account suspended: any authenticated request, login (after a correct password), refresh, password reset |
| 404 | `not_found` | |
| 409 | `username_taken` | register, `POST /auth/email/signup` |
| 409 | `last_login_method` | `DELETE /me/identities/{id}` would leave the account with no password and no identity |
| 409 | `no_active_window` | `/nearby`, `/waves`, `/me/location`, `PATCH /me/window` without own active window |
| 409 | `outside_area` | `POST /me/window` with `kind: 'area'` from a point outside the area's circle |
| 409 | `too_close_to_midnight` | `POST /me/window` with `until: 'end_of_day'` less than 30 min before local midnight |
| 409 | `left_area` | `POST /me/location` beyond the area's leave margin (see below): the area window was ended |
| 409 | `area_in_use` | `DELETE /admin/areas/{id}` while a window row (running, or ended < 24 h ago) references it |
| 409 | `not_visible` | wave target is not currently mutually visible |
| 409 | `push_disabled` | `POST /me/push/subscriptions` while the server has no VAPID keys (push off) |
| 409 | `cannot_ban_admin` | `POST /admin/users/{id}/ban` on an admin (incl. yourself) |
| 409 | `already_resolved` | `POST /admin/reports/{id}/dismiss` on a resolved report |
| 422 | `underage` | birth date < 18 years ago |
| 422 | `profile_incomplete` | window start without profile + filter + ≥ 1 photo |
| 422 | `photo_limit` | 7th photo |
| 422 | `unsupported_image` | not decodable jpeg/png/webp, > 10 MB, an edge > 10 000 px or > 32 Mi pixels (~33 MP) |
| 429 | `rate_limited` | per-IP throttle: login, register, `/auth/email/start`, `/auth/email/signup`, `/auth/password/forgot`, `POST /me/identities/email`, `PUT /me/password` |
| 429 | `wave_limit` | > 20 waves in one window |
| 503 | `email_disabled` | any email endpoint while the server has no mailer (`GET /auth/providers` → `email: false`), incl. `/auth/password/forgot` |

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
type IdentityProvider = 'email'   // later: 'telegram' | 'google' | 'facebook'
interface Identity {
  id: string; provider: IdentityProvider
  subject: string            // email: the normalized address (it is the caller's own, shown in full)
  verified_at: string; created_at: string
}
type EmailTokenPurpose = 'login' | 'signup' | 'link'
interface EmailPreview {
  purpose: EmailTokenPurpose
  username: string | null   // login: the account it logs into · link: the account that asked · signup: null
  email: string
}
type MailLang = 'cs' | 'en'   // language of the sent email; anything else / missing → 'cs'
```

## Auth

| Method & path | Body | 2xx response |
|---|---|---|
| `POST /auth/register` | `{ username, password }` | `201 Tokens` |
| `POST /auth/login` | `{ username, password }` | `200 Tokens` |
| `POST /auth/refresh` | `{ refresh_token }` | `200 Tokens` (old token revoked) |
| `POST /auth/logout` | `{ refresh_token }` | `204` |

Username: trimmed, lowercased, `[a-z0-9_]{3,32}`. Password: 10–128 chars.
An account created by email (or a later OAuth provider) has **no password**: password login for it
answers the same `401 invalid_credentials` as a wrong password (same argon2 timing).

### Login providers and email magic link

| Method & path | Body | 2xx response |
|---|---|---|
| `GET /auth/providers` | — | `200 { email: boolean }` — which login methods this server offers (more keys later) |
| `POST /auth/email/start` | `{ email, lang?: MailLang }` | `202` (empty) — always, whether or not the address has an account |
| `POST /auth/email/preview` | `{ token }` | `200 EmailPreview` — never consumes the token |
| `POST /auth/email/verify` | `{ token }` | `200 Tokens` — login tokens only, consumed |
| `POST /auth/email/signup` | `{ token, username }` | `201 Tokens` (token consumed, account + verified email identity created) |

**Email address** (identical rule on the client): trimmed + lowercased, ≤ 254 chars, exactly one `@`;
local part 1–64 of `a-z 0-9 . _ + -`, no leading/trailing/double dot; domain ≥ 2 dot-separated labels of
`a-z 0-9 -` (1–63 each, no leading/trailing `-`), the last (TLD) containing a letter. No display names, quotes, commas, angle brackets or
whitespace — else `400 validation`. The validated address is what is stored and mailed to, never the raw input.

`start` sends a **login link** when an email identity with that address exists, otherwise a **sign-up link** —
the response never tells which. Mails go out after the response (sending failures are only logged), so the
answer is always `202`. Limits (shared with `POST /me/identities/email`), over them still `202` and nothing sent:
3 mails per (address, client IP) per 15 min, and 10 per address per hour from all IPs together.

Links carry the token in the **URL fragment**, so it never reaches server or ingress logs:
`{APP_BASE_URL}/auth/email#token=…` (login / sign-up) and `{APP_BASE_URL}/auth/email/link#token=…` (linking,
below). Login and link mails name the account (`pro účet <username>`). Tokens: 32 random bytes base64url,
stored as sha256, valid 15 min, single use.

Nothing is consumed by opening a link: the page calls `preview` and shows an explicit button
("Přihlásit se jako <username>", "Propojit <email> s účtem <username>") before `verify` / `confirm` /
`signup`, so link scanners and prefetchers cannot spend the token. Deliberately **no browser binding**
(nonce cookie from the requesting browser): requesting on a laptop and clicking on the phone must keep working;
the 15-minute single-use token and the explicit confirmation are the protection.

`preview` / `verify` → `400 invalid_token` for unknown, expired or used tokens, a login token whose identity was
removed since or whose account is gone, and a sign-up token whose address has meanwhile become an account.
`verify` also refuses sign-up and link tokens (`400 invalid_token`); a banned account → `403 banned` (not
consumed — consumption, account check and session creation are one transaction).
`signup` validates the username like register (`400 validation`, `409 username_taken` — the token
stays usable for another name). The new account then goes through onboarding like a registered one
(birth date, 18+ check). A second sign-up for an address that already got an account → `400 invalid_token`.

### Password reset

| Method & path | Body | 2xx response |
|---|---|---|
| `POST /auth/password/forgot` | `{ login, lang?: MailLang }` — `login` is a username or an email address | `202` (empty) — always |
| `POST /auth/password/reset/preview` | `{ token }` | `200 { username: string }` — never consumes the token |
| `POST /auth/password/reset` | `{ token, new_password }` | `204` |

`login` containing `@` is validated as an email address, anything else as a username (`400 validation` for a
malformed one — that says nothing about whether it exists). The answer is `202` whether or not there is such an
account, and it is sent **before** the account is even looked up, so its timing reveals nothing either. Then:
an email address that is a linked email identity gets a reset link; a username gets one at every email address
linked to that account. Accounts without a linked email get nothing (Telegram delivery comes with #14) — the UI
says "if the account has a linked email…". `503 email_disabled` while the server has no mailer. Limits, a
budget of their own (asking for resets never uses up an inbox's magic-link mails, nor the other way round); over
them still `202` and nothing sent: 3 requests per (username or address as typed, client IP) per 15 min, and 5
reset mails per account per hour however they were asked for.

The link is `{APP_BASE_URL}/auth/password/reset#token=…` (fragment, as above), names the account
(`pro účet <username>`), and its token (purpose `password_reset`) is valid 15 min, single use. Opening it spends
nothing: the page calls `preview` and shows "Nastavit nové heslo pro <username>". `reset` checks the password
policy of register (`400 validation`, token unspent), then in one transaction consumes the token, sets the new
password, **ends every session** of the account — refresh tokens revoked, and access tokens issued before
it refused from then on (`401 unauthorized`, WebSocket `4401`), every push subscription of the account deleted
(a possibly stolen device stops getting notifications) — and voids every other unused mailed token of the account (login, link, reset) — a link sent
before must not undo or bypass the new password. After the commit the account's WebSockets are closed with
`4401`. It returns no tokens — the user logs in normally afterwards. Banned account → `403 banned`, token
unspent. `preview` / `reset` → `400 invalid_token` for unknown, expired, used or other-purpose tokens, and when
the address was unlinked from the account since the mail went out. Reset tokens are refused by
`/auth/email/preview` and `/auth/email/verify` (`400 invalid_token`), login tokens by the reset endpoints.

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
| `PUT /me/password` | `{ current_password?: string, new_password }` | `200 Tokens` — a fresh session; every other one is logged out |
| `GET /me/identities` | — | `200 { has_password: boolean, identities: Identity[] }` (oldest first) |
| `POST /me/identities/email` | `{ email, lang?: MailLang }` | `202` (empty) — always |
| `POST /me/identities/email/confirm` | `{ token }` | `201 Identity` |
| `DELETE /me/identities/{id}` | — | `204`; `409 last_login_method` if it is the only identity of a passwordless account; another user's / unknown id → `404` |

Linking: `POST /me/identities/email` mails a link token bound to the caller to that address — or, if the
address is already linked to any account (the caller's included), a short notice instead, so the answer
never reveals whose it is. Same validation, limits, fragment links, `preview` and `503 email_disabled` as
`/auth/email/start`. A logged-out click keeps the token in `sessionStorage` across the login, never in a query.
`confirm` must come from the account that requested the link: another account's token, or an expired / used
one → `400 invalid_token` (a wrong account does not consume it). If the address got linked elsewhere in the
meantime → `400 invalid_token`.

Password change: `new_password` follows the register policy (`400 validation`). An account that has a password
must send the right `current_password` (missing → `400 validation`, wrong → `401 invalid_credentials`); an
account without one (created by email) sets its first password without it (`current_password` ignored), after
which `has_password` is `true`. Every refresh token of the account is revoked — the access JWT does not say
which session made the call, so instead of guessing, the response carries new `Tokens` for the caller, who
replaces both tokens; other devices fall back to login. Like a reset it voids the account's unused mailed tokens
and refuses every older access token, closing the WebSockets with `4401` — the caller's own too, which
reconnects with the returned access token — and deletes the account's push subscriptions; the caller's client
re-registers its own device with the new session (the usual resync). Precision is whole seconds: a token issued in the same second as the
change still counts as newer (that is how the returned one stays valid), so only an access token minted earlier
within that very second outlives it. A client request that gets `401` with the old token while the answer is still
on its way retries with the new tokens instead of refreshing. A concurrent change between the check and the write → `401 invalid_credentials`.
Known trade-off: setting a *first* password needs only a valid access token (there is no old password to ask
for), so a stolen session of an email-only account can add one; the same session could already link an address.

Validation: display_name 1–40, bio ≤ 500, interest_ids ≤ 10 and existing, age_min ≤ age_max,
reasons non-empty.

## Visibility window & location

| Method & path | Body | 2xx response |
|---|---|---|
| `GET /me/window` | — | `200 Window \| null` |
| `POST /me/window` | `{ kind?: 'timed'\|'area', area_id?: string, minutes?: 30\|60\|120\|240, until?: 'end_of_day', tz?: string, lat, lon }` | `201 Window` (ends any previous active window) |
| `PATCH /me/window` | `{ extend_minutes: 30\|60\|120\|240 }` | `200 Window` (max total 12 h from now) |
| `DELETE /me/window` | — | `204` (sets `ended_at`, deletes its pending waves) |
| `POST /me/location` | `{ lat, lon, accuracy?: number /* metres, ≥ 0 */ }` | `204` |

Duration: exactly one of `minutes` or `until: 'end_of_day'` (with `tz`, an IANA zone name such as
`Europe/Prague` — the client sends `Intl.DateTimeFormat().resolvedOptions().timeZone`); `tz` without
`until`, both, neither or an unknown zone is `400 validation`. End of day works for both kinds and ends the
window at the next local midnight in `tz` — a midnight skipped by a DST change becomes the first valid
local time after it, an ambiguous one its first occurrence after now — but never more than 12 h from now (the cap
every window obeys). Less than 30 min before that midnight it is `409 too_close_to_midnight` (the client
hides the option then, and after such a refusal until midnight passes). It is only a way to set `ends_at`: the `Window` carries no trace of it and extend
works as for any window.

lat ∈ [-90, 90], lon ∈ [-180, 180]; stored rounded to 3 decimals.
Client updates location every 2 min or after moving > 100 m while a window is active.

`kind` defaults to `timed`. `kind: 'area'` needs `area_id` (and `timed` must not send one, `400 validation`);
an unknown or inactive area is `404 not_found`, a point farther than `radius_m` from the area centre is
`409 outside_area`. `minutes` is the area window's maximum length; extend works as for timed windows.
An area window ends by itself when a `POST /me/location` lands more than `radius_m` + margin from the
centre, margin = max(100 m, `accuracy`) with `accuracy` capped at 500 m (hysteresis, so GPS jitter or a
coarse fix at the edge does not end it): the window is ended exactly like `DELETE /me/window` and the
response is `409 left_area`; the next `GET /me/window` returns `null`. The client sends the fix's
`coords.accuracy`; a negative or non-finite `accuracy` is `400 validation`.
Distances are checked against the coordinates as sent, before rounding.
An area window whose location was last updated more than 10 min ago is **stale**: it is invisible to
others, and its owner's `/nearby` is empty, until the next location update (the client sends one at
least every 2 min).

## Areas

| Method & path | Body | 2xx response |
|---|---|---|
| `GET /areas?lat=&lon=` | — | `200 Area[]` — active areas whose circle contains the point (distance to centre ≤ `radius_m`), nearest centre first. The point is neither stored nor echoed. Missing/out-of-range `lat`/`lon` → `400 validation` |

## Nearby

`GET /nearby` → `200 NearbyProfile[]`, sorted by number of `shared_interests` (most first), then
distance band (nearest first), then most recent window start, then `user_id`.
Shared interests only rank and highlight people — they never affect who is visible.
Applies every rule in architecture.md "Mutual filters". Excludes self.
An area window sees only fresh area windows in the same area (`area` set on every item, `max_distance_m`
ignored, `distance_band: null` — deliberately, the shared area is the only place information; these
sort by shared interests, then newest window, then `user_id`); a timed window sees only timed windows
within distance (`area: null`, `distance_band` set).

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
| `GET /admin/areas` | — | `200 Area[]` — all areas, active first, then by name |
| `POST /admin/areas` | `{ name, kind: AreaKind, lat, lon, radius_m, active?: boolean /* default true */ }` | `201 Area` |
| `PUT /admin/areas/{id}` | `{ name, kind, lat, lon, radius_m, active }` | `200 Area`; `404` unknown |
| `DELETE /admin/areas/{id}` | — | `204`; `409 area_in_use`, `404` unknown |

Area validation: `name` trimmed 1–80 chars, `radius_m` an integer 50–5000, lat/lon in range (stored as sent, no
rounding). Deactivating (`active: false`) hides the area from `/areas` and refuses new windows in it;
windows already running there go on until they time out, are ended, or their user leaves the circle.
Moving or resizing an area applies to running windows' leave check at once. An area can only be
deleted once no window row references it (ended windows are purged 24 h after they end) — until
then deactivate it.

Ban: sets `banned_at` (kept if already banned), revokes every refresh token, ends the active window
(coordinates wiped, its waves deleted), resolves all open reports against the user as `banned`
(`resolved_by` = the admin), and closes the user's WebSockets with `4403`. A banned user is invisible in
`/nearby` and `/waves/incoming`, missing from other users' `/matches`, and messaging them
(either direction) is `403 forbidden`. Reports against an account that is deleted disappear with it.

## WebSocket `/api/ws`

Client connects, then sends `{ "type": "auth", "token": "<access_token>" }` within 10 s.
Invalid/expired token or timeout → server closes with code `4401` (client refreshes the token before reconnecting).
A password reset or change also closes the account's open sockets with `4401`.
A banned account → `4403`, both at auth time and for open sockets when the ban happens (client logs out,
no reconnect). A server-side failure while checking the account → `1011` (client retries with backoff).
`1012` → the server may have missed events for this socket (its cross-replica listener reconnected); the
client waits its backoff plus a random 0–5 s, reconnects and on `ready` refetches the match list and the
open chat thread. `1001` → the server is shutting down; the client reconnects with its usual backoff.
Server → client events:

```ts
{ type: 'ready' }
{ type: 'message', message: Message }
{ type: 'match', match: MatchSummary }
{ type: 'wave', from_user_id: string }
```

Client reconnects with backoff and re-authenticates with a fresh token.

Liveness: the server sends a WebSocket ping every 25 s and closes a socket that has sent nothing (no
pong, no frame) for 60 s, since an open socket counts as "online" and suppresses push. Browsers answer
pings automatically. The client closes its socket after its page has been hidden for 30 s and
reconnects when it is visible again.

## Push notifications (Web Push)

Optional: without VAPID keys on the server (`GET /push/config` → `enabled: false`) nothing is sent and
subscribing is `409 push_disabled`; preferences still work.

```ts
interface PushConfig { enabled: boolean; public_key: string | null /* VAPID key, base64url: applicationServerKey */ }
interface PushSubscriptionBody {
  endpoint: string                       // PushSubscription.endpoint
  keys: { p256dh: string; auth: string } // base64url, as PushSubscription.toJSON() gives them
  lang?: 'cs' | 'en'                     // notification language for this device, default 'cs'
}
interface PushPrefs { waves: boolean; matches: boolean; messages: boolean }
```

| Method & path | Body | 2xx response |
|---|---|---|
| `GET /push/config` | — | `200 PushConfig` (no auth) |
| `POST /me/push/subscriptions` | `PushSubscriptionBody` | `204` — upsert by `endpoint`; an endpoint registered by another account moves to the caller. At most 10 per user (the oldest beyond that are dropped) |
| `DELETE /me/push/subscriptions` | `{ endpoint }` | `204` (idempotent; only the caller's own) |
| `GET /me/push/prefs` | — | `200 PushPrefs` (all `true` if never saved) |
| `PATCH /me/push/prefs` | `Partial<PushPrefs>` | `200 PushPrefs` |

Validation (`400 validation`): `endpoint` ≤ 1024 chars, `https`, default port, no credentials, and its host
must be a known push service — `fcm.googleapis.com`, `updates.push.services.mozilla.com`,
`web.push.apple.com`, `*.push.apple.com`, `*.notify.windows.com` (an allowlist, so the server never
POSTs to an address a client picked); `p256dh` a P-256 point (65 bytes), `auth` 16 bytes.

A push goes out for the same events as the WebSocket (`wave`, `match`, `message`) — never to the user
who caused it — only when the recipient has that preference on **and no open WebSocket on any replica**.
Messages coalesce: at most one push per match per 60 s per recipient. Subscriptions the push service
reports gone (404/410) are deleted, as are those it rejects (any other 4xx but 429) 3 times in a row.
The client calls `DELETE /me/push/subscriptions` on an explicit logout only. When the session is lost
any other way it keeps its browser subscription, and the next login re-posts it (moving it to that
account). Payload the service worker receives (text is generic on purpose —
no names, no message bodies):

```ts
interface PushPayload {
  type: 'wave' | 'match' | 'message'
  title: string; body: string            // already in the subscription's `lang`
  url: string                            // in-app route: '/nearby' | '/matches/<match_id>'
  tag: string                            // 'wave' | 'match-<match_id>' | 'message-<match_id>' (replaces older ones)
}
```

## Interest keys (seeded by migration, ids 1..40 in this order; FE translates `interest.<key>`)

`hiking`, `running`, `cycling`, `climbing`, `swimming`, `yoga`, `gym`, `football`, `tennis`, `skiing`,
`music`, `concerts`, `festivals`, `dancing`, `singing`, `guitar`, `cinema`, `theatre`, `art`, `photography`,
`reading`, `writing`, `gaming`, `board_games`, `tech`, `science`, `travel`, `languages`, `cooking`, `coffee`,
`wine`, `beer`, `food`, `nature`, `animals`, `volunteering`, `fashion`, `meditation`, `history`, `cars`
