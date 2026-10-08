# Deployment

Production runs on the k8s cluster behind ingress-nginx + cert-manager (`letsencrypt-prod`) at
<https://localdate.mmik.cz>. Everything lives in namespace `localdate`, defined in one file:
[`deploy/k8s.yml`](../deploy/k8s.yml).

| Object | What |
|---|---|
| `ConfigMap localdate` | non-secret env (`BIND_ADDR`, `PHOTO_DIR`, `RUST_LOG`, `CLEANUP_INTERVAL_SECS`, `TRUST_PROXY_HEADERS=true`, `APP_BASE_URL`, `EMAIL_FROM`, `VAPID_SUBJECT`) |
| `Secret localdate` | `POSTGRES_PASSWORD`, `JWT_SECRET`, optional `TELEGRAM_CLIENT_ID` + `TELEGRAM_CLIENT_SECRET`, optional `TELEGRAM_BOT_TOKEN`, optional `SMTP_URL`, optional `GOOGLE_CLIENT_ID` + `GOOGLE_CLIENT_SECRET`, optional `FACEBOOK_APP_ID` + `FACEBOOK_APP_SECRET`, optional `VAPID_PUBLIC_KEY` + `VAPID_PRIVATE_KEY` — **not in git**, created by hand (below) |
| `StatefulSet localdate-db` + headless `Service` | Postgres 18, 5 Gi PVC `data-localdate-db-0` |
| `NetworkPolicy localdate-db` | only `app=localdate-api` pods may reach 5432 |
| `Deployment localdate-api` | 1 replica, `Recreate`, 768 Mi memory limit, 5 Gi PVC `localdate-photos` at `/data/photos` |
| `NetworkPolicy localdate-api` | only namespace `ingress-nginx` may reach 3000 (adjust if the controller lives elsewhere) |
| `Service` + `Ingress localdate` | class `nginx`, TLS for `localdate.mmik.cz`, body limit 11m, 1 h proxy timeouts for `/api/ws` |

The NetworkPolicies only take effect if the cluster's CNI enforces them.

