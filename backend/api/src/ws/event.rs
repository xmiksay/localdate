use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::social::{MatchSummaryDto, MessageDto};

/// Server -> client push events, exactly as in docs/api.md "WebSocket". `Deserialize` is for the
/// cross-replica bridge, which ships them as JSON through Postgres NOTIFY.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    Ready,
    Message {
        message: MessageDto,
    },
    Match {
        #[serde(rename = "match")]
        summary: MatchSummaryDto,
    },
    Wave {
        from_user_id: Uuid,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::social::OtherDto;
    use chrono::Utc;
    use serde_json::json;

    fn message() -> MessageDto {
        MessageDto {
            id: Uuid::nil(),
            match_id: Uuid::nil(),
            sender_id: Uuid::nil(),
            body: "hi".into(),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn ready_and_wave_shapes() {
        let ready = serde_json::to_value(ServerEvent::Ready).expect("json");
        assert_eq!(ready, json!({ "type": "ready" }));
        let from = Uuid::new_v4();
        let wave = serde_json::to_value(ServerEvent::Wave { from_user_id: from }).expect("json");
        assert_eq!(wave, json!({ "type": "wave", "from_user_id": from }));
    }

    #[test]
    fn message_shape() {
        let v = serde_json::to_value(ServerEvent::Message { message: message() }).expect("json");
        assert_eq!(v["type"], "message");
        assert_eq!(v["message"]["body"], "hi");
        assert!(v["message"]["created_at"].is_string());
    }

    #[test]
    fn match_shape_uses_match_key() {
        let summary = MatchSummaryDto {
            match_id: Uuid::nil(),
            created_at: Utc::now(),
            other: OtherDto {
                user_id: Uuid::nil(),
                display_name: "Eva".into(),
                photo_url: None,
            },
            last_message: None,
        };
        let v = serde_json::to_value(ServerEvent::Match { summary }).expect("json");
        assert_eq!(v["type"], "match");
        assert_eq!(v["match"]["other"]["display_name"], "Eva");
        assert!(v["match"]["last_message"].is_null());
        assert!(v["match"]["other"]["photo_url"].is_null());
    }
}
