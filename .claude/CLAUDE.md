# localdate

Meet people nearby right now — a PWA where a user opens a time-boxed **visibility window**
and sees every mutually-matching profile around them (distance band only, never coordinates),
waves, and chats after a mutual wave.

- Architecture, data model, config: [docs/architecture.md](../docs/architecture.md) and [docs/architecture/](../docs/architecture/)
- HTTP / WebSocket contract (source of truth for FE ↔ BE): [docs/api.md](../docs/api.md) and [docs/api/](../docs/api/)
- Deployment (Dockerfile, ghcr image jobs in ci.yml, `deploy/k8s.yml`, `deploy/letsgo/` example, secrets, backups): [docs/deployment.md](../docs/deployment.md)
- Tasks: GitHub issues in `xmiksay/localdate`

## Stack

- `backend/` — Rust workspace: `api` (Axum, binary `localdate-api`), `entity` (SeaORM), `migration`.
  `anyhow` in main/setup, typed `AppError` (→ `{error:{code,message}}`) in handlers, `tracing`.
- `frontend/` — Vue 3 `<script setup>` + TS + Vite + Tailwind + Pinia + vue-i18n (cs default, en) + vite-plugin-pwa.
- Postgres on the host (no docker-compose); photos on local disk (`PHOTO_DIR`). Deploy: Kubernetes, namespace
  `localdate`, single API replica (photos on an RWO PVC; WS already fans out across replicas) + Postgres
  StatefulSet, ingress `localdate.mmik.cz`.

## Code map

