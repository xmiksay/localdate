# Architecture

localdate — meet people nearby *right now* (missed bus, waiting for a train, new city, concert/festival).
Not swipe-based: a user who opens a **visibility window** sees every mutually-matching profile around them.

## Sections

- [Data model and storage](architecture/data-model.md) — tables, cleanup job and retention, photos
- [Auth](architecture/auth.md) — tokens, login providers, identities, password reset
- [Realtime and push](architecture/realtime-push.md) — WebSocket hub, LISTEN/NOTIFY, Web Push
- [Configuration and serving](architecture/config.md) — env table, serving the PWA

## Layout

```
backend/            Cargo workspace
  api/              Axum HTTP + WebSocket server (binary `localdate-api`)
  entity/           SeaORM entities
  migration/        sea-orm-migration (append-only)
frontend/           Vue 3 + TS + Vite + Tailwind + Pinia + vue-i18n PWA
docs/               architecture.md + architecture/ (this), api.md + api/ (HTTP/WS contract), deployment.md
deploy/             k8s.yml (namespace, Postgres, API, ingress), secrets.example.yml
Dockerfile          node → rust → debian-slim; image ghcr.io/xmiksay/localdate (built and pushed by .github/workflows/ci.yml after the gate)
Makefile            single entry point for build / lint / test / run / image / deploy
```

