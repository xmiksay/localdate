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
docs/               architecture.md (this), api.md (HTTP/WS contract), deployment.md
deploy/             k8s.yml (namespace, Postgres, API, ingress), secrets.example.yml
Dockerfile          node → rust → debian-slim; image ghcr.io/xmiksay/localdate (built and pushed by .github/workflows/ci.yml after the gate)
Makefile            single entry point for build / lint / test / run / image / deploy
```

Deployment target is Kubernetes: one API replica (photos live on a `ReadWriteOnce` volume; WebSocket
push already works across replicas, see Realtime) plus a Postgres StatefulSet in namespace
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
| `user` | id uuid PK, username text UNIQUE (lowercase, `[a-z0-9_]{3,32}`), password_hash text NULL (argon2id; NULL = account created by email, no password), created_at, is_admin bool (default false), banned_at NULL, credentials_changed_at NULL (last password reset/change, whole seconds; older access tokens are refused) |
| `refresh_token` | id uuid PK, user_id FK, token_hash text UNIQUE (sha256), family_id uuid, expires_at, revoked_at NULL, created_at |
| `user_identity` | id uuid PK, user_id FK, provider enum(email, google; later telegram/facebook), subject text (email: validated, lowercased address; google: the account's `sub`), verified_at, created_at; UNIQUE (provider, subject) |
| `oauth_grant` | id uuid PK, token_hash text UNIQUE (sha256), purpose enum(login, signup_code, signup), provider identity_provider, subject text, user_id FK NULL (CHECK: set iff login), binding text NULL (sha256 of the flow state; CHECK: NULL iff signup), expires_at (codes +60 s, signup +15 min), used_at NULL, created_at |
| `email_token` | id uuid PK, token_hash text UNIQUE (sha256), purpose enum(login,link,signup,password_reset), user_id FK NULL (CHECK: NULL iff signup), email text, expires_at (+15 min), used_at NULL, created_at |
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

## Auth

Username + password (argon2id), an **email magic link**, or **Sign in with Google**; a forgotten password is
reset through a linked email.

**Identity model.** A *login method* is the password (`user.password_hash`, optional) or a row in
`user_identity` — `(provider, subject)` unique, so one address belongs to at most one account. Email and Google
are the providers now; Telegram/Facebook (#14, #17) add an enum value and their own verify step,
everything else (listing, unlinking, "last login method" guard, `session()` issuing Tokens) is shared.
An account always keeps at least one method: `DELETE /me/identities/{id}` locks the user row
`FOR UPDATE` and refuses (`409 last_login_method`) to remove the last identity of a passwordless account.
Password login on a passwordless account verifies against the dummy hash, so it fails like a wrong password
with the same timing.

**Magic link** (`auth/email/`). Addresses are validated strictly (`token::normalize_email`: plain
`local@domain`, restricted charset, no display names/quotes/commas) into a `lettre::Address`; that value is what is
stored and mailed to, so one inbox cannot register under many spellings and user input is never re-parsed as a
mailbox. `start` always answers `202`: an identity for the address gets a `login` token, anything else a `signup`
token — the mail differs, the response does not. Mails are rendered, then sent in a spawned task (at most 4 in
flight, `EmailService`; failures logged), so neither timing nor errors leak. Tokens are 32 random bytes (shared
generator with refresh tokens), stored as sha256, 15 min, single use, and travel in the URL **fragment**
(`/auth/email#token=…`), which never reaches the server or ingress logs. Nothing is spent by opening a link:
the page calls `preview` (purpose, target username, address) and the user presses an explicit button, so mail
scanners and prefetchers cannot burn tokens. No browser binding on purpose — request on a laptop, click on the
phone must work. Consumption is one conditional `UPDATE … WHERE used_at IS NULL AND expires_at > now() RETURNING`;
`verify` (login only) consumes, re-checks that the identity still links that address to that user, takes the
user row `FOR SHARE` (`lock_unbanned`) and creates the session in one transaction, so a ban or error leaves the
token unspent. `signup` consumes inside the user-insert transaction (a taken username rolls it back); a signup
token whose address has meanwhile become an account is `invalid_token` already at `preview`. Linking
(`me/identities.rs`) mails a `link` token bound to the requesting user; confirming filters on that user, so a
stranger's attempt neither works nor burns it. Linking an address that is already anyone's mails a notice
instead of a link. Login and link mails name the target account. Limits (`auth/email/limit.rs`, in memory per
replica; over them still `202`, nothing sent): 3 mails per (address, client IP) per 15 min — a flood from
elsewhere cannot lock the owner out — and 10 per address per hour overall against mail-bombing; on top of the
per-IP limiter on `start`, `signup`, `/auth/password/forgot`, `POST /me/identities/email` and `PUT /me/password`. Mail goes through the `mail::Mailer` trait
(lettre SMTP; `EMAIL_DEV_LOG` logs instead; `MemoryMailer` in tests); `AppState::email` is `None` when neither
is configured, which makes the endpoints `503 email_disabled` and `GET /auth/providers` report `email: false`.
Tokens are never logged outside dev-log mode.