- `backend/api/src/`: `auth/` (password, jwt, refresh rotation, `AuthUser` extractor, `session()` = new Tokens;
  `email/` = magic link: `token` single-use tokens + strict `normalize_email` → `lettre::Address`, `flow` preview/
  verify/signup, `message` cs/en texts, `limit` (address, IP) + per-address limiter, `EmailService` = spawned bounded
  sends, in `AppState::email`, `None` = disabled; `limit` also has the separate `ResetLimiter`; `reset.rs` = password reset
  (`reset/deliver.rs`: email + Telegram channels, run after the response via `AppState::detached` = `detached.rs`;
  `telegram/` = the bot for reset links: `TelegramBot` trait, `HttpBot` sendMessage, `MemoryBot`, `TelegramService` in
  `AppState::telegram`, `None` without `TELEGRAM_BOT_TOKEN`) (`replace_password` = hash + `credentials_changed_at` + revoke sessions + delete push subscriptions + void mailed tokens, shared with `PUT /me/password`; both close
  the account's sockets with 4401 after commit); `oauth/` = generic OIDC / OAuth 2.0 code flow (Google, Telegram — subject = numeric `id` claim, Facebook):
  `config` per-provider `OidcConfig` + env presets + `SubjectSource` (`IdTokenClaim` | `Userinfo` = Graph `/me` + `appsecret_proof`), `OAuthService` registry `Provider → Oidc` in `AppState::oauth`, `cookie`
  signed `ld_oauth` flow cookie + PKCE + `safe_redirect`, `oidc` client + single-flight/stale-tolerant JWKS cache +
  ID token checks + userinfo calls, `grant` one-time codes / sign-up tokens in `oauth_grant` (+ pending imported photo),
  `notice` link notice mail, `import` profile picture import (CDN host allowlist, size cap, `photos::to_webp`/`add`),
  `flow` start/link/callback, `exchange` exchange/signup), `mail.rs` (`Mailer` trait: lettre SMTP, dev log,
  `MemoryMailer` for tests), `me/` (profile, photos + `image_proc`, filter, `identities` = linked login methods +
  email linking, `password` = change / first password, `DELETE /me`), `discovery/` (`geo` bands/haversine, `rules::mutually_visible` = spec,
  `duration` presets/12 h cap/end of day, `window`, `location` (location updates + area leave check), `nearby` = the **only runtime visibility SQL**,
  reused by waves, + shared-interest ranking), `areas/` (`GET /areas` containment, admin CRUD `/admin/areas`),
  `social/` (waves, matches, messages), `ws/` (`hub` = per-replica `LocalHub`, `bridge` = cross-replica `Hub` over Postgres LISTEN/NOTIFY (own listener
  connection), `publisher` (bounded ordered NOTIFY queue, Close ops retried),
  `envelope` wire format + `presence` (`Hub::is_online`, `ws_presence`/`ws_replica`), session (re-reads the account every 60 s); `Hub::disconnect` on ban), `push/` (Web Push: `Notifier::send` =
  the **only** wave/match/message emit point — WS, then a background push to offline recipients; `PushSender`
  trait (tests: `tests/common/push.rs` recorder), endpoint allowlist, VAPID, HMAC topics, subscription/prefs
  routes), `sql.rs` (`exec` for raw statements), `cleanup.rs` (retention
  job spawned from `main`; tests call `run_once(db, shift)`), `web.rs` (router fallback serving the `rust-embed`ded
  `frontend/dist`: files, SPA `index.html` fallback, cache headers/ETag, traversal guard; `/api` has its
  own JSON 404 fallback so it never gets the shell; `build.rs` rebuilds on dist changes and refuses a
  release build without it), `safety.rs` (blocks/reports, `is_blocked_between`, `blocked_with` — both also cover banned accounts),
  `admin/` (report queue, soft ban/unban, `set_admin` for the CLI), `error.rs`
  (`AppError`, `AppJson`, `parse_id`), `rate_limit.rs` (per-IP bucket; `TRUST_PROXY_HEADERS` keys on the
  first `X-Forwarded-For` entry), `retry.rs` (shared reconnect backoff). New domain = module with `router()` merged in `lib.rs`.
  `AuthUser` loads the account on every request (deleted or token `iat` before `credentials_changed_at` → 401,
  banned → 403 `banned`; the WS re-check applies the same); `AdminUser` also
  needs `is_admin`; `lock_unbanned` (`FOR SHARE`) guards window/wave/refresh writes against a concurrent ban;
  `lock_user` (`FOR UPDATE`, same account mapping) for writes to the user row itself (password).
  `main.rs` is a clap CLI: no subcommand = serve, `admin grant|revoke <username>` (refuses with pending migrations).
- `backend/api/tests/common/mod.rs`: `TestApp` harness (fresh DB per test, `register`, `onboard`, `open_window`,
  `visible_user`, `match_up`, `admin`, multipart helpers; `with_frontend::<F>()` serves a fixture bundle from `tests/fixtures/dist`;
  `with_config(|c| …)` tweaks the `Config`, e.g. to enable the rate limiter);
  `common/areas.rs`: `area`, `area_user`, `start_area_window`; `common/ws.rs`: WS client (`serve`, `ready_socket`, `next`);
  `common/replica.rs`: `app.replica()` = second API replica on the same DB (own pool + hub) for `tests/ws_replicas.rs`.
  `common/email.rs`: `TestApp::with_email()` (email on, mail captured in `outbox`), `mails_to`, `last_link`,
  `email_signup`. `tests/password_reset.rs` / `password_change.rs` / `password_sessions.rs` (sockets, voided tokens) cover #18. `common/oidc_fake.rs`: `FakeProvider` (local token/JWKS server, fixture RSA key, `config_for(provider)`,
  `IdClaims` — empty `nonce` = left out); `common/oauth.rs`: browser side, `TestApp::with_google()`, `with_google_opts(email, tweak)`, `with_oauth(provider, …)`, `oauth_start`/`oauth_link`/`oauth_return`
  (+ `_as(provider)`)/`oauth_exchange`, `settle_messages` (waits for detached work + mail/bot sends);
  `tests/oauth_login.rs` / `oauth_link.rs` / `oauth_hardening.rs`. `common/telegram.rs`: `with_telegram(email, bot)`,
  `claims(started, id)`, `telegram_signup` / `telegram_link`, `bot_messages_to`; `tests/telegram_login.rs` +
  `telegram_reset.rs` cover #14. `common/facebook.rs`: `TestApp::with_facebook()` + `FakeFacebook` (token, `/me`,
  `/me/picture`, CDN; checks PKCE + `appsecret_proof`), `fb_start`/`fb_link`/`fb_return`/`fb_signup`;
  `tests/oauth_facebook.rs` / `oauth_facebook_photo.rs` (import, photo cap, SSRF refusals) cover #17.
  `tests/visibility_agreement.rs` = SQL ↔ rule cross-check.