Deployment target is Kubernetes: one API replica (photos live on a `ReadWriteOnce` volume; WebSocket
push already works across replicas, see [Realtime](architecture/realtime-push.md#realtime)) plus a Postgres StatefulSet in namespace
`localdate`, behind ingress-nginx at `localdate.mmik.cz` — see [deployment.md](deployment.md).
Local dev uses the host Postgres; there is no docker-compose.

The release `localdate-api` binary is the whole deployable: `make build` builds `frontend/dist`
first and `rust-embed` compiles it into the binary. `backend/api/build.rs` makes cargo rebuild the
crate whenever `frontend/dist` changes and refuses a release build without `frontend/dist/index.html`.
Debug builds (clippy, tests) need no bundle: they read `frontend/dist` from disk at request time.

## Core concepts

- **Visibility window** — the user turns on "I'm available" for a preset duration
  (30 / 60 / 120 / 240 min, default from settings) or until the end of their local day
  (`discovery::duration`: next midnight in the client's IANA `tz` via `chrono-tz`, refused < 30 min before it).
  Can be extended or ended early. No window ever runs more than 12 h ahead of now (start or extend), so an
  end-of-day window started in the morning ends after 12 h. End of day is only how `ends_at` is computed —
  nothing about it is stored, it is not a `kind`. The invariant is "≤ 12 h ahead", not "one of the
  presets": any client can pick a `tz` whose midnight is e.g. 47 min away and so get a non-preset length,
  which is accepted on purpose (`ends_at` is computed after the start's user-row lock).
  Only a user with an active window can see others (reciprocity), and only users with
  an active window **and ≥ 1 photo and a profile** are visible. `kind` is `timed` (distance
  around the user) or `area` (bound to a predefined area, below); either can run until end of day.
- **Areas** — admin-managed circles (centre + `radius_m` 50–5000) around public places: city centre,
  train station, venue. Plain Postgres + Rust haversine (`discovery::geo::Circle`), no PostGIS; the table is
  small, so `GET /areas` filters active areas in Rust. An area window starts only inside the circle
  (`409 outside_area`) and ends itself (`window::end`, coordinates wiped, waves deleted) when a location
  update lands more than radius + margin off the centre, margin = max(100 m, reported fix accuracy capped at
  500 m) (`geo::exit_margin`, GPS-jitter hysteresis) —
  that update answers `409 left_area` and the client shows a "you left the area" notice. Containment uses
  the coordinates as sent, before rounding. Admins never delete an area that window rows still reference
  (the FK has no delete action → `409 area_in_use`); they deactivate it instead, which hides it from
  `/areas` and refuses new windows, while running windows go on until they end (≤ 12 h) or their user leaves.
  Moving/resizing an area applies to running windows at their next location update.
- **Location** — stored only on the active window row, rounded to 3 decimals (~100 m),
  and wiped (`lat`/`lon` set to NULL) as soon as the window ends — immediately on an explicit
  end/replace/lazy close, by the cleanup job (which closes the window) for windows that ran out.
  A CHECK enforces that an open window (`ended_at IS NULL`) always has coordinates. A location update on a
  timed window is one conditional write on a still-active window; on an area window it locks the row
  (`FOR UPDATE`), runs the leave check and then writes or ends the window. Extend locks the row too.
  The server never returns anyone's coordinates; only a **distance band**.
  Distance = haversine over lat/lon in SQL (bounding-box prefilter + exact check).
  Area windows are matched by area instead (below).
- **Mutual filters** — A sees B iff all hold, in both directions:
  - both windows are timed and distance ≤ min(A.max_distance_m, B.max_distance_m), **or** both are area
    windows in the same area (distance and max distance then ignored; timed and area windows never meet).
    Area matches carry `area { id, name }` and `distance_band: null` (deliberate: no distance hint inside an
    area); they sort by shared interests, newest window, `user_id`. An area window whose
    `location_updated_at` is older than `AREA_STALE_SECS` (10 min, `discovery::rules`) is stale: invisible to
    others and sees nobody, so a phone that stopped reporting cannot linger in an area
  - A.genders empty ("doesn't matter") or contains B.gender, and vice versa
  - B.age ∈ [A.age_min, A.age_max] and vice versa
  - A.reasons ∩ B.reasons ≠ ∅ (one shared reason is enough)
  - neither has blocked the other
- **Wave** — A waves at a currently visible B. Valid until A's window ends.
  Max 20 waves per window. When B waves back (or A waves at B who already waved),
  a **match** is created and a chat opens. Chat persists after windows end.
  `waves_left` counts *pending* waves of the window, so waves consumed by a match free a slot.
  The visibility rule lives in one SQL query (`discovery/nearby.rs`) used by `/nearby`, `POST /waves`
  and `/waves/incoming`; `discovery/rules.rs::mutually_visible` is the readable spec, and an
  integration test asserts both agree.
- **Interest overlap** — each nearby profile carries `shared_interests` (viewer ∩ them). It only
  ranks and highlights, never gates visibility: lists sort by shared count desc, then distance band,
  newest window, `user_id` (`nearby::sort_for_display`, in Rust after the single visibility query;
  the viewer's interests ride along in the batched `user_interest` lookup).
- **Expired windows** are closed lazily (`ended_at` set, pending waves deleted) when next read;
  the [cleanup job](architecture/data-model.md#cleanup-job-and-retention) closes them the same way, then deletes them a day later.
- **Safety** — block (hides both ways, hides match, forbids messages), report
  (also blocks; lands in the moderation queue, below), account deletion (hard delete of all
  rows and photo files).
- **Moderation** — admins (`user.is_admin`, set only with `localdate-api admin grant|revoke <username>`)
  work the report queue at `/admin` (`admin/` module, [docs/api/admin.md](api/admin.md)): dismiss a report, or
  **soft-ban** the subject. A ban (one transaction, `admin::ban`) first locks the user row `FOR UPDATE`, then
  sets `banned_at`, revokes every refresh token, ends the active window (`window::end`, coordinates wiped,
  its waves deleted) and resolves all open reports on the user as `banned`; after commit
  `Hub::disconnect(user, CloseReason::Banned)` closes their sockets (`4403`). Transactions that create what
  only an unbanned user may own — `start_window`, `post_wave`, `refresh::rotate` — read the user row
  `FOR SHARE` (`auth::extractor::lock_unbanned`): they either wait for a running ban and then answer
  `403 banned`, or finish first and the ban cleans up after them. Login (after a correct password),
  refresh and every authenticated request answer `403 banned`; the nearby SQL joins `user.banned_at IS NULL`
  as a second line of defence, and `safety::is_blocked_between` / `blocked_with` treat banned accounts like
  blocked ones, so `/matches` drops them and messaging them is 403. Unban clears `banned_at` only. Admins
  cannot be banned (`409 cannot_ban_admin`) — revoke the role first. A banned user cannot delete their own
  account (every request is 403); that is an operator task for now.
  The `admin grant|revoke` CLI never migrates: with pending migrations it exits non-zero and asks for
  `make migrate` (or a server start) first.
- **Age** — birth date mandatory, < 18 rejected at onboarding. Profiles expose age, never the date.
