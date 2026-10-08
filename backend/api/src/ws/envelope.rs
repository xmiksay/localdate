//! Wire format of the cross-replica bridge: one JSON NOTIFY payload per hub operation.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::event::ServerEvent;
use super::hub::CloseReason;

/// Postgres NOTIFY channel shared by all replicas.
pub const CHANNEL: &str = "localdate_ws";

/// Postgres refuses NOTIFY payloads of 8000 bytes or more; this leaves headroom.
pub const MAX_PAYLOAD: usize = 7500;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Envelope {
    /// The publishing replica, which delivered locally already and skips its own notifications.
    pub origin: Uuid,
    pub op: Op,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Op {
    Event {
        user: Uuid,
        event: ServerEvent,
    },
    Close {
        user: Uuid,
        reason: CloseReason,
    },
    /// A message event too large to inline (2000 four-byte chars alone are 8000 bytes); the
    /// receiving replica loads the row.
    MessageRef {
        user: Uuid,
        id: Uuid,
    },
    /// A match event too large to inline (its `last_message` can be); the receiver rebuilds the
    /// summary for `user`.
    MatchRef {
        user: Uuid,
        id: Uuid,
    },
    /// Heartbeat: proves the publisher → Postgres → listener round trip is alive.
    Ping,
}

impl Op {
    pub fn is_close(&self) -> bool {
        matches!(self, Op::Close { .. })
    }

    /// For logs.
    pub fn kind(&self) -> &'static str {
        match self {
            Op::Event { .. } => "event",
            Op::Close { .. } => "close",
            Op::MessageRef { .. } => "message_ref",
            Op::MatchRef { .. } => "match_ref",
            Op::Ping => "ping",
        }
    }

    /// The thin form of an oversized operation; `None` when it has none.
    fn by_reference(&self) -> Option<Op> {
        match self {
            Op::Event {
                user,
                event: ServerEvent::Message { message },
            } => Some(Op::MessageRef {
                user: *user,
                id: message.id,
            }),
            Op::Event {
                user,
                event: ServerEvent::Match { summary },
            } => Some(Op::MatchRef {
                user: *user,
                id: summary.match_id,
            }),
            _ => None,
        }
    }
}

/// The NOTIFY payload for `op`, swapped for its reference form when it would not fit.
pub fn encode(origin: Uuid, op: Op) -> Result<String> {
    let json =
        |op: Op| serde_json::to_string(&Envelope { origin, op }).context("encoding envelope");
    let thin = op.by_reference();
    let full = json(op)?;
    if full.len() <= MAX_PAYLOAD {
        return Ok(full);
    }
    let Some(thin) = thin else {
        bail!("{}-byte ws payload has no reference form", full.len());
    };
    json(thin)
}

pub fn decode(payload: &str) -> Result<Envelope> {
    serde_json::from_str(payload).context("decoding ws envelope")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::social::{MatchSummaryDto, MessageDto, OtherDto};
    use chrono::Utc;

    fn message(body: String) -> MessageDto {
        MessageDto {
            id: Uuid::new_v4(),
            match_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            body,
            created_at: Utc::now(),
        }
    }

    // The worst case the API allows: 2000 chars (MAX_BODY_CHARS), each 4 bytes in UTF-8.
    fn longest_body() -> String {
        "😀".repeat(2000)
    }

    #[test]
    fn small_events_travel_inline_and_round_trip() {
        let (origin, user) = (Uuid::new_v4(), Uuid::new_v4());
        let msg = message("hi".into());
        let payload = encode(
            origin,
            Op::Event {
                user,
                event: ServerEvent::Message {
                    message: msg.clone(),
                },
            },
        )
        .expect("encode");
        let env = decode(&payload).expect("decode");
        assert_eq!(env.origin, origin);
        let Op::Event {
            user: got,
            event: ServerEvent::Message { message },
        } = env.op
        else {
            panic!("expected an inline message event, got {:?}", env.op);
        };
        assert_eq!((got, message.id, message.body), (user, msg.id, msg.body));
    }

    #[test]
    fn longest_message_falls_back_to_a_reference_that_fits() {
        let user = Uuid::new_v4();
        let msg = message(longest_body());
        let event = ServerEvent::Message {
            message: msg.clone(),
        };
        let payload = encode(Uuid::new_v4(), Op::Event { user, event }).expect("encode");
        assert!(payload.len() <= MAX_PAYLOAD);
        let op = decode(&payload).expect("decode").op;
        assert!(
            matches!(op, Op::MessageRef { user: u, id } if u == user && id == msg.id),
            "{op:?}"
        );
    }

    #[test]
    fn match_with_a_huge_last_message_falls_back_to_a_reference() {
        let user = Uuid::new_v4();
        let summary = MatchSummaryDto {
            match_id: Uuid::new_v4(),
            created_at: Utc::now(),
            other: OtherDto {
                user_id: Uuid::new_v4(),
                display_name: "Eva".into(),
                photo_url: None,
            },
            last_message: Some(message(longest_body())),
        };
        let id = summary.match_id;
        let event = ServerEvent::Match { summary };
        let payload = encode(Uuid::new_v4(), Op::Event { user, event }).expect("encode");
        let op = decode(&payload).expect("decode").op;
        assert!(
            matches!(op, Op::MatchRef { user: u, id: m } if u == user && m == id),
            "{op:?}"
        );
    }

    #[test]
    fn close_and_ping_round_trip() {
        let user = Uuid::new_v4();
        let reason = CloseReason::Banned;
        let payload = encode(Uuid::nil(), Op::Close { user, reason }).expect("encode");
        assert!(matches!(
            decode(&payload).expect("decode").op,
            Op::Close {
                reason: CloseReason::Banned,
                ..
            }
        ));
        let payload = encode(Uuid::nil(), Op::Ping).expect("encode");
        assert!(matches!(decode(&payload).expect("decode").op, Op::Ping));
        assert!(decode("{not json").is_err());
    }
}
