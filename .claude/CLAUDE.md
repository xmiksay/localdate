# localdate

Meet people nearby right now — a PWA where a user opens a time-boxed **visibility window**
and sees every mutually-matching profile around them (distance band only, never coordinates),
waves, and chats after a mutual wave.

- Architecture, data model, config: [docs/architecture.md](../docs/architecture.md)
- HTTP / WebSocket contract (source of truth for FE ↔ BE): [docs/api.md](../docs/api.md)
- Tasks: GitHub issues in `xmiksay/localdate`

## Stack

- `backend/` — Rust workspace: `api` (Axum, binary `localdate-api`), `entity` (SeaORM), `migration`.
  `anyhow` in main/setup, typed `AppError` (→ `{error:{code,message}}`) in handlers, `tracing`.
- `frontend/` — Vue 3 `<script setup>` + TS + Vite + Tailwind + Pinia + vue-i18n (cs default, en) + vite-plugin-pwa.
- Postgres on the host (no docker-compose); photos on local disk (`PHOTO_DIR`). Deploy target: Kubernetes.

## Code map

- `backend/api/src/`: `auth/` (password, jwt, refresh rotation, `AuthUser` extractor), `me/` (profile, photos +
  `image_proc`, filter, `DELETE /me`), `discovery/` (`geo` bands/haversine, `rules::mutually_visible` = spec,
  `window`, `location` (location updates + area leave check), `nearby` = the **only runtime visibility SQL**,
  reused by waves, + shared-interest ranking), `areas/` (`GET /areas` containment, admin CRUD `/admin/areas`),
  `social/` (waves, matches, messages), `ws/` (hub + session, `Hub::disconnect` on ban), `cleanup.rs` (retention
  job spawned from `main`; tests call `run_once(db, shift)`), `web.rs` (router fallback serving the `rust-embed`ded
  `frontend/dist`: files, SPA `index.html` fallback, cache headers/ETag, traversal guard; `/api` has its
  own JSON 404 fallback so it never gets the shell; `build.rs` rebuilds on dist changes and refuses a
  release build without it), `safety.rs` (blocks/reports, `is_blocked_between`, `blocked_with` — both also cover banned accounts),
  `admin/` (report queue, soft ban/unban, `set_admin` for the CLI), `error.rs`
  (`AppError`, `AppJson`, `parse_id`), `rate_limit.rs`. New domain = module with `router()` merged in `lib.rs`.
  `AuthUser` loads the account on every request (deleted → 401, banned → 403 `banned`); `AdminUser` also
  needs `is_admin`; `lock_unbanned` (`FOR SHARE`) guards window/wave/refresh writes against a concurrent ban.
  `main.rs` is a clap CLI: no subcommand = serve, `admin grant|revoke <username>` (refuses with pending migrations).
- `backend/api/tests/common/mod.rs`: `TestApp` harness (fresh DB per test, `register`, `onboard`, `open_window`,
  `visible_user`, `match_up`, `admin`, multipart helpers; `with_frontend::<F>()` serves a fixture bundle from `tests/fixtures/dist`);
  `common/areas.rs`: `area`, `area_user`, `start_area_window`. `tests/visibility_agreement.rs` = SQL ↔ rule cross-check.
- `frontend/src/`: `api/` (typed client with single-flight refresh, `ws.ts`, per-domain modules, `types.ts` mirrors
  docs/api.md; any `403 banned` or WS close `4403` → `onBanned` → logout + suspended notice on `/login`),
  `stores/` (auth, me, window, nearby, matches, safety, admin, areas), `views/AdminView.vue` (`/admin`, admins only:
  reports + areas tabs), `composables/` (geolocation sharing — `left_area` ends the window and sets the notice
  shown by `WindowEndedNotice` in `AppLayout`; realtime), `i18n/cs.ts` (source of truth; `en.ts` typed against it;
  `i18n/plural.ts` = Czech one/few/many rule; `i18n/typed.ts` `useT()` = key-checked `t`, use it instead of
  `useI18n`, enforced by ESLint), `utils/interests.ts` (`sharedFirst` chips, `byOverlapThenBand` client-side
  nearby order), `SharedInterestsBadge.vue`, `AreaPicker.vue` (area mode of the window start panel), `components/ui/`
  primitives (`PillRadios` = shared pill radiogroup).

## Commands (always via make)

```
make install           # npm ci
make run-api           # API on BIND_ADDR, runs migrations on start
make run-web           # Vite on :5173, proxies /api, /media, WS
make build             # npm run build, then cargo build --release (binary embeds frontend/dist)
make lint              # cargo fmt --check, clippy -D warnings, eslint, vue-tsc
make test              # test-unit (cargo --lib/--bins + vitest) + test-integration (cargo tests/)
make migrate
make admin-grant ADMIN=<username>   # / admin-revoke — moderator role via `localdate-api admin …`
```

Copy `.env.example` → `.env`. Local DB: role/db `localdate` (password `localdate`, CREATEDB for test DBs).

## Testing

- Backend: unit tests next to the code for pure logic (geo/bands, filter matching, validation, tokens);
  integration tests in `backend/api/tests/` drive the router with `tower::ServiceExt::oneshot`
  against a fresh `localdate_test_<uuid>` database per test (created/dropped via `TEST_DATABASE_URL`).
- Frontend: Vitest unit tests for stores, API client and pure utils. **No E2E / click-through tests yet.**

## Conventions

- Contract changes go into `docs/api.md` first, then both sides.
- Migrations are append-only.
- Never return coordinates or birth dates of other users.
