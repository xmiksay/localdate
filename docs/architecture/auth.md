# Architecture — Auth

Part of the [architecture](../architecture.md) docs.

## Auth

Username + password (argon2id), an **email magic link**, or **Sign in with Google / Telegram / Facebook**; a forgotten password is
reset through a linked email.

**Identity model.** A *login method* is the password (`user.password_hash`, optional) or a row in
`user_identity` — `(provider, subject)` unique, so one address belongs to at most one account. Email, Google and
Telegram and Facebook are the providers now; a new one adds an enum value and its own verify step,
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

**Password reset** (`auth/reset.rs`). `POST /auth/password/forgot` takes a username or an address (`deliver::Login`: tried as both when it is both, since usernames may contain `@`),
charges `ResetLimiter` (`auth/email/limit.rs`; a budget separate from the magic-link `EmailLimiter`, so neither
drains the other: 3 per (input as typed, IP) / 15 min up front, 5 messages per account / hour once the account is
known) and answers `202` at once; the account lookup and the sending run afterwards in `AppState::detached`,
so neither existence nor a linked channel shows in the timing (`detached.rs`, a counter tests poll until idle; the
OAuth link notice uses it too; graceful shutdown waits up to 10 s for it after the last request and logs how many
tasks it abandoned). `GET /auth/providers` → `password_reset` is true when a mailer or the bot exists; with the bot
alone the forgot page asks for a username only. A username's link goes to every linked channel (`reset::deliver::linked_channels`:
each email identity by mail, each Telegram identity by bot message; Google identities cannot be messaged); an address
only to itself; a login that is both reaches both accounts (or one account's channels once); a channel whose sender is not configured is skipped. Telegram delivery goes through the
`TelegramBot` trait (`auth/telegram/bot.rs`): `HttpBot` calls `sendMessage` (chat id = the identity subject) with
reqwest — https only, no redirects, 10 s timeout; its URL holds the bot token, so the type has no `Debug` and errors
are stripped of the URL — at most 4 in flight, failures logged; `MemoryBot` in tests. `AppState::telegram` is `None`
without `TELEGRAM_BOT_TOKEN`. The bot may message the user because Telegram login/link asked for
`telegram:bot_access`; a user who blocked the bot gets nothing (logged). Tokens are `email_token` rows with
purpose `password_reset` and the channel's `provider`, the same 15 min / single use / fragment link
(`/auth/password/reset#token=…`) / non-consuming `preview` as the magic link, and `email::target` re-checks that
`(provider, subject)` is still linked to the account (unlinking kills pending reset links). `reset` validates the password and peeks
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
`{APP_BASE_URL}/api/auth/oauth/{provider}/callback`. A new provider is a `Provider`
variant + `identity_provider` enum value + preset; `SubjectSource::IdTokenClaim(name)` reads the account id from
the verified ID token (a string as is, an integer in decimal — Telegram's `id`), `SubjectSource::Userinfo` reads
a field of a userinfo-style endpoint called with the access token, for providers whose web login has no ID token
(Facebook). Google: scope `openid`, subject `sub`.
Telegram (`oauth.telegram.org`, issuer `https://oauth.telegram.org`, RS256, `aud` = the bot id = BotFather's Client
ID): scope `openid profile telegram:bot_access`, subject `id` — Telegram's `sub` is an opaque value unrelated to the
user id the bot needs as chat id, and `id` comes only with `profile` (its names/photo are discarded);
`telegram:bot_access` is what lets the bot send reset links. The ID token `nonce` is checked per preset
(`NonceCheck`): Google `Required`; Telegram `IfPresent` — a wrong one is refused, a missing one accepted, because
Telegram's server-side flow may not echo it and `state` + PKCE + the cookie binding already stop code injection
(to tighten once a real token shows it is echoed). With both Telegram login and the bot configured, startup checks
they are the same bot (token prefix = client id). Telegram's legacy HMAC login widget is not used (no
third-party script on the login page). Facebook (`config::facebook`, Graph v25.0): scope `public_profile` (no App
Review), subject = app-scoped `id` from `GET /me?fields=id` with `Authorization: Bearer` and `appsecret_proof`
(hex HMAC-SHA256 of the access token keyed with the app secret, so a leaked token alone cannot call Graph as this
app); issuers / JWKS / `NonceCheck` are unused (no ID token, so `verify` never runs). PKCE (S256) is sent to
Facebook's dialog and the verifier to its token endpoint (both documented by Facebook); the flow's integrity does
not depend on it — `state` + the signed cookie bind the callback to the starting browser as for Google.

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
key + JWKS, JWKS hit counter / outage switch); Facebook against `tests/common/facebook.rs` (token, `/me`,
`/me/picture`, CDN; verifies PKCE, client secret, bearer and `appsecret_proof`).

