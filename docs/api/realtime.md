# API contract — WebSocket `/api/ws`

Part of the [API contract](../api.md); conventions, errors and shared types live there.

## WebSocket `/api/ws`

Client connects, then sends `{ "type": "auth", "token": "<access_token>" }` within 10 s.
Invalid/expired token or timeout → server closes with code `4401` (client refreshes the token before reconnecting).
A password reset or change also closes the account's open sockets with `4401`.
A banned account → `4403`, both at auth time and for open sockets when the ban happens (client logs out,
no reconnect). A server-side failure while checking the account → `1011` (client retries with backoff).
`1012` → the server may have missed events for this socket (its cross-replica listener reconnected); the
client waits its backoff plus a random 0–5 s, reconnects and on `ready` refetches the match list and the
open chat thread. `1001` → the server is shutting down; the client reconnects with its usual backoff.
Server → client events:

```ts
{ type: 'ready' }
{ type: 'message', message: Message }
{ type: 'match', match: MatchSummary }
{ type: 'wave', from_user_id: string }
```

Client reconnects with backoff and re-authenticates with a fresh token.

Liveness: the server sends a WebSocket ping every 25 s and closes a socket that has sent nothing (no
pong, no frame) for 60 s, since an open socket counts as "online" and suppresses push. Browsers answer
pings automatically. The client closes its socket after its page has been hidden for 30 s and
reconnects when it is visible again.
