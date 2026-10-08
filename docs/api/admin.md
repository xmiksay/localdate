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