- `frontend/src/`: `api/` (typed client with single-flight refresh, `ws.ts`, per-domain modules, `types.ts` mirrors
  docs/api.md + docs/api/; any `403 banned` or WS close `4403` → `onBanned` → logout + suspended notice on `/login`),
  `stores/` (auth incl. providers + email login, me, window, nearby, matches, safety, admin, areas, identities, push),
  `views/EmailAuthView.vue` (`/auth/email`: verify → login or username sign-up), `EmailLinkView.vue`
  (`/auth/email/link`, confirm linking), `ForgotPasswordView.vue` (`/auth/password/forgot`) + `PasswordResetView.vue`
  (`/auth/password/reset`: preview → new password), `OAuthDoneView.vue` (`/auth/oauth/done`: fragment → exchange →
  login or username sign-up; `linked`/`error`), `api/oauth.ts`, `LinkedAccountsSection.vue` + `PasswordSection.vue` (Settings), `utils/redirect.ts` (`?redirect=` guard),
  `sw/` (custom service worker:
  `sw.ts` precache + push handlers, own `tsconfig.sw.json`; `push.ts` = the handlers as pure, tested functions),
  `utils/webPush.ts` (support/iOS detection + the only `PushManager`/`Notification` adapter),
  `utils/visibility.ts` (closes the WS after 30 s hidden), `scripts/icons.sh` (`make icons`: PNG icons from SVG),
  `NotificationsSection.vue` (Settings opt-in), `composables/usePushSync.ts`, `views/AdminView.vue` (`/admin`, admins only:
  reports + areas tabs), `composables/` (geolocation sharing — `left_area` ends the window and sets the notice
  shown by `WindowEndedNotice` in `AppLayout`; realtime), `i18n/cs/` (source of truth, one module per domain — core, auth, profile, discovery, moderation, notifications — merged in
  `cs/index.ts`; each `en/<domain>.ts` is typed against its cs module via `Messages<T>` in `i18n/messages.ts`;
  `i18n/plural.ts` = Czech one/few/many rule; `i18n/typed.ts` `useT()` = key-checked `t`, use it instead of
  `useI18n`, enforced by ESLint), `utils/interests.ts` (`sharedFirst` chips, `byOverlapThenBand` client-side
  nearby order), `utils/imageResize.ts` (`prepareUpload`: re-encode to a ≤ 2048 px JPEG unless already an accepted small file, before `PhotoManager` uploads),
  `SharedInterestsBadge.vue`, `AreaPicker.vue` (area mode of the window start panel), `components/ui/`
  primitives (`PillRadios` = shared pill radiogroup).

## Commands (always via make)

```
make install           # npm ci
make run-api           # API on BIND_ADDR, runs migrations on start
make run-web           # Vite on :5173, proxies /api, /media, WS
make build             # npm run build, then cargo build --release (binary embeds frontend/dist)
make lint              # check-pins (Dockerfile vs toolchain pins), cargo fmt --check, clippy -D warnings, eslint, vue-tsc
make test              # test-unit (cargo --lib/--bins + vitest) + test-integration (cargo tests/)
make migrate
make vapid-keys        # fresh VAPID pair for Web Push (VAPID_* env; unset = push off)
make admin-grant ADMIN=<username>   # / admin-revoke — moderator role via `localdate-api admin …`
make image             # docker build -t localdate:dev .
make deploy            # kubectl apply -f deploy/k8s.yml (current context!)
make deploy-letsgo     # kubectl apply -k deploy/letsgo (letsgo.sc-l.eu example; -diff to preview)
```

Copy `.env.example` → `.env`. Local DB: role/db `localdate` (password `localdate`, CREATEDB for test DBs).

## Testing

- Backend: unit tests next to the code for pure logic (geo/bands, filter matching, validation, tokens);
  integration tests in `backend/api/tests/` drive the router with `tower::ServiceExt::oneshot`
  against a fresh `localdate_test_<uuid>` database per test (created/dropped via `TEST_DATABASE_URL`).
- Frontend: Vitest unit tests for stores, API client and pure utils. **No E2E / click-through tests yet.**

## Conventions

- Contract changes go into `docs/api.md` / `docs/api/` first, then both sides.
- Migrations are append-only.
- Never return coordinates or birth dates of other users.
