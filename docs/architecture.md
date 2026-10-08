# Architecture

localdate — meet people nearby *right now* (missed bus, waiting for a train, new city, concert/festival).
Not swipe-based: a user who opens a **visibility window** sees every mutually-matching profile around them.

## Layout

```
backend/            Cargo workspace
  api/              Axum HTTP + WebSocket server (binary `localdate-api`)
  entity/           SeaORM entities
  migration/        sea-orm-migration (append-only)
frontend/           Vue 3 + TS + Vite + Tailwind + Pinia + vue-i18n PWA
docs/               architecture.md (this), api.md (HTTP/WS contract)
Makefile            single entry point for build / lint / test / run
```

Deployment target is Kubernetes (manifests are a follow-up issue). Local dev uses the
host Postgres; there is no docker-compose.

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
  the cleanup job (below) closes them the same way, then deletes them a day later.
- **Safety** — block (hides both ways, hides match, forbids messages), report
  (also blocks; lands in the moderation queue, below), account deletion (hard delete of all
  rows and photo files).
- **Moderation** — admins (`user.is_admin`, set only with `localdate-api admin grant|revoke <username>`)
  work the report queue at `/admin` (`admin/` module, docs/api.md "Admin"): dismiss a report, or
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

## Data model

| Table | Columns (all timestamps `timestamptz`) |
|---|---|
| `user` | id uuid PK, username text UNIQUE (lowercase, `[a-z0-9_]{3,32}`), password_hash text (argon2id), created_at, is_admin bool (default false), banned_at NULL |
| `refresh_token` | id uuid PK, user_id FK, token_hash text UNIQUE (sha256), family_id uuid, expires_at, revoked_at NULL, created_at |
| `profile` | user_id PK/FK, display_name text (1–40), birth_date date, gender enum(male,female,other), bio text (≤ 500), updated_at |
| `photo` | id uuid PK, user_id FK, file_name text, position smallint (0 = primary), created_at; max 6 per user |
| `interest` | id serial PK, key text UNIQUE (i18n key suffix, seeded ~40) |
| `user_interest` | (user_id, interest_id) PK; max 10 per user |
| `filter` | user_id PK/FK, max_distance_m int (200–10000), genders gender[] (empty = any), age_min smallint, age_max smallint (18–99), reasons reason[] non-empty, default_window_minutes smallint (30/60/120/240) |
| `area` | id uuid PK, name text (1–80), kind enum(city_centre,train_station,venue,other), lat double, lon double (centre, not rounded), radius_m int (50–5000), active bool (default true), created_at |
| `visibility_window` | id uuid PK, user_id FK, kind enum(timed,area), area_id FK NULL (no delete action; CHECK set iff kind = area), lat double NULL, lon double NULL (NULL once ended; CHECK open ⇒ set), location_updated_at, starts_at, ends_at, ended_at NULL. Active = `ended_at IS NULL AND ends_at > now()`. Unique partial index on `user_id WHERE ended_at IS NULL` — the app must set `ended_at` when replacing/ending a window |
| `wave` | id uuid PK, from_user_id FK, to_user_id FK, window_id FK, created_at, expires_at (= sender window ends_at) |
| `match` (Rust module `matches`) | id uuid PK, user_a FK, user_b FK (user_a < user_b, UNIQUE pair), created_at |
| `message` | id uuid PK, match_id FK, sender_id FK, body text (1–2000), created_at |
| `block` | (blocker_id, blocked_id) PK, created_at |
| `report` | id uuid PK, reporter_id FK NULL (**ON DELETE SET NULL** — evidence outlives the reporter's account), reported_id FK, reason enum(spam,harassment,fake,underage,other), note text NULL, created_at, resolved_at NULL, resolved_by FK NULL (ON DELETE SET NULL), resolution enum(dismissed,banned) NULL (CHECK: set together with resolved_at); partial index on reported_id of open reports |

`reason` enum: `date`, `meet`. All user FKs `ON DELETE CASCADE` except the two `report` ones marked above;
reports *about* a deleted account cascade away with it. `wave.window_id` cascades too;
`match`/`message` have no FK to windows, so deleting windows never touches chats.

## Cleanup job and retention

`cleanup.rs`: `main` spawns `run_forever`, which runs `run_once(db, shift)` every
`CLEANUP_INTERVAL_SECS`, each tick in its own task so an error or panic is logged and the next tick
retries. The test router does not start it; tests call `run_once` with a time `shift`. A tick is one
transaction guarded by `pg_try_advisory_xact_lock(cleanup::LOCK_KEY)`, so with several replicas
only one does the work and the others skip that tick. "now" is the database's `now()` (as in the
nearby query), plus `shift`.