Why one replica: photos live on the `ReadWriteOnce` PVC, which only one pod can mount; more replicas
need shared photo storage (S3, #21). WebSocket push is not the blocker any more — replicas fan events
out to each other through Postgres LISTEN/NOTIFY ([architecture/realtime-push.md](architecture/realtime-push.md#realtime)). Why `Recreate`: the photos PVC is
`ReadWriteOnce`, so the old pod must release it before the new one starts (a few seconds of downtime
per rollout). The PVCs use the cluster's default StorageClass; add `storageClassName` if there is none.

**Client IP.** `TRUST_PROXY_HEADERS=true` makes the login/register rate limiter key on the first
`X-Forwarded-For` entry. ingress-nginx overwrites that header with the address *it* sees, so this only
works if ingress-nginx sees the real client IP. That means a hostNetwork controller, or a controller
Service with `externalTrafficPolicy: Local`, or PROXY protocol between a load balancer and the
controller. If kube-proxy SNATs traffic on the way in, every user shares one rate-limit bucket. To
check, look for real public IPs (not node or pod addresses) in the ingress-nginx access log.

Never enable `use-forwarded-headers: true` in the ingress-nginx ConfigMap without narrowing
`proxy-real-ip-cidr`. Its default is `0.0.0.0/0`, so the controller would take the client's own
`X-Forwarded-For` as the real IP. Every client could then pick its own bucket and get around the
limiter. The same applies to `compute-full-forwarded-for: true`: it appends to the header instead of
replacing it, so the first entry becomes whatever the client sent.

IPv6 clients are bucketed per /64 prefix, so rotating addresses inside one prefix doesn't help.

## Image

[`Dockerfile`](../Dockerfile): Node 22 builds `frontend/dist`. Rust then builds the release
`localdate-api`, which embeds the bundle. The runtime is `debian:trixie-slim` running as uid 10001,
with `PHOTO_DIR=/data/photos` and `BIND_ADDR=0.0.0.0:3000`. The Rust base tag must match
`rust-toolchain.toml` and the Node tag must match `.nvmrc`. `make check-pins` enforces both and runs
as part of `make lint`. Dependencies are compiled in a separate layer from stubbed sources, so a
code-only change rebuilds just the workspace crates. A new workspace crate has to be added to that
stub list.

- Local: `make image` → `localdate:dev`.
- CI ([`ci.yml`](../.github/workflows/ci.yml)) only builds the image after the `gate` job (lint +
  test) passes:
  - On pull requests, `image-build` builds it without pushing, so Dockerfile breakage shows up before
    merge.
  - On a push to `master`, `image-publish` pushes `ghcr.io/xmiksay/localdate:master` and
    `:sha-<short>`. On a `v*` tag it pushes `:<version>`. Only this job gets `packages: write`.
- The package must be public for the cluster to pull it without credentials. Otherwise add an
  `imagePullSecrets` entry with a read-only ghcr token.

Smoke-test an image against the host Postgres:

```sh
docker run --rm --network host -e DATABASE_URL=postgres://localdate:localdate@localhost/localdate \
  -e JWT_SECRET="$(openssl rand -hex 32)" -e BIND_ADDR=127.0.0.1:3900 localdate:dev
curl -s localhost:3900/api/health; curl -sI localhost:3900/
```

## First deploy

`make deploy` runs `kubectl apply -f deploy/k8s.yml` against the **current kubectl context** — check
it (`kubectl config current-context`) first.

```sh
kubectl apply -f deploy/k8s.yml                 # namespace etc.; pods wait for the Secret
kubectl -n localdate create secret generic localdate \
  --from-literal=POSTGRES_PASSWORD="$(openssl rand -hex 24)" \
  --from-literal=JWT_SECRET="$(openssl rand -hex 32)"
kubectl -n localdate rollout status statefulset/localdate-db
kubectl -n localdate rollout status deployment/localdate-api
```

`POSTGRES_PASSWORD` is expanded into `DATABASE_URL` by the kubelet, so keep it URL-safe (hex is).
Postgres only reads it when initialising an empty volume: rotating it later means `ALTER ROLE
localdate PASSWORD '…'` inside the database **and** updating the Secret, then restarting the API.
Rotating `JWT_SECRET` invalidates outstanding access tokens; clients recover through their refresh token.
[`deploy/secrets.example.yml`](../deploy/secrets.example.yml) is the same Secret as a template.

## Email (magic link)

Email login and account linking need SMTP. Without `SMTP_URL` in the Secret the API starts with
email disabled: `GET /api/auth/providers` says `email: false`, the frontend hides the email UI and the
email endpoints answer `503 email_disabled`. To turn it on, add the SMTP URL (lettre syntax, credentials
inside; `smtps://` = implicit TLS on 465, `smtp://…?tls=required` = STARTTLS on 587; percent-encode
special characters in the password) and restart:

```sh
kubectl -n localdate patch secret localdate --type merge \
  -p '{"stringData":{"SMTP_URL":"smtps://USER:PASSWORD@smtp.example.com:465"}}'
kubectl -n localdate rollout restart deployment/localdate-api
```

`APP_BASE_URL` (origin the mailed links point at) and `EMAIL_FROM` live in the ConfigMap; the sender
domain needs SPF/DKIM at the mail provider or the links land in spam. `EMAIL_DEV_LOG` (log links instead
of sending) is for local development only — the release binary refuses to start with it.
## Sign in with Google (optional)

Without `GOOGLE_CLIENT_ID` / `GOOGLE_CLIENT_SECRET` the API runs with Google off
(`GET /api/auth/providers` → `google: false`, the button is hidden). To turn it on:

1. Google Cloud Console → APIs & Services → **OAuth consent screen**: app name, support email,
   authorized domain `mmik.cz`; only the default `openid` scope is needed (no sensitive scopes, no
   verification). Publish the app (status "In production"), otherwise only listed test users can log in.
2. **Credentials → Create credentials → OAuth client ID**, type *Web application*.
   Authorized redirect URI: `https://localdate.mmik.cz/api/auth/oauth/google/callback`
   (= `{APP_BASE_URL}/api/auth/oauth/google/callback`; for local dev add
   `http://localhost:5173/api/auth/oauth/google/callback`). No JavaScript origins are needed.
3. Put both values into the Secret and restart:

```sh
kubectl -n localdate patch secret localdate --type merge \
  -p '{"stringData":{"GOOGLE_CLIENT_ID":"<id>.apps.googleusercontent.com","GOOGLE_CLIENT_SECRET":"<secret>"}}'
kubectl -n localdate rollout restart deployment/localdate-api
```

`APP_BASE_URL` (ConfigMap) must be the exact public origin, or Google refuses the redirect URI. The API
refuses to start with only one of the two values. The pod needs outbound HTTPS to `oauth2.googleapis.com`
and `www.googleapis.com` (token endpoint, signing keys). Rotating the client secret only needs the Secret
updated; Google accounts stay linked (they are keyed by Google's account id, not the client).

## Telegram login and reset messages (optional)

Two independent switches. Without `TELEGRAM_CLIENT_ID` / `TELEGRAM_CLIENT_SECRET` Telegram login is off
(`GET /api/auth/providers` → `telegram: false`, the button is hidden). Without `TELEGRAM_BOT_TOKEN` the bot
sends no password-reset links (accounts reset by email only). Login uses Telegram's OpenID Connect flow
(`oauth.telegram.org`), like Google; no Telegram script is loaded on our pages.

1. In [@BotFather](https://t.me/BotFather): `/newbot` (or pick the existing bot); note its **token**.
2. BotFather → the bot → **Login Widget**: add the Allowed URLs `https://localdate.mmik.cz` and
   `https://localdate.mmik.cz/api/auth/oauth/telegram/callback` (= `{APP_BASE_URL}/api/auth/oauth/telegram/callback`;
   for local dev add the `http://localhost:5173/…` one too, if BotFather accepts it). Copy the **Client ID** and
   **Client Secret** shown there. Leave the signing algorithm (Login Widget → Advanced) at **RS256** — the API
   accepts only RS256 ID tokens.
3. Put the values into the Secret and restart:

```sh
kubectl -n localdate patch secret localdate --type merge \
  -p '{"stringData":{"TELEGRAM_CLIENT_ID":"<id>","TELEGRAM_CLIENT_SECRET":"<secret>","TELEGRAM_BOT_TOKEN":"123456789:AA…"}}'
kubectl -n localdate rollout restart deployment/localdate-api
```

The API refuses to start with only one of the client values, a bot token that does not look like
`<digits>:<secret>`, or — with both login and the bot set — a token of another bot than the login client (the
token's numeric prefix must equal `TELEGRAM_CLIENT_ID`: users grant message access to the bot they log in with). The pod needs outbound HTTPS to `oauth.telegram.org` (token endpoint, signing keys) and
`api.telegram.org` (`sendMessage`). The bot token is never logged. Telegram accounts are keyed by the numeric
Telegram user id, so rotating the client secret or the bot token keeps them linked.

**Check on the first real login** (the docs leave these open; the code follows the documented answer):

- **Client ID = `aud`.** The ID token's `aud` must equal `TELEGRAM_CLIENT_ID`; Telegram documents `aud` as the bot
  id. If BotFather's Client ID differs, every login ends in `#error=oauth_failed` (logged as an audience mismatch).
- **RS256.** A token signed with another algorithm is refused (`oauth_failed`); keep BotFather's default.
- **Nonce.** The API sends a `nonce` but accepts a Telegram ID token without one (a wrong one is always refused).
  Decode one real ID token (log it once in a dev build): if it carries the `nonce`, tighten the Telegram preset to
  `NonceCheck::Required` in `backend/api/src/auth/oauth/config.rs`, like Google.
- **Bot access.** After a Telegram login, request a password reset by username: the link must arrive in the chat
  with the bot. If it does not (the `telegram:bot_access` consent missing or declined, or the user blocked the bot),
  the API logs "sending Telegram message failed" with Telegram's reason.

## Facebook login (optional)

Without `FACEBOOK_APP_ID` / `FACEBOOK_APP_SECRET` the API runs with Facebook off
(`GET /api/auth/providers` → `facebook: false`, the button is hidden). The app asks only for
`public_profile` (the app-scoped user id, plus the profile picture when the user ticks the import box), which
needs **no App Review**. To turn it on:

1. <https://developers.facebook.com/apps> → **Create app**, use case *Authenticate and request data from users
   with Facebook Login*. Keep the default permission `public_profile` only; add nothing else (no `email`).
2. **Facebook Login → Settings**: *Client OAuth login* and *Web OAuth login* on, *Enforce HTTPS* and *Use Strict
   Mode for redirect URIs* on, **Valid OAuth Redirect URIs**:
   `https://localdate.mmik.cz/api/auth/oauth/facebook/callback`
   (= `{APP_BASE_URL}/api/auth/oauth/facebook/callback`; Facebook accepts `http://localhost` redirects only while
   the app is in Development mode, so use a separate dev app for `http://localhost:5173/...`).
3. **App settings → Basic**: App Domains `localdate.mmik.cz`, a **Privacy Policy URL** and **User data deletion →
   Data deletion instructions URL** — both are required before the app can go Live. The instructions page should
   say: delete the account in the app (Settings → *Smazat účet*, which is `DELETE /api/me` and hard-deletes the
   account, its Facebook link and all photos), or just remove the Facebook link under Settings → Propojené účty
   (and the app in Facebook's *Apps and websites* settings). These pages are not part of this repo yet; publish
   them before going Live.
4. Switch the app to **Live** mode (top bar). In Development mode only the app's roles can log in.
5. Copy *App ID* and *App secret* into the Secret and restart:

```sh
kubectl -n localdate patch secret localdate --type merge \
  -p '{"stringData":{"FACEBOOK_APP_ID":"<app id>","FACEBOOK_APP_SECRET":"<app secret>"}}'
kubectl -n localdate rollout restart deployment/localdate-api
```

The API refuses to start with only one of the two values, or without `APP_BASE_URL`. The pod needs outbound
HTTPS to `graph.facebook.com` (token, `/me`, `/me/picture`) and, for the picture import, `*.fbcdn.net`.
Facebook user ids are **app-scoped**: keep using the same app. Resetting the app secret is fine (update the Secret,
restart), but a new App ID gives every user a new id, so their existing Facebook links would no longer match.

## Web Push (optional)

Without VAPID keys the API runs with push off (`GET /api/push/config` → `enabled: false`). To turn it
on, generate a pair once, add both keys to the existing Secret and restart:

```sh
make vapid-keys    # prints VAPID_PUBLIC_KEY=… and VAPID_PRIVATE_KEY=…
kubectl -n localdate patch secret localdate --type merge \
  -p '{"stringData":{"VAPID_PUBLIC_KEY":"<public>","VAPID_PRIVATE_KEY":"<private>"}}'
kubectl -n localdate rollout restart deployment/localdate-api
```

The API refuses to start when only one key is set or the public key does not belong to the private
one. `VAPID_SUBJECT` (ConfigMap) is the contact push services see. Keep the keys stable: rotating them
invalidates every browser subscription. A client renews its subscription on its next start only if it
still holds one with permission granted. The pod needs outbound HTTPS to the push services
(`fcm.googleapis.com`, `updates.push.services.mozilla.com`, `*.push.apple.com`, `*.notify.windows.com`);
the NetworkPolicies only restrict ingress, so that works as is.

## Updates and migrations

On start the API retries the Postgres connection with backoff for 60 s, then gives up. The last
attempt can add up to 30 s if it times out. This matters on the first deploy, when the DB and the API start together. It then
runs migrations (`Migrator::up`); the startup probe gives it up to 5 minutes. With `Recreate`, the
old binary never runs against the new schema.

```sh
# New master build (the manifest tracks :master with imagePullPolicy: Always)
kubectl -n localdate rollout restart deployment/localdate-api
# After changing the ConfigMap: env is read at start only, so restart too
kubectl apply -f deploy/k8s.yml && kubectl -n localdate rollout restart deployment/localdate-api
# Rollback: pin a known-good build (tags are listed on the ghcr package page)
kubectl -n localdate set image deployment/localdate-api api=ghcr.io/xmiksay/localdate:sha-<short>
```

A pinned image stays until the next `make deploy`, which puts `:master` back. You can only roll back
to a build that knows every migration already applied to the database. An older binary refuses to
start when the database has a migration it doesn't know. Rolling back across a migration therefore
means restoring a database backup.

## Admin role

Run the CLI inside the API pod, where `DATABASE_URL` is already set. It refuses to run while
migrations are pending, so the server has to have started (and migrated) first. After a rollout,
wait for `rollout status` before running it.

```sh
kubectl -n localdate exec deploy/localdate-api -- localdate-api admin grant <username>
kubectl -n localdate exec deploy/localdate-api -- localdate-api admin revoke <username>
```

## Backups

Two things hold state: the database and the photos volume. Back up both together (a photo row
without its file renders a broken image; a file without a row is just garbage).

```sh
# Database (custom format; restore with pg_restore -d localdate)
kubectl -n localdate exec localdate-db-0 -- pg_dump -U localdate -Fc localdate > localdate-$(date +%F).dump
# Photos
kubectl -n localdate exec deploy/localdate-api -- tar -C /data -cf - photos > photos-$(date +%F).tar
```

Dumps contain personal data (profiles, birth dates, messages, photos) — store them encrypted.
