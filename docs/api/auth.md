# API contract — Auth

Part of the [API contract](../api.md); conventions, errors and shared types live there.

## Auth

| Method & path | Body | 2xx response |
|---|---|---|
| `POST /auth/register` | `{ username, password }` | `201 Tokens` |
| `POST /auth/login` | `{ username, password }` | `200 Tokens` |
| `POST /auth/refresh` | `{ refresh_token }` | `200 Tokens` (old token revoked) |
| `POST /auth/logout` | `{ refresh_token }` | `204`; also clears the OAuth flow cookie (`ld_oauth`, `Path=/api/auth/oauth`) |

Username: trimmed, lowercased, `[a-z0-9_]{3,32}`. Password: 10–128 chars.
An account created by email (or a later OAuth provider) has **no password**: password login for it
answers the same `401 invalid_credentials` as a wrong password (same argon2 timing).

### Login providers and email magic link

| Method & path | Body | 2xx response |
|---|---|---|
| `GET /auth/providers` | — | `200 { email: boolean, google: boolean, telegram: boolean, facebook: boolean, password_reset: boolean }` — which login methods this server offers; `password_reset`: a reset link can be sent at all (a mailer or the Telegram bot is configured). With the bot only, reset works by username alone |
| `POST /auth/email/start` | `{ email, lang?: MailLang }` | `202` (empty) — always, whether or not the address has an account |
| `POST /auth/email/preview` | `{ token }` | `200 EmailPreview` — never consumes the token |
| `POST /auth/email/verify` | `{ token }` | `200 Tokens` — login tokens only, consumed |
| `POST /auth/email/signup` | `{ token, username }` | `201 Tokens` (token consumed, account + verified email identity created) |

**Email address** (identical rule on the client): trimmed + lowercased, ≤ 254 chars, exactly one `@`;
local part 1–64 of `a-z 0-9 . _ + -`, no leading/trailing/double dot; domain ≥ 2 dot-separated labels of
`a-z 0-9 -` (1–63 each, no leading/trailing `-`), the last (TLD) containing a letter. No display names, quotes, commas, angle brackets or
whitespace — else `400 validation`. The validated address is what is stored and mailed to, never the raw input.

`start` sends a **login link** when an email identity with that address exists, otherwise a **sign-up link** —
the response never tells which. Mails go out after the response (sending failures are only logged), so the
answer is always `202`. Limits (shared with `POST /me/identities/email`), over them still `202` and nothing sent:
3 mails per (address, client IP) per 15 min, and 10 per address per hour from all IPs together.

Links carry the token in the **URL fragment**, so it never reaches server or ingress logs:
`{APP_BASE_URL}/auth/email#token=…` (login / sign-up) and `{APP_BASE_URL}/auth/email/link#token=…` (linking,
below). Login and link mails name the account (`pro účet <username>`). Tokens: 32 random bytes base64url,
stored as sha256, valid 15 min, single use.

Nothing is consumed by opening a link: the page calls `preview` and shows an explicit button
("Přihlásit se jako <username>", "Propojit <email> s účtem <username>") before `verify` / `confirm` /
`signup`, so link scanners and prefetchers cannot spend the token. Deliberately **no browser binding**
(nonce cookie from the requesting browser): requesting on a laptop and clicking on the phone must keep working;
the 15-minute single-use token and the explicit confirmation are the protection.

`preview` / `verify` → `400 invalid_token` for unknown, expired or used tokens, a login token whose identity was
removed since or whose account is gone, and a sign-up token whose address has meanwhile become an account.
`verify` also refuses sign-up and link tokens (`400 invalid_token`); a banned account → `403 banned` (not
consumed — consumption, account check and session creation are one transaction).
`signup` validates the username like register (`400 validation`, `409 username_taken` — the token
stays usable for another name). The new account then goes through onboarding like a registered one
(birth date, 18+ check). A second sign-up for an address that already got an account → `400 invalid_token`.

### Password reset

| Method & path | Body | 2xx response |
|---|---|---|
| `POST /auth/password/forgot` | `{ login, lang?: MailLang }` — `login` is a username or an email address | `202` (empty) — always |
| `POST /auth/password/reset/preview` | `{ token }` | `200 { username: string }` — never consumes the token |
| `POST /auth/password/reset` | `{ token, new_password }` | `204` |

