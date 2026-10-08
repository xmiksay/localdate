# API contract — Social

Part of the [API contract](../api.md); conventions, errors and shared types live there.

## Waves & matches

| Method & path | Body | 2xx response |
|---|---|---|
| `POST /waves` | `{ to_user_id }` | `200 { matched: boolean, match_id: string \| null }` |
| `GET /waves/incoming` | — | `200 NearbyProfile[]` — senders with a pending (unexpired) wave to me who are still visible, same order as `/nearby` |
| `GET /matches` | — | `200 MatchSummary[]` newest activity first, excluding blocked and banned partners |
| `GET /matches/{id}/messages?before=<message_id>&limit=50` | — | `200 Message[]` newest first, limit ≤ 100 |
| `POST /matches/{id}/messages` | `{ body }` | `201 Message` |

Waving twice at the same person within a window is idempotent (no extra count).
If the target already has a pending wave to the sender, a match is created (idempotent on the pair)
and both waves are deleted.

## Safety

| Method & path | Body | 2xx response |
|---|---|---|
| `GET /blocks` | — | `200 { user_id, display_name, created_at }[]` — users I blocked, newest first |
| `POST /blocks` | `{ user_id }` | `204` (idempotent) |
| `DELETE /blocks/{user_id}` | — | `204` |
| `POST /reports` | `{ user_id, reason: 'spam'\|'harassment'\|'fake'\|'underage'\|'other', note?: string ≤ 1000 }` | `201` empty body — also blocks; blank note stored as null |
