# Architecture — Realtime and push

Part of the [architecture](../architecture.md) docs.

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
  route and the tag ([docs/api/push.md](../api/push.md)). TTL 1 h for waves and matches, 24 h for
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
