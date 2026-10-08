# Architecture — Configuration and serving

Part of the [architecture](../architecture.md) docs.

## Configuration (env)

| Var | Example | Notes |
|---|---|---|
| `DATABASE_URL` | `postgres://localdate:localdate@localhost/localdate` | |
| `JWT_SECRET` | 32+ random bytes | required |
| `PHOTO_DIR` | `./data/photos` | created on start |
| `BIND_ADDR` | `127.0.0.1:3000` | |
| `CLEANUP_INTERVAL_SECS` | `300` | optional, default 300; period of the cleanup job |
| `TRUST_PROXY_HEADERS` | `false` | optional (`true`/`false`/`1`/`0`), default false; rate-limit on `X-Forwarded-For` (see [Auth](auth.md)) |
| `APP_BASE_URL` | `https://localdate.mmik.cz` | required when email, Google, Telegram login, the Telegram bot or Facebook is on; mailed (and Telegram) reset links point at `{APP_BASE_URL}/auth/email…` and `/auth/password/reset`, redirect URIs are `{APP_BASE_URL}/api/auth/oauth/{google,telegram,facebook}/callback` |
| `SMTP_URL` | `smtps://user:pass@smtp.example.com:465` | optional, secret; lettre URL, enables email |
| `EMAIL_FROM` | `localdate <noreply@localdate.mmik.cz>` | required with `SMTP_URL` |
| `EMAIL_DEV_LOG` | `true` | optional, debug builds only (release refuses to start): log mails incl. links instead of sending; exclusive with `SMTP_URL` |
| `GOOGLE_CLIENT_ID` / `GOOGLE_CLIENT_SECRET` | from Google Cloud Console | optional (Secret); both or neither; unset = Google login off |
| `TELEGRAM_CLIENT_ID` / `TELEGRAM_CLIENT_SECRET` | BotFather → bot → Login Widget | optional (Secret); both or neither; unset = Telegram login off |
| `TELEGRAM_BOT_TOKEN` | `123456789:AA…` (BotFather) | optional (Secret); unset = no Telegram reset messages; must look like `<digits>:<secret>` |
| `FACEBOOK_APP_ID` / `FACEBOOK_APP_SECRET` | from Meta for Developers | optional (Secret); both or neither; unset = Facebook login off; redirect URI `{APP_BASE_URL}/api/auth/oauth/facebook/callback` |
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