**Password reset** (`auth/reset.rs`). `POST /auth/password/forgot` takes a username or an address (an `@` decides),
charges `ResetLimiter` (`auth/email/limit.rs`; a budget separate from the magic-link `EmailLimiter`, so neither
drains the other: 3 per (input as typed, IP) / 15 min up front, 5 mails per account / hour once the account is
known) and answers `202` at once; the account lookup and the sending run afterwards in `EmailService::detach`,
so neither existence nor a linked email shows in the timing (`idle()` also waits for detached work, which is what
tests poll). The link goes to every linked address (`linked_addresses`); Telegram (#14) adds its own delivery when
it exists. Tokens are `email_token` rows with purpose `password_reset`, the same 15 min / single use / fragment
link (`/auth/password/reset#token=…`) / non-consuming `preview` as the magic link, and `email::target` re-checks
that the address is still linked (unlinking kills pending reset links). `reset` validates the password and peeks
the token before spending argon2 time, then in one transaction consumes it, locks the user row `FOR UPDATE`
(`extractor::lock_user`, the same account mapping as the per-request check: banned → `403`, token unspent) and
runs `replace_password`: write the hash and `credentials_changed_at` (now, truncated to whole seconds), revoke
every refresh token, delete every `push_subscription` of the user (log out everywhere includes a stolen device's
notifications; after `PUT /me/password` the auth store calls `push.resync` so the caller's device re-subscribes), mark every unused `email_token` of the user
used (a link mailed before must not undo the new password). After the commit `hub.disconnect(user,
Unauthorized)` closes the account's sockets on every replica with `4401`. `PUT /me/password` (`me/password.rs`)
verifies `current_password` outside the transaction (argon2 is slow), then under the same lock requires the
stored hash to be unchanged since — a concurrent change is `401 invalid_credentials` — and reuses
`replace_password` and the disconnect. Because the access JWT carries no refresh family, "every session but this
one" cannot be told apart: it revokes all and returns a fresh session; the caller's socket closes too and
reconnects with the returned access token. Older **access tokens** die at once: `verify_access` (HTTP, WS auth)
and the WS periodic re-check compare the JWT `iat` with `credentials_changed_at` (`extractor::superseded`:
refused iff `iat < changed_at`, both in whole seconds) and answer `401 unauthorized` / `4401`. The boundary is
deliberate — a token from the change's own second is accepted, so the fresh session issued right after the change
works without waiting; the price is that a token minted earlier in that same second survives. The client retries a
`401` without refreshing when its stored tokens changed while the request was in flight, and a refresh refused
after such a swap counts as done, so a password change does not log the caller out through a race. **Known trade-off:** a passwordless account can set its first password with nothing but a valid access
token — there is no old password to ask for, and that session could already link an address.

**OAuth / OpenID Connect** (`auth/oauth/`). Providers are generic: `config.rs` holds one `OidcConfig` per
provider (endpoints, accepted issuers, scope, `SubjectSource`, JWKS cache time; presets read from env in
`config::from_env`), `OAuthService` keeps a registry `Provider → Oidc` of the configured ones (absent =
`provider_disabled`, `GET /auth/providers` → `false`), and every redirect URI is
`{APP_BASE_URL}/api/auth/oauth/{provider}/callback`. A new provider (Telegram next, Facebook #17) is a `Provider`
variant + `identity_provider` enum value + preset; `SubjectSource::IdTokenClaim(name)` reads the account id from
the verified ID token (a string as is, an integer in decimal — Telegram's `id`), and a userinfo-style resolver
for providers without a usable ID token slots in as another variant. Google: scope `openid`, subject `sub`.

Server-side authorization code flow: `start` (login; a plain browser navigation) or `link` (a
bearer-authenticated `POST` that returns the provider URL) sets the `ld_oauth` cookie — HttpOnly, SameSite=Lax,
`Path=/api/auth/oauth`, 10 min, `Secure` when `APP_BASE_URL` is https — holding provider, mode, `state`, PKCE
verifier, `nonce`, the in-app redirect and (link) the user id, the starting access token's `iat` and the mail
language, HMAC-SHA256-signed with a key derived from `JWT_SECRET` (`cookie.rs`). The callback checks the cookie
signature and expiry, compares `state` in constant time, exchanges the code with the verifier (`oidc.rs`,
reqwest, no redirects, 10 s timeout) and validates the ID token with `jsonwebtoken`: RS256 only, key by `kid`
from the provider's JWKS, `iss`, `aud` = client id, `exp`, `nonce`, and `azp` = client id when present. JWKS
cache: keys are used for the provider's cache time (Google 1 h); an unknown `kid` refetches at most once a
minute; refetches are single-flight (one download for a burst of callbacks); a failed refetch keeps using the
stale set (logged at warn) rather than locking everyone out. Only the account id is used — the email is neither
requested nor stored, so a changed or unverified address can never take over an account.

Link mode attaches the identity in the callback (user row `FOR SHARE`; someone else's → `identity_taken`, own →
no-op; a password reset/change committed after the starting token's `iat` → `unauthorized`, like that token)
and then mails a "new sign-in method linked" notice to every linked email address (`notice.rs`, detached, no
token). Login mode never puts tokens in a URL: it stores a one-time code (`oauth_grant`, 60 s, sha256,
`binding` = sha256 of the flow `state`) and redirects to `/auth/oauth/done#code=…`; the SPA's `exchange` must
present the same cookie, which defeats login CSRF via a planted code, and gets a session (consume + ban check +
identity re-check + session in one transaction, so a ban since the callback reads `banned`) or, for an unknown
account id, a 15-minute sign-up token for the username step (`signup`, same transaction pattern as email
sign-up). All callback errors redirect to `/auth/oauth/done#error=<code>`. Rate limiting counts flows at
`start` (inside the handler, so a refusal still lands on the done page) and `link`/`signup` (`limit_by_ip`);
the callback is never limited, as refusing it would waste the provider's single-use code. Logout also clears
the flow cookie. Tests run the whole flow against a local fake provider (`tests/common/oauth.rs`, fixture RSA
key + JWKS, JWKS hit counter / outage switch).

JWT HS256 access token (15 min, `sub` = user id) + opaque refresh token (30 days, stored hashed,
rotated on every use; reuse of a revoked token revokes the whole family). The frontend keeps both in
`localStorage` (accepted XSS trade-off — strict CSP mitigates). Every authenticated request (and WS auth)
also loads the account by primary key (`auth::extractor::verify_access`): a deleted account (or a token older
than the last password change) gets 401 and a banned one `403 banned` at once, instead of the token living out
its 15 minutes.
Login/register rate-limited per client IP (in-memory token bucket, `rate_limit.rs`). The client IP is
the peer address, or — with `TRUST_PROXY_HEADERS=true`, as in k8s — the first `X-Forwarded-For` entry
(falling back to the peer when absent or unparsable); IPv6 is bucketed per /64. That is only sound because ingress-nginx, with its
default `use-forwarded-headers`/`compute-full-forwarded-for` off, *overwrites* the header with the
address it saw; a proxy that appends would let clients pick their bucket. Without the flag the header
is ignored, so a directly exposed server cannot be bypassed by spoofing it.

## Photos

Uploaded via multipart to the API, decoded with `image`, resized to max 1280 px long edge,
re-encoded as lossless WebP via the pure-Rust `image` encoder (strips EXIF incl. GPS, honours orientation; no libwebp
C dependency), written to `PHOTO_DIR/<uuid>.webp`. Inputs over 10 000 px on an edge or 32 Mi pixels are
refused from the header, before decoding (decode allocation capped at 128 MiB), and at most two decodes run
at once (`AppState::image_permits`), which bounds memory for the pod limit.
Served publicly at `/media/<uuid>.webp` — filenames are unguessable v4 UUIDs, so `<img>` works
without auth headers. S3 storage is a follow-up issue.

## Realtime

`/api/ws` WebSocket, server → client push only (messages are sent over REST). Works with any number
of API replicas; no config beyond `DATABASE_URL`.

- **Local delivery** — `ws::hub::LocalHub`: per-replica map user id → open sockets (several tabs each).
- **Bridge** — `ws::Hub` (`ws/bridge.rs`) wraps it. Every replica picks a random `replica_id` at start.
  `Hub::send(user, event)` / `Hub::disconnect(user, reason)` deliver to local sockets at once and queue
  the same operation for one publisher task (`ws/publisher.rs`), which runs
  `SELECT pg_notify('localdate_ws', <json>)` on the normal pool (one task, so other replicas see this
  replica's operations in order). A `PgListener` on **its own connection** (opened from `DATABASE_URL`,
  outside the app pool, so it never takes a request slot) receives every notification and hands it to the
  local hub, **skipping its own `replica_id`**. Local-first was chosen over "deliver only via NOTIFY":
  same-replica pushes skip a database round trip and keep working while the listener is down; the cost
  is just the origin check.
- **Publish queue** — bounded (10 000 ops). When full, new events are dropped with a warning; an event
  still queued after 30 s is dropped when its turn comes (logged as a count) — its recipient has most likely
  reconnected and refetched by then. A failed NOTIFY of an event is logged and lost for other replicas.
  **Close ops are never dropped**: they wait for room, and a failed NOTIFY is retried with backoff for up to
  5 minutes (then logged as an error). As a second line of defence every session re-reads its account every
  60 s (`Config::ws_account_recheck`) and closes with `4403` (banned) or `4401` (deleted), so a ban whose
  Close op got lost still ends the socket within a minute.
- **Wire format** (`ws/envelope.rs`): `{origin, op}` with `op` = `event` (user + `ServerEvent`), `close`
  (user + reason), `ping`, or a reference. Postgres refuses payloads ≥ 8000 bytes and a message body may be
  2000 four-byte characters, so a payload over 7500 bytes is replaced by `message_ref {user, id}` /
  `match_ref {user, id}` and the receiving replica loads the row (match summaries rebuilt for that user) —
  only if that user has a socket there, in a task of its own so the listener keeps going; a failed load
  is logged.
- **Listener failures** — the listener reconnects with backoff (1, 2, 4, 8, then 10 s; `retry.rs`).
  Every 30 s each replica publishes a `ping` on the shared channel (own ticker, independent of the
  presence SQL). Every listener receives every ping, including its own replica's, so 75 s without any
  notification means the connection is dead (catches a silently dropped TCP link). While disconnected,
  local delivery still works but events from other replicas are lost, so after reconnecting the replica
  closes all its local sockets with `1012` (`CloseReason::Resync`). Clients wait their backoff plus a random
  0–5 s (so one replica's sockets do not all return at once), reconnect, and on `ready` refetch the match
  list and the open chat thread (`matches.resync()`, merged by message id).
- **Presence** — `Hub::is_online(user)` is true when a `ws_presence` row of the user belongs to a
  `ws_replica` seen less than 90 s ago (`ws::REPLICA_STALE`). A row is inserted when a socket subscribes
  (before `ready`) and deleted when its session ends. Every 30 s (`presence::HEARTBEAT`, each beat capped
  at 10 s) the replica upserts its `ws_replica.seen_at` and reconciles its rows with the open sockets: rows
  of vanished sockets (failed delete, aborted session) are dropped, missing rows (failed insert, replica
  purged after a long outage) restored. A crashed replica's rows stop counting after 90 s and the cleanup
  job deletes them. The replica id is random per process, so there is nothing of its own to clear at startup.
- **Liveness** — presence must not outlive a dead peer, or a sleeping phone would count as online and
  get no push. The server pings every socket every 25 s and closes one that has sent nothing (not even a
  pong) for 60 s (`Config::ws_ping_every` / `ws_idle_timeout`), which removes its presence row. The client
  closes its socket after the page has been hidden for 30 s (`utils/visibility.ts`) and reconnects when
  it becomes visible; `ready` then refetches what it missed.
- **Graceful shutdown** (`Hub::shutdown`, run when SIGTERM/ctrl-c arrives, before axum drains): closes
  local sockets with `1001` (clients reconnect, to another replica if there is one), stops the heartbeat
  and waits for it so it cannot re-register the replica, then deletes the replica row (presence cascades).

## Push notifications

Web Push (RFC 8030 + aes128gcm RFC 8291 + VAPID RFC 8292), optional: on only when `VAPID_PUBLIC_KEY`
and `VAPID_PRIVATE_KEY` are set (both or neither; a public key that does not match the private one
is a startup error). Crypto is `web-push-native` (pure RustCrypto, no OpenSSL), which only builds the
request. `reqwest` (rustls + ring, the stack sqlx already uses) sends it with redirects off, https only
and a 10 s timeout. `localdate-api vapid generate` / `make vapid-keys` prints a fresh pair.

- **One emit point** — `push::Notifier::send(actor, user, event)` (`AppState::notify`) replaces direct
  `Hub::send` for wave / match / message. It delivers over the WebSocket hub as before, then — unless
  `user == actor` — spawns a background task and returns, so the request path never waits on push.
  The task runs the cheapest checks first: a message whose coalescing slot is still taken (below) stops
  before any query, then a user without subscriptions stops after one. After that it skips missing or
  banned accounts, a preference turned off (`push_prefs`), and a recipient with an open socket on any
  replica (`Hub::is_online`, the `ws_presence` rows). Otherwise it sends to every subscription of the
  user, at most 16 sends in flight per replica (semaphore).
  Blocks and bans need no extra check here: the events are never emitted across them (visibility SQL,
  `participant_match`).
- **Coalescing** — at most one message push per (recipient, match) per 60 s, in memory per replica,
  so with several replicas a burst can push once per replica. Collapse on the device: notification
  `tag` per kind / match. Collapse at the push service: the RFC 8030 `Topic` header (`wave`, or for
  messages an HMAC-SHA256 of the match id, base64url cut to 32 chars, keyed by a key derived from the
  VAPID private key — the push service sees the header, so it must not carry the raw id). The service
  keeps only the newest undelivered push per topic.
- **Payload** — generic text in the subscription's `lang` (no names, no message bodies), the in-app
  route and the tag (docs/api.md "Push notifications"). TTL 1 h for waves and matches, 24 h for
  messages. The VAPID token is signed separately with a 12 h expiry, because push services reject
  tokens valid for more than 24 h. Its `aud` is the endpoint's origin with a lowercased host (services
  compare it literally), while the stored endpoint stays exactly as the browser sent it.
- **Outcomes** — 2xx sets `last_success_at` and resets `failure_count`. 404/410 deletes the
  subscription. Any other 4xx except 429 (and a stored subscription that cannot be encrypted for)
  increments `failure_count`, and 3 in a row delete it. 429, 5xx and network errors only get logged,
  with the endpoint host only (the full endpoint URL is a bearer capability).
- **SSRF** — the server POSTs to client-supplied URLs, so subscribing accepts only https endpoints on the
  default port on an allowlist of push-service hosts (`push::endpoint`). Redirects are not followed.
- **Subscriptions** — upserted by endpoint (a browser re-registered by another account moves to it),
  at most 10 per user (oldest registration dropped). An explicit logout first `DELETE`s the device's
  subscription with the still-valid token, then unsubscribes in the browser, then clears the tokens.
  A session lost any other way (failed refresh, ban) keeps the browser subscription, so push does not
  silently stop when a token expires. The next login's resync re-posts it, which moves it to whoever
  logged in.
- **Client** — custom service worker (`frontend/src/sw/sw.ts`, vite-plugin-pwa `injectManifest`):
  Workbox precache + SPA navigation fallback (deny `/api`, `/media`) as before, plus `push` →
  `showNotification` and `notificationclick` → focus an open tab of the same origin and route it
  (`postMessage`), or open a window. Routes are resolved against the origin and must stay on it
  (backslashes and control characters refused). Notifications use `icon-192.png` and the monochrome
  `badge-96.png`, PNGs generated from SVG by `make icons`. Settings has the opt-in (permission is
  requested on the click itself), per-kind toggles and opt-out. iOS gets Web Push only as a home-screen
  app, so Safari tabs see an install hint instead. `usePushSync` re-posts an existing subscription on
  app start and on language change. Endpoints can rotate, and the server keeps the device language.
  It never subscribes on its own.

## Configuration (env)

| Var | Example | Notes |
|---|---|---|
| `DATABASE_URL` | `postgres://localdate:localdate@localhost/localdate` | |
| `JWT_SECRET` | 32+ random bytes | required |
| `PHOTO_DIR` | `./data/photos` | created on start |
| `BIND_ADDR` | `127.0.0.1:3000` | |
| `CLEANUP_INTERVAL_SECS` | `300` | optional, default 300; period of the cleanup job |
| `TRUST_PROXY_HEADERS` | `false` | optional (`true`/`false`/`1`/`0`), default false; rate-limit on `X-Forwarded-For` (see Auth) |
| `APP_BASE_URL` | `https://localdate.mmik.cz` | required when email or Google is on; mailed links point at `{APP_BASE_URL}/auth/email…` and `/auth/password/reset`, the Google redirect URI is `{APP_BASE_URL}/api/auth/oauth/google/callback` |
| `SMTP_URL` | `smtps://user:pass@smtp.example.com:465` | optional, secret; lettre URL, enables email |
| `EMAIL_FROM` | `localdate <noreply@localdate.mmik.cz>` | required with `SMTP_URL` |
| `EMAIL_DEV_LOG` | `true` | optional, debug builds only (release refuses to start): log mails incl. links instead of sending; exclusive with `SMTP_URL` |
| `GOOGLE_CLIENT_ID` / `GOOGLE_CLIENT_SECRET` | from Google Cloud Console | optional (Secret); both or neither; unset = Google login off |
| `VAPID_PUBLIC_KEY` / `VAPID_PRIVATE_KEY` | from `make vapid-keys` | optional (Secret); both or neither; unset = Web Push off |
| `VAPID_SUBJECT` | `https://localdate.mmik.cz` | `mailto:` or `https://` contact; required when the keys are set |
| `RUST_LOG` | `info,sqlx=warn,localdate_api=debug` | sqlx logs every query at info |

Frontend dev server (Vite, :5173) proxies `/api` and `/media` (incl. WS) to `BIND_ADDR`.

## Serving the PWA

`web.rs` is the router fallback, so `/api/*` (which has its own JSON `not_found` fallback) and
`/media/*` always win. For other GET/HEAD requests the path is percent-decoded and refused with
404 if it contains `.`/`..`/empty segments, `\` or NUL (debug builds read from disk). Then:

- embedded file → served with its MIME type and an ETag (`If-None-Match` → 304);
  `assets/*` (Vite content-hashed) get `Cache-Control: public, max-age=31536000, immutable`,
  everything else (`index.html`, `sw.js` — the custom service worker with Workbox bundled in —,
  `registerSW.js`, manifest, icons) `no-cache`;
- no such file, last path segment has an extension → 404;
- otherwise (client-side route such as `/nearby`) → `index.html`, `no-cache`.

Other methods → 405. A debug binary without `frontend/dist` (`#[allow_missing]`, e.g. CI) answers
404 to every path outside `/api` and `/media`, and logs a warning at startup.

The frontend reloads once when a lazy chunk fails to load (`vite:preloadError`), which is what a
tab opened before a deploy hits; a 10 s `sessionStorage` guard prevents reload loops.
