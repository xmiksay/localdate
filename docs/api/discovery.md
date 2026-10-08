# API contract — Discovery

Part of the [API contract](../api.md); conventions, errors and shared types live there.

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
Applies every rule in [architecture.md "Mutual filters"](../architecture.md#core-concepts). Excludes self.
An area window sees only fresh area windows in the same area (`area` set on every item, `max_distance_m`
ignored, `distance_band: null` — deliberately, the shared area is the only place information; these
sort by shared interests, then newest window, then `user_id`); a timed window sees only timed windows
within distance (`area: null`, `distance_band` set).

Band = smallest of 200 / 500 / 1000 / 2000 / 5000 / 10000 m that the real distance is below.
