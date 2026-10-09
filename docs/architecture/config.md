# Architecture — Configuration and serving

Part of the [architecture](../architecture.md) docs.

## Configuration (env)

| Var | Example | Notes |
|---|---|---|
| `DATABASE_URL` | `postgres://localdate:localdate@localhost/localdate` | |
| `JWT_SECRET` | 32+ random bytes | required |
| `PHOTO_STORAGE` | `disk` | optional, `disk` (default) or `s3`: where photo files live (see [Photos](data-model.md#photos)) |
| `PHOTO_DIR` | `./data/photos` | required with `disk` (created on start), ignored with `s3` |
| `S3_ENDPOINT` | `http://garage.services.svc:3900` | required with `s3`; `http(s)://`, addressed path-style (`{endpoint}/{bucket}/{name}`) |
| `S3_REGION` / `S3_BUCKET` | `garage` / `localdate` | required with `s3` |
| `S3_ACCESS_KEY_ID` / `S3_SECRET_ACCESS_KEY` | from Garage `make bucket` | required with `s3` (Secret); never logged. The names match the Garage Secret, which the Deployment can `envFrom`; its extra `S3_PUBLIC_ENDPOINT` is ignored |
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

On start, after migrations and **before binding** `BIND_ADDR`, the API checks the photo storage: it writes and
deletes a probe object `_probe-<uuid>` (proves the bucket exists and the key may write and delete; the prefix
can never be a photo name, so `/media` never serves one). A refusal — 403 credentials or read-only key, 404
bucket, any other 4xx — stops the start at once; an outage (connection failure, 5xx) is retried with backoff for
60 s like the DB connection, then the start fails too. Bad S3 settings thus fail the rollout (the pod never turns
ready), not the first upload.

## Serving the PWA

`web.rs` is the router fallback, so `/api/*` (which has its own JSON `not_found` fallback) and
`/media/*` always win (`/media` is the `media` module: photo names only, see [Photos](data-model.md#photos)). For other GET/HEAD requests the path is percent-decoded and refused with
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
