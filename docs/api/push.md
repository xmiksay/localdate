# API contract — Push notifications (Web Push)

Part of the [API contract](../api.md); conventions, errors and shared types live there.

## Push notifications (Web Push)

Optional: without VAPID keys on the server (`GET /push/config` → `enabled: false`) nothing is sent and
subscribing is `409 push_disabled`; preferences still work.

```ts
interface PushConfig { enabled: boolean; public_key: string | null /* VAPID key, base64url: applicationServerKey */ }
interface PushSubscriptionBody {
  endpoint: string                       // PushSubscription.endpoint
  keys: { p256dh: string; auth: string } // base64url, as PushSubscription.toJSON() gives them
  lang?: 'cs' | 'en'                     // notification language for this device, default 'cs'
}
interface PushPrefs { waves: boolean; matches: boolean; messages: boolean }
```

| Method & path | Body | 2xx response |
|---|---|---|
| `GET /push/config` | — | `200 PushConfig` (no auth) |
| `POST /me/push/subscriptions` | `PushSubscriptionBody` | `204` — upsert by `endpoint`; an endpoint registered by another account moves to the caller. At most 10 per user (the oldest beyond that are dropped) |
| `DELETE /me/push/subscriptions` | `{ endpoint }` | `204` (idempotent; only the caller's own) |
| `GET /me/push/prefs` | — | `200 PushPrefs` (all `true` if never saved) |
| `PATCH /me/push/prefs` | `Partial<PushPrefs>` | `200 PushPrefs` |

Validation (`400 validation`): `endpoint` ≤ 1024 chars, `https`, default port, no credentials, and its host
must be a known push service — `fcm.googleapis.com`, `updates.push.services.mozilla.com`,
`web.push.apple.com`, `*.push.apple.com`, `*.notify.windows.com` (an allowlist, so the server never
POSTs to an address a client picked); `p256dh` a P-256 point (65 bytes), `auth` 16 bytes.

A push goes out for the same events as the WebSocket (`wave`, `match`, `message`) — never to the user
who caused it — only when the recipient has that preference on **and no open WebSocket on any replica**.
Messages coalesce: at most one push per match per 60 s per recipient. Subscriptions the push service
reports gone (404/410) are deleted, as are those it rejects (any other 4xx but 429) 3 times in a row.
The client calls `DELETE /me/push/subscriptions` on an explicit logout only. When the session is lost
any other way it keeps its browser subscription, and the next login re-posts it (moving it to that
account). Payload the service worker receives (text is generic on purpose —
no names, no message bodies):

```ts
interface PushPayload {
  type: 'wave' | 'match' | 'message'
  title: string; body: string            // already in the subscription's `lang`
  url: string                            // in-app route: '/nearby' | '/matches/<match_id>'
  tag: string                            // 'wave' | 'match-<match_id>' | 'message-<match_id>' (replaces older ones)
}
```
