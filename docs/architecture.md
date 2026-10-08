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
  (30 / 60 / 120 / 240 min, default from settings). Can be extended or ended early.
  Only a user with an active window can see others (reciprocity), and only users with
  an active window **and ≥ 1 photo and a profile** are visible. `kind` is an enum
  (`timed` only for now) so future modes (until end of day, predefined area) fit in.
- **Location** — stored only on the active window row, rounded to 3 decimals (~100 m).
  The server never returns anyone's coordinates; only a **distance band**.
  Distance = haversine over lat/lon in SQL (bounding-box prefilter + exact check).
  Predefined areas (city centre, train station) are a future replacement.
- **Mutual filters** — A sees B iff all hold, in both directions:
  - distance ≤ min(A.max_distance_m, B.max_distance_m)
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
- **Expired windows** are closed lazily (`ended_at` set, pending waves deleted) when next read.
- **Safety** — block (hides both ways, hides match, forbids messages), report
  (stored for manual moderation, also blocks), account deletion (hard delete of all
  rows and photo files).
- **Age** — birth date mandatory, < 18 rejected at onboarding. Profiles expose age, never the date.

## Data model

| Table | Columns (all timestamps `timestamptz`) |
|---|---|
| `user` | id uuid PK, username text UNIQUE (lowercase, `[a-z0-9_]{3,32}`), password_hash text (argon2id), created_at |
| `refresh_token` | id uuid PK, user_id FK, token_hash text UNIQUE (sha256), family_id uuid, expires_at, revoked_at NULL, created_at |
| `profile` | user_id PK/FK, display_name text (1–40), birth_date date, gender enum(male,female,other), bio text (≤ 500), updated_at |
| `photo` | id uuid PK, user_id FK, file_name text, position smallint (0 = primary), created_at; max 6 per user |
| `interest` | id serial PK, key text UNIQUE (i18n key suffix, seeded ~40) |
| `user_interest` | (user_id, interest_id) PK; max 10 per user |
| `filter` | user_id PK/FK, max_distance_m int (200–10000), genders gender[] (empty = any), age_min smallint, age_max smallint (18–99), reasons reason[] non-empty, default_window_minutes smallint (30/60/120/240) |
| `visibility_window` | id uuid PK, user_id FK, kind enum(timed), lat double, lon double, location_updated_at, starts_at, ends_at, ended_at NULL. Active = `ended_at IS NULL AND ends_at > now()`. Unique partial index on `user_id WHERE ended_at IS NULL` — the app must set `ended_at` when replacing/ending a window |
| `wave` | id uuid PK, from_user_id FK, to_user_id FK, window_id FK, created_at, expires_at (= sender window ends_at) |
| `match` (Rust module `matches`) | id uuid PK, user_a FK, user_b FK (user_a < user_b, UNIQUE pair), created_at |
| `message` | id uuid PK, match_id FK, sender_id FK, body text (1–2000), created_at |
| `block` | (blocker_id, blocked_id) PK, created_at |
| `report` | id uuid PK, reporter_id FK, reported_id FK, reason enum(spam,harassment,fake,underage,other), note text NULL, created_at |

`reason` enum: `date`, `meet`. All user FKs `ON DELETE CASCADE`.

## Auth

Username + password (argon2id), no password reset (needs a linked email/Telegram — follow-up issue).
JWT HS256 access token (15 min, `sub` = user id) + opaque refresh token (30 days, stored hashed,
rotated on every use; reuse of a revoked token revokes the whole family). The frontend keeps both in
`localStorage` (accepted XSS trade-off — strict CSP mitigates). Access tokens are trusted without a DB
lookup, so a deleted account's token stays valid ≤ 15 min (FK cascade makes writes fail safely).
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