**Profile picture import** (`auth/oauth/import.rs`, Facebook only: `Userinfo::picture`). Opt-in per flow
(`import_photo` on `start` / `link`, kept in the signed cookie; dropped for providers without a picture source),
and only for a new account or a link — a login to an already linked account never imports. The provider access
token is never stored, so the import runs inside the callback, and only after the account checks passed
(login: identity lookup + `lock_unbanned`; link: lock, superseded check, identity insert), so a refused flow
downloads nothing: Graph
`/me/picture?redirect=false&width=1280&height=1280` (+ `appsecret_proof`) names the image; the default silhouette
is skipped (`none`). SSRF guard: the URL must be https on the default port, without credentials, on an allowlisted
host or its subdomain (`fbcdn.net`; IP literals never match), and the HTTP client follows no redirects, so no other
host is ever contacted; the body is capped at the upload limit (10 MB, `Content-Length` and streamed) within the
10 s client timeout. The bytes then go through `photos::to_webp` (decode limits, permit semaphore, EXIF stripped)
— nothing unprocessed is stored. Where it lands: link → `photos::add` at once (the usual locked count check:
`full` at 6); a new account → the WebP rides in the `signup_code` grant (`photo=pending`), is copied to the
`signup` grant at `exchange`, and is attached after `signup` commits, whose response then carries the real outcome
(`photo: imported|full|failed`; a failure is logged, never undoes the account); an unfinished sign-up's picture
is deleted with its expired grant by the cleanup job. The outcome is appended to the done page fragment
(`photo=pending|imported|full|none|failed`); an import failure never fails the sign-up or link. Request errors are
logged without their URL (`reqwest::Error::without_url`): Graph URLs carry the `appsecret_proof`, CDN URLs are signed.
A later re-import (`POST /auth/oauth/{provider}/import`, "Importovat z Facebooku" in the photo manager) is a third
cookie mode, `import`: a link flow with `import_photo` forced on, refused up front for a provider without a picture
source (`OAuthService::offers_picture`). `flow::link_identity` serves both — under the user row lock (`FOR UPDATE` for import, so two concurrent imports
cannot both pass the check below) and the same superseded
check, import additionally refuses (`identity_mismatch`) when the account holds another identity of that provider,
so a re-import never quietly adds a second provider account; then `import::fetch_and_attach` as for a link. The
done fragment is `imported=<outcome>`; the SPA hands it to the photo manager (`me.justImportedPhoto`).

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

## Admin impersonation ("act as")

For alpha/beta testing an admin can use the app as another user, typically a test user (`user.is_test`,
created in the admin UI or by `localdate-api admin seed-test-users`). Security model:

- **Off by default.** `ADMIN_IMPERSONATION=true` turns it on (the letsgo deployment does). Off, the issuing
  endpoint answers 404 and `auth::extractor::authorize` refuses every token carrying an `act` claim — so
  disabling = set `false` + restart, which also ends every running impersonation (sockets within one
  `WS_ACCOUNT_RECHECK`, 60 s).
- **Token.** `POST /admin/users/{id}/impersonate` (`admin::impersonate`) issues an access JWT with `sub` =
  target, `act` = admin, 60 min, and no refresh token: when it runs out the admin starts again. A malformed
  `act` invalidates the whole token rather than degrading it to a plain one.
- **Per-request checks.** `authorize` (used by the extractors and by the WebSocket at auth and on every
  re-check) refuses an impersonation token older than its TTL (`iat` + 60 min — the JWT `exp` covers HTTP,
  this covers a socket that keeps the token it was opened with), checks the target as for any token and then
  the actor: it must still exist, be unbanned, be an admin and not have changed or reset its password since the
  token's `iat`; the target must not be an admin. Demoting, banning or re-passwording the admin therefore ends
  the impersonation at the next request (a socket at its next re-check). The admin's **logout does not**: it
  revokes refresh tokens, and the impersonation token has none — the client discards it instead (below).
- **Deny by default.** `AuthUser` (and `AdminUser`, built on it) refuses any impersonation token with
  `403 impersonation_forbidden`. Only handlers that take `ActingUser` accept one: own profile, photos, filter,
  window, location, nearby, areas, waves, matches, messages, the WebSocket (list in
  [api/admin.md](../api/admin.md#users-test-users-and-impersonation-alpha--beta-testing)). Credentials,
  identities, push subscriptions and preferences, blocks and reports, account deletion and admin rights stay
  closed without anyone having to remember a guard, and `tests/impersonation_routes.rs` fails until every new
  route is classified (and checks the refusal for each denied one) and until every function taking
  `ActingUser` is in its explicit `ACTING_FNS` allow-list (so a new method on an allowed path, or a handler
  mounted via `route_service` / `nest` / `on(…)`, cannot slip through).
- **No privilege confusion.** Admins (yourself included) and banned users cannot be targets, and an admin
  endpoint never accepts the token, so it never carries admin rights.
- **Audit.** Issuing writes an `impersonate` row to `admin_audit`. The `ActingUser` extractor (which `AuthUser`
  runs too) writes an `impersonated_request` row (method + matched route template, never the body or concrete
  ids) for **every** request with such a token *before* the handler runs — `GET`s and refused requests
  included; if the insert fails, the request fails. An impersonated WebSocket writes `WS OPEN` when it is
  accepted (no row → the socket is closed `1011`) and `WS CLOSE` when it ends.
- **Known effect.** An impersonated WebSocket registers presence for the target, so the target's Web Push is
  suppressed while it is open (accepted for alpha testing; the target is normally a test user).

Client side (`api/tokens.ts`): the admin's own access and refresh tokens **stay in `localStorage`**, untouched;
the impersonation token, its expiry, the target and the admin's user live in this tab's **`sessionStorage`**
and take precedence for requests, with no refresh token while acting. Stop, expiry, a `401`, the admin's logout
(which clears both storages) and a logout in another tab (a `storage` event on the admin token) all drop that
record. While acting, geolocation sharing and push sync are paused, and a position picker (Leaflet +
OpenStreetMap tiles, area names bound as text, never HTML) places the target by hand.