`login` containing `@` is validated as an email address, anything else as a username (`400 validation` for a
malformed one — that says nothing about whether it exists). The answer is `202` whether or not there is such an
account, and it is sent **before** the account is even looked up, so its timing reveals nothing either. Then:
an email address that is a linked email identity gets a reset link; a username gets one at every email address
linked to that account **and** by Telegram message to every Telegram account linked to it (an address only ever
reaches that address). Accounts with neither get nothing — the UI says "if the account has a linked email or
Telegram…". A channel the server has not configured is skipped (no mailer; no `TELEGRAM_BOT_TOKEN`), and Google
identities are never a channel. `503 email_disabled` for an address while the server has no mailer, and for a
username while it has neither a mailer nor a Telegram bot. Limits, a
budget of their own (asking for resets never uses up an inbox's magic-link mails, nor the other way round); over
them still `202` and nothing sent: 3 requests per (username or address as typed, client IP) per 15 min, and 5
reset messages (mails and Telegram messages together) per account per hour however they were asked for.

The link is `{APP_BASE_URL}/auth/password/reset#token=…` (fragment, as above; the same link in a Telegram
message, sent by the bot to the chat with the user — allowed by the `telegram:bot_access` scope granted at Telegram
login/link; a user who blocked the bot gets nothing), names the account
(`pro účet <username>`), and its token (purpose `password_reset`) is valid 15 min, single use. Opening it spends
nothing: the page calls `preview` and shows "Nastavit nové heslo pro <username>". `reset` checks the password
policy of register (`400 validation`, token unspent), then in one transaction consumes the token, sets the new
password, **ends every session** of the account — refresh tokens revoked, and access tokens issued before
it refused from then on (`401 unauthorized`, WebSocket `4401`), every push subscription of the account deleted
(a possibly stolen device stops getting notifications) — and voids every other unused mailed token of the account (login, link, reset) — a link sent
before must not undo or bypass the new password. After the commit the account's WebSockets are closed with
`4401`. It returns no tokens — the user logs in normally afterwards. Banned account → `403 banned`, token
unspent. `preview` / `reset` → `400 invalid_token` for unknown, expired, used or other-purpose tokens, and when
the address (or Telegram account) was unlinked from the account since the message went out. Reset tokens are refused by
`/auth/email/preview` and `/auth/email/verify` (`400 invalid_token`), login tokens by the reset endpoints.

### Sign in with Google or Telegram (OAuth / OpenID Connect)

Server-side authorization code flow with PKCE (S256), a `state` and a `nonce`. `{provider}` is an
`OAuthProvider` (`google`, `telegram`); an unknown one → `404`. Each is offered only when the server has its
client credentials: `GOOGLE_CLIENT_ID` + `GOOGLE_CLIENT_SECRET`, `TELEGRAM_CLIENT_ID` + `TELEGRAM_CLIENT_SECRET`
(`GET /auth/providers` → `google`, `telegram`).

| Provider | scope | identity subject |
|---|---|---|
| `google` | `openid` | `sub` |
| `telegram` | `openid profile telegram:bot_access` | `id` (numeric Telegram user id, decimal); Telegram's `sub` is a different, opaque value and is not used. `profile` is requested only because it carries `id` — names, username and photo are discarded; `telegram:bot_access` lets the bot send password-reset links |

| Method & path | Body | 2xx response |
|---|---|---|
| `GET /auth/oauth/{provider}/start?redirect=<path>&import_photo=1` | — | `302` to the provider (login or sign-up); sets the flow cookie. `import_photo` (`1`/`true`, optional): see Facebook below |
| `POST /auth/oauth/{provider}/link` (auth) | `{ redirect?: string, lang?: MailLang, import_photo?: boolean }` | `200 { url: string }` — navigate the browser there; sets the flow cookie |
| `GET /auth/oauth/{provider}/callback?code&state` | — (the provider redirects here) | `302 /auth/oauth/done#…` |
| `POST /auth/oauth/exchange` | `{ code }` | `200 OAuthExchange` — needs the flow cookie, code consumed |
| `POST /auth/oauth/signup` | `{ token, username }` | `201 Tokens & { photo?: 'imported' \| 'full' \| 'failed' }` (token consumed, account + identity created; `photo` only when the sign-up holds an imported picture, see Facebook below) |

The SPA starts a login with a plain navigation to `start` (no fetch); linking needs the bearer token, so
it is a `POST` returning the provider URL. Both set an **HttpOnly, SameSite=Lax** cookie `ld_oauth`
(`Path=/api/auth/oauth`, 10 min, `Secure` unless `APP_BASE_URL` is `http://`), HMAC-signed by the server:
provider, mode (login / link), `state`, PKCE verifier, `nonce`, the safe redirect and — link only — the
caller's user id. The provider's account id (table above) is the identity subject; nothing else from the ID token
is stored, and an email Google may know is neither requested nor used to match accounts. An ID token without the
subject claim (e.g. Telegram without `id`) → `#error=oauth_failed`.

`redirect` must be an in-app path: starts with a single `/`, ≤ 512 printable ASCII chars, no `\`; anything
else is dropped (→ none). The callback **never puts tokens in a URL**; it redirects to the SPA route
`/auth/oauth/done` with one of these fragments (`redirect` appended as `&redirect=<percent-encoded path>`
when one was given):

| Fragment | Meaning |
|---|---|
| `#code=<one-time code>` | login or sign-up: `POST /auth/oauth/exchange { code }` within 60 s, from the same browser |
| `#linked=<provider>` | link mode: the identity was added to the account that started the flow |
| `&photo=<PhotoImportOutcome>` | appended to `#linked=…`, or to `#code=…` of a **new** account, only when a picture import was asked for and the provider offers one (Facebook below); never with `#error=`, never for a login to an existing account |
| `#error=<code>` | `cancelled` (denied at the provider), `invalid_state` (cookie missing / expired / tampered, `state` mismatch, wrong provider), `oauth_failed` (code exchange or ID token check failed), `provider_disabled`, `banned`, `identity_taken` (link: that provider account belongs to another user), `unauthorized` (link: the account is gone, or its password was reset / changed after the access token that started the flow — that session was ended, so its link flow dies with it), `rate_limited`, `internal` |

ID tokens are checked against the provider's JWKS (RS256, cached, refetched for an unknown `kid`): `iss`,
`aud` = client id, `exp`, and `nonce` = the cookie's — required for Google; for Telegram checked when present and accepted when absent (its server-side flow may not echo it; `state`, PKCE and the cookie binding still stop code injection). One-time codes (and sign-up tokens) are 32 random
bytes base64url, stored as sha256; codes live 60 s and are bound to the flow cookie's `state`, so a code
lifted from another browser is `400 invalid_token` and so is reuse. `exchange` clears the cookie. An `azp`
claim, when present, must equal the client id. When the signing keys cannot be refetched after their
cache time, the previous keys keep being used (logged).

A successful link (not a repeated one) mails a notice — "<Provider> sign-in was linked to your account",
no link, no token, in `lang` — to every email address linked to the account, after the redirect; nothing
when email is off or no address is linked.

`exchange` → `{ session }` when the provider account is linked to an account (re-checked after the ban check, so a
ban since the callback is `403 banned`,
not consumed — consumption, ban check and session are one transaction), or `{ signup }` with a sign-up token
(15 min) when it is new: the SPA asks for a username and calls `signup`, which validates it like register
(`400 validation`, `409 username_taken` — the token stays usable). A provider account linked elsewhere in the
meantime → `400 invalid_token`. The new account has no password and goes through onboarding like any other.

### Facebook login and profile picture import

Facebook (`{provider}` = `facebook`, offered when the server has `FACEBOOK_APP_ID` + `FACEBOOK_APP_SECRET`) uses
the same endpoints, cookie, fragments and errors. It is plain OAuth 2.0 (no ID token): scope `public_profile`
only, the identity subject is the **app-scoped user id** from Graph `/me?fields=id`, called with the access
token and its `appsecret_proof`. No email or other profile data is requested or stored.

`import_photo` (on `start` / `link`) asks to add the Facebook profile picture to the account's photos — for a
**new account** (sign-up) or a **link** only. A login to an account the Facebook account is already linked to
never imports (the flag is ignored, no `photo=`), and so does every provider without a picture (Google). The
picture is fetched inside the callback — the only time the server holds the Facebook access token, which is never
stored — and only after the account checks passed (a banned, taken or superseded flow downloads nothing):

| `photo=` | Meaning |
|---|---|
| `pending` | sign-up (`#code` → `{ signup }`): the picture is fetched and kept with the sign-up token; the outcome comes in the `POST /auth/oauth/signup` response as `photo: 'imported' \| 'full' \| 'failed'` |
| `imported` | link: added as the account's last photo |
| `full` | link: the account already has the maximum of 6 photos; nothing added |
| `none` | link or sign-up: the Facebook account only has the default silhouette |
| `failed` | link or sign-up: lookup, download or decoding failed (or the image is not on Facebook's CDN, is over 10 MB, or is not an image) |

A failed import never fails the sign-up or link itself; with `none` / `failed` the signup response has no `photo`. The picture runs through the normal upload pipeline
(decode limits, EXIF stripped, WebP, ≤ 1280 px) and then behaves like any uploaded photo (`DELETE /me/photos/{id}`).
