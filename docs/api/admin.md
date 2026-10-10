# API contract — Admin (moderation)

Part of the [API contract](../api.md); conventions, errors and shared types live there.

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
    banned_at: string | null; is_admin: boolean; is_test: boolean
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

## Users, test users and impersonation (alpha / beta testing)

```ts
interface AdminUserRow {
  id: string; username: string; display_name: string | null; photo_url: string | null
  gender: Gender | null; age: number | null
  is_test: boolean; is_admin: boolean; banned_at: string | null; created_at: string
}
interface Impersonation {
  access_token: string   // for the target, carries the `act` claim (see auth.md); no refresh token
  expires_at: string     // 60 min after issue
  user: User             // the target
}
type AuditAction = 'impersonate' | 'impersonated_request'
interface AuditEntry {
  id: string; created_at: string; action: AuditAction
  admin: UserRef | null    // null once that admin's account was deleted
  target: UserRef | null   // null once the target was deleted
  meta: { method?: string; route?: string }   // impersonated_request: e.g. { method: 'POST', route: '/api/me/window' },
                                              // { method: 'WS OPEN' | 'WS CLOSE', route: '/api/ws' }
}
```

| Method & path | Body | 2xx response |
|---|---|---|
| `GET /admin/settings` | — | `200 { impersonation: boolean }` — whether `ADMIN_IMPERSONATION` is on |
| `GET /admin/users?q=` | — | `200 AdminUserRow[]`, at most 50 — `q` (trimmed, ≤ 64 chars) matches username or display name, case-insensitive substring; no/empty `q` = newest accounts |
| `GET /admin/test-users` | — | `200 AdminUserRow[]` — every test user, newest first (at most 500) |
| `POST /admin/test-users` | `{ username, display_name, gender: Gender, birth_date, bio?: string, interest_ids: number[], placeholder_photo?: boolean /* default true */ }` | `201 AdminUserRow` |
| `POST /admin/test-users/{id}/photos` | multipart `file` | `201 Photo` — like `POST /me/photos`; `404` unless `{id}` is a test user |
| `DELETE /admin/test-users/{id}` | — | `204` — deletes the account exactly like `DELETE /me` and closes its sockets; `404` unless a test user |
| `POST /admin/users/{id}/impersonate` | — | `200 Impersonation`; `404` when impersonation is off or the user is unknown; `409 cannot_impersonate` for an admin (yourself included) or a banned account |
| `GET /admin/audit` | — | `200 AuditEntry[]` — newest first, at most 200 |

**Test users** (`is_test`) are ordinary accounts — visible to and matched with real users like anyone else —
marked for the admin only. Creation validates the username like register (`400 validation`,
`409 username_taken`) and the profile like `PUT /me/profile` (`400 validation`, `422 underage`, unknown interest
ids `400`). The account has **no password and no identity** (nobody can log in as it; it is driven through
impersonation), gets the profile, a filter (any gender, 18–99, 10 km, both reasons, 60 min) and — with
`placeholder_photo` — a generated placeholder avatar, so it is onboarded and can open a window at once.
`localdate-api admin seed-test-users --count N` (`make seed-test-users COUNT=N`) creates N such users with
random Czech names, mixed genders, ages 18–45, 3–6 interests and a placeholder avatar each.

**Impersonation** ("act as") is off unless `ADMIN_IMPERSONATION=true`. The token is valid for 60 minutes; there
is no refresh, the admin starts again. It is **deny-by-default**: only these endpoints accept it —

`GET /me`, `PUT /me/profile`, `GET|PUT /me/filter`, `POST /me/photos`, `PUT /me/photos/order`,
`DELETE /me/photos/{id}`, `GET|POST|PATCH|DELETE /me/window`, `POST /me/location`, `GET /nearby`, `GET /areas`,
`POST /waves`, `GET /waves/incoming`, `GET /matches`, `GET|POST /matches/{id}/messages`, and the WebSocket.

Every other authenticated endpoint answers `403 impersonation_forbidden` — among them password, identities
(list, email link/confirm, unlink, OAuth link), `DELETE /me`, push subscriptions and preferences, blocks and
reports, and all of `/admin/*`; endpoints added later are refused too unless they opt in
(`backend/api/tests/impersonation_routes.rs` makes every route and every `ActingUser` handler be classified). **Every** request made with an
impersonation token — `GET`s and refused ones included — writes an `impersonated_request` audit row
(`{ method, route }`: the route template, never the body or concrete ids) before it runs; an impersonated
WebSocket writes one with `method: 'WS OPEN'` when it is accepted and one with `'WS CLOSE'` when it ends (route
`/api/ws`). Issuing the token writes `impersonate`.

The token ends at its 60-minute expiry (an open WebSocket is closed with `4401` at its next account re-check), when
`ADMIN_IMPERSONATION` is turned off (+ restart), and when the acting admin is demoted, banned or changes or resets
their password. It does **not** end when the admin logs out — logout revokes refresh tokens only; the admin UI
drops the impersonation itself on logout, also in other open tabs. An impersonated WebSocket counts as the target
being online, which suppresses the target's Web Push like any open socket (accepted for alpha testing).
