# API contract — Me / profile

Part of the [API contract](../api.md); conventions, errors and shared types live there.

## Me / profile

| Method & path | Body | 2xx response |
|---|---|---|
| `GET /me` | — | `{ user: User, profile: Profile \| null, filter: Filter \| null, is_admin: boolean }` |
| `DELETE /me` | — | `204` — hard delete everything incl. photo files (a file the storage fails to delete is logged, the account is deleted anyway) |
| `PUT /me/profile` | `{ display_name, birth_date, gender, bio, interest_ids: number[] }` | `200 Profile` |
| `GET /interests` | — | `200 Interest[]` |
| `POST /me/photos` | multipart field `file` | `201 Photo` (appended at last position); limits: 10 MiB, ≤ 10 000 px per edge, ≤ 32 Mi px — the client first re-encodes oversized or non-JPEG/PNG/WebP images to a ≤ 2048 px JPEG |
| `DELETE /me/photos/{id}` | — | `204` (positions compacted) |
| `PUT /me/photos/order` | `{ photo_ids: string[] }` (must be exactly the user's photos) | `200 Photo[]` |
| `GET /me/filter` | — | `200 Filter` (defaults if never saved: 2000 m, [], 18, 99, [date, meet], 60) |
| `PUT /me/filter` | `Filter` | `200 Filter` |
| `PUT /me/password` | `{ current_password?: string, new_password }` | `200 Tokens` — a fresh session; every other one is logged out |
| `GET /me/identities` | — | `200 { has_password: boolean, identities: Identity[] }` (oldest first) |
| `POST /me/identities/email` | `{ email, lang?: MailLang }` | `202` (empty) — always |
| `POST /me/identities/email/confirm` | `{ token }` | `201 Identity` |
| `DELETE /me/identities/{id}` | — | `204`; `409 last_login_method` if it is the only identity of a passwordless account; another user's / unknown id → `404` |

Google and Facebook are linked through `POST /auth/oauth/{provider}/link` ([Auth](auth.md#sign-in-with-google-or-telegram-oauth--openid-connect)) and unlinked here like any identity.

Linking: `POST /me/identities/email` mails a link token bound to the caller to that address — or, if the
address is already linked to any account (the caller's included), a short notice instead, so the answer
never reveals whose it is. Same validation, limits, fragment links, `preview` and `503 email_disabled` as
`/auth/email/start`. A logged-out click keeps the token in `sessionStorage` across the login, never in a query.
`confirm` must come from the account that requested the link: another account's token, or an expired / used
one → `400 invalid_token` (a wrong account does not consume it). If the address got linked elsewhere in the
meantime → `400 invalid_token`.

Password change: `new_password` follows the register policy (`400 validation`). An account that has a password
must send the right `current_password` (missing → `400 validation`, wrong → `401 invalid_credentials`); an
account without one (created by email) sets its first password without it (`current_password` ignored), after
which `has_password` is `true`. Every refresh token of the account is revoked — the access JWT does not say
which session made the call, so instead of guessing, the response carries new `Tokens` for the caller, who
replaces both tokens; other devices fall back to login. Like a reset it voids the account's unused mailed tokens
and refuses every older access token, closing the WebSockets with `4401` — the caller's own too, which
reconnects with the returned access token — and deletes the account's push subscriptions; the caller's client
re-registers its own device with the new session (the usual resync). Precision is whole seconds: a token issued in the same second as the
change still counts as newer (that is how the returned one stays valid), so only an access token minted earlier
within that very second outlives it. A client request that gets `401` with the old token while the answer is still
on its way retries with the new tokens instead of refreshing. A concurrent change between the check and the write → `401 invalid_credentials`.
Known trade-off: setting a *first* password needs only a valid access token (there is no old password to ask
for), so a stolen session of an email-only account can add one; the same session could already link an address.

Validation: display_name 1–40, bio ≤ 500, interest_ids ≤ 10 and existing, age_min ≤ age_max,
reasons non-empty.
