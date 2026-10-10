# Architecture — Data model and storage

Part of the [architecture](../architecture.md) docs.

## Data model

| Table | Columns (all timestamps `timestamptz`) |
|---|---|
| `user` | id uuid PK, username text (free-form display name as typed: trimmed, NFC, 1–64 chars, no control chars; CHECK char_length 1–64), username_key text UNIQUE (`auth::validation::username_key`: NFC + Rust Unicode lowercase, computed by the app so it does not depend on the database locale; every lookup by username compares it. Simple lowercase, not full case folding: `ß`/`SS` and final `ς`/`σ` stay distinct keys. The key is stored, so a Rust Unicode upgrade that changes a lowercase mapping needs a re-key migration (and a collision check)), password_hash text NULL (argon2id; NULL = account created by email, no password), created_at, is_admin bool (default false), banned_at NULL, credentials_changed_at NULL (last password reset/change, whole seconds; older access tokens are refused) |
| `refresh_token` | id uuid PK, user_id FK, token_hash text UNIQUE (sha256), family_id uuid, expires_at, revoked_at NULL, created_at |
| `user_identity` | id uuid PK, user_id FK, provider enum(email, google, telegram, facebook), subject text (email: validated, lowercased address; google: the account's `sub`; telegram: the numeric user id `id`, decimal; facebook: the app-scoped user id), verified_at, created_at; UNIQUE (provider, subject) |
| `oauth_grant` | id uuid PK, token_hash text UNIQUE (sha256), purpose enum(login, signup_code, signup), provider identity_provider, subject text, user_id FK NULL (CHECK: set iff login), binding text NULL (sha256 of the flow state; CHECK: NULL iff signup), expires_at (codes +60 s, signup +15 min), used_at NULL, created_at, photo bytea NULL (imported profile picture, WebP ≤ 8 MiB; CHECK: sign-up purposes only) |
| `email_token` | id uuid PK, token_hash text UNIQUE (sha256), purpose enum(login,link,signup,password_reset), user_id FK NULL (CHECK: NULL iff signup), provider identity_provider (default email; telegram only for reset links sent by the bot), email text (the identity subject of `provider`: an address, or a Telegram user id), expires_at (+15 min), used_at NULL, created_at |
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
| `ws_replica` | replica_id uuid PK, seen_at (heartbeat, DB `now()`) |
| `ws_presence` | (replica_id FK → ws_replica ON DELETE CASCADE, socket_id bigint) PK, user_id FK, connected_at; one row per open WebSocket; index on user_id |
| `push_subscription` | id uuid PK, user_id FK, endpoint text UNIQUE, p256dh text, auth text (base64url), user_agent text NULL, lang text (`cs`/`en`, CHECK), created_at (= last registration), last_success_at NULL, failure_count int (consecutive rejections, default 0); index on user_id |
| `push_prefs` | user_id PK/FK, waves bool, matches bool, messages bool (all default true; no row = all true) |
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
| `email_token` | deleted once expired, used or not (drops the address it held) |
| `oauth_grant` | deleted once expired, used or not |
| `match`, `message` | kept until account deletion |
| `ws_replica` (+ its `ws_presence` rows by cascade) | deleted once `seen_at` is ≥ 90 s old (replica died without deregistering) |

## Photos

The client (`frontend/src/utils/imageResize.ts`) uploads a JPEG/PNG/WebP within 2048 px long edge and 10 MiB
unchanged; anything else the browser can decode (bigger shots, HEIC on Safari, AVIF, …) is re-encoded as a 2048 px
JPEG (q 0.9, transparency flattened onto white). Files it cannot decode keep their type, so the client's type check
rejects non-JPEG/PNG/WebP ones before upload.
Uploaded via multipart to the API, decoded with `image`, resized to max 1280 px long edge,
re-encoded as lossless WebP via the pure-Rust `image` encoder (strips EXIF incl. GPS, honours orientation; no libwebp
C dependency), stored as `<uuid>.webp` through `media::PhotoStore` (see Storage below). Inputs over 10 000 px on an edge or 32 Mi pixels are
refused from the header, before decoding (decode allocation capped at 128 MiB), and at most two decodes run
at once (`AppState::image_permits`), which bounds memory for the pod limit.
Served publicly at `/media/<uuid>.webp` — filenames are unguessable v4 UUIDs, so `<img>` works
without auth headers. A Facebook profile picture can also be imported at login
([Auth](auth.md) → "Profile picture import"); it runs through the same `to_webp` / `add` path as an upload.

**Storage.** `media::PhotoStore` (`object_store` crate) is the only code that writes, reads or deletes photo
files; `PHOTO_STORAGE` picks the backend ([Config](config.md)):

- `disk` — `PHOTO_DIR/<uuid>.webp`; each write goes to a staging file that is fsynced and renamed into place.
- `s3` — object `<uuid>.webp` at the bucket root of an S3-compatible store (Garage), path-style, SigV4 signed
  with `ring` (no aws-lc). Connect timeout 5 s, 30 s per read (no total timeout, so a slow but progressing
  `/media` stream is not cut), 2 retries within 15 s.

Write order: object first, then the row (under the user lock, with the authoritative 6-photo check); if the
insert fails the object is deleted again, so a row never points at a missing file. A failed put → `500 internal`,
no row. Deleting a photo, and `DELETE /me` (names are read before the cascade), delete the objects after the
commit, best effort, as one bulk delete (`delete_stream`; one DeleteObjects request on S3) bounded by a 20 s
overall timeout: a failure never fails the request, missing objects count as deleted, and the names left behind
are logged in one `photo files left behind in storage` warning so they can be removed by hand. Reordering touches rows only.
There is no orphan sweep job.

`GET`/`HEAD /media/{name}` (`media/mod.rs`) accepts only the generated name shape — canonical lowercase
hyphenated UUID + `.webp` — so no request can reach a path outside the photo set or any other object in the
bucket; anything else, and a missing object, is `404`. The body is streamed from the store with
`Content-Type: image/webp`, `Content-Length`, `X-Content-Type-Options: nosniff`, the backend's `ETag` and
`Cache-Control: private, max-age=31536000, immutable`: a name is never reused (a new upload gets a new UUID), so a
cached copy cannot go stale; `private` keeps photos out of shared caches (CDN, proxies), so after a deletion only
browsers that already fetched the photo can still show it. `If-None-Match` follows RFC 9110 (`*`, lists, several
header lines, weak comparison) and answers `304` with `ETag` + `Cache-Control`. HEAD and a conditional GET are
decided from the object's metadata (`HEAD` on the store), so neither a HEAD nor a 304 downloads the object. Unauthenticated, as
before.