| Data | Rule |
|---|---|
| open `visibility_window` with `ends_at <= now` | closed: `ended_at = ends_at`, coordinates NULLed |
| ended `visibility_window` still holding coordinates | coordinates NULLed |
| `wave` | deleted once `expires_at <= now` or its window has ended (all remaining waves are unanswered — a match deletes its waves) |
| `visibility_window` row | deleted 24 h after it ended (`LEAST(ends_at, ended_at)`) |
| `refresh_token` | deleted once expired, or once revoked > 7 days ago **and** its family has no live token — while a family is live, replaying any of its revoked tokens still revokes it |
| `match`, `message` | kept until account deletion |

## Auth

Username + password (argon2id), no password reset (needs a linked email/Telegram — follow-up issue).
JWT HS256 access token (15 min, `sub` = user id) + opaque refresh token (30 days, stored hashed,
rotated on every use; reuse of a revoked token revokes the whole family). The frontend keeps both in
`localStorage` (accepted XSS trade-off — strict CSP mitigates). Every authenticated request (and WS auth)
also loads the account by primary key (`auth::extractor::verify_access`): a deleted account gets 401 and a
banned one `403 banned` at once, instead of the token living out its 15 minutes.
Login/register rate-limited per peer IP (in-memory token bucket) — behind an ingress this needs trusted
`X-Forwarded-For` handling (see the k8s issue).

## Photos

Uploaded via multipart to the API, decoded with `image`, resized to max 1280 px long edge,
re-encoded as lossless WebP via the pure-Rust `image` encoder (strips EXIF incl. GPS, honours orientation; no libwebp
C dependency), written to `PHOTO_DIR/<uuid>.webp`.
Served publicly at `/media/<uuid>.webp` — filenames are unguessable v4 UUIDs, so `<img>` works
without auth headers. S3 storage is a follow-up issue.

## Realtime

`/api/ws` WebSocket, server → client push only (messages are sent over REST).
In-process broadcast hub keyed by user id — single API replica. Multi-replica fan-out
(Postgres LISTEN/NOTIFY) is a follow-up issue.

## Configuration (env)

| Var | Example | Notes |
|---|---|---|
| `DATABASE_URL` | `postgres://localdate:localdate@localhost/localdate` | |
| `JWT_SECRET` | 32+ random bytes | required |
| `PHOTO_DIR` | `./data/photos` | created on start |
| `BIND_ADDR` | `127.0.0.1:3000` | |
| `CLEANUP_INTERVAL_SECS` | `300` | optional, default 300; period of the cleanup job |
| `RUST_LOG` | `info,sqlx=warn,localdate_api=debug` | sqlx logs every query at info |

Frontend dev server (Vite, :5173) proxies `/api` and `/media` (incl. WS) to `BIND_ADDR`.

## Serving the PWA

`web.rs` is the router fallback, so `/api/*` (which has its own JSON `not_found` fallback) and
`/media/*` always win. For other GET/HEAD requests the path is percent-decoded and refused with
404 if it contains `.`/`..`/empty segments, `\` or NUL (debug builds read from disk). Then:

- embedded file → served with its MIME type and an ETag (`If-None-Match` → 304);
  `assets/*` (Vite content-hashed) get `Cache-Control: public, max-age=31536000, immutable`,
  everything else (`index.html`, `sw.js`, `workbox-*.js`, `registerSW.js`, manifest, icons) `no-cache`;
- no such file, last path segment has an extension → 404;
- otherwise (client-side route such as `/nearby`) → `index.html`, `no-cache`.

Other methods → 405. A debug binary without `frontend/dist` (`#[allow_missing]`, e.g. CI) answers
404 to every path outside `/api` and `/media`, and logs a warning at startup.

The frontend reloads once when a lazy chunk fails to load (`vite:preloadError`), which is what a
tab opened before a deploy hits; a 10 s `sessionStorage` guard prevents reload loops.
