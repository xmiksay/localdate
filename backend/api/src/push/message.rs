//! What a push says for each event: generic text (no names, no message bodies), route, TTL, collapse key.

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::Sha256;
use uuid::Uuid;

use super::PrefsDto;
use crate::ws::ServerEvent;

const SHORT_TTL: Duration = Duration::from_secs(60 * 60);
const MESSAGE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// RFC 8030 allows at most 32 base64url characters in a `Topic`.
const TOPIC_LEN: usize = 32;

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts keys of any length");
    mac.update(data);
    mac.finalize().into_bytes().into()
}

/// Secret behind message `Topic` values. The push service sees the header, so it gets a keyed hash
/// of the match id: stable per match (so pushes collapse) but not linkable to the id.
#[derive(Clone)]
pub struct TopicKey([u8; 32]);

impl TopicKey {
    /// Derived from a server secret under its own label, so the secret itself is never used as-is.
    pub fn derive(secret: &[u8]) -> Self {
        Self(hmac_sha256(secret, b"localdate push topic key"))
    }

    /// An unrelated key per process (for a notifier without VAPID keys, i.e. tests).
    pub fn random() -> Self {
        let mut key = [0u8; 32];
        key[..16].copy_from_slice(Uuid::new_v4().as_bytes());
        key[16..].copy_from_slice(Uuid::new_v4().as_bytes());
        Self(key)
    }

    pub fn topic(&self, match_id: Uuid) -> String {
        let mut topic = URL_SAFE_NO_PAD.encode(hmac_sha256(&self.0, match_id.as_bytes()));
        topic.truncate(TOPIC_LEN);
        topic
    }
}

/// Notification language, stored per subscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    Cs,
    En,
}

impl Lang {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cs => "cs",
            Self::En => "en",
        }
    }

    /// The DB CHECK allows only `cs`/`en`; anything else falls back to the default.
    pub fn from_db(raw: &str) -> Self {
        if raw == "en" { Self::En } else { Self::Cs }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    Normal,
    High,
}

impl Urgency {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::High => "high",
        }
    }
}

/// One encrypted-to-be push: JSON payload plus the RFC 8030 delivery headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Push {
    pub payload: Vec<u8>,
    pub ttl: Duration,
    /// RFC 8030 `Topic`: the push service keeps only the newest undelivered push per topic.
    pub topic: Option<String>,
    pub urgency: Urgency,
}

/// A pushable event; the match id where there is one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Wave,
    Match(Uuid),
    Message(Uuid),
}

impl Kind {
    pub fn of(event: &ServerEvent) -> Option<Self> {
        match event {
            ServerEvent::Ready => None,
            ServerEvent::Wave { .. } => Some(Self::Wave),
            ServerEvent::Match { summary } => Some(Self::Match(summary.match_id)),
            ServerEvent::Message { message } => Some(Self::Message(message.match_id)),
        }
    }

    pub fn allowed(self, prefs: &PrefsDto) -> bool {
        match self {
            Self::Wave => prefs.waves,
            Self::Match(_) => prefs.matches,
            Self::Message(_) => prefs.messages,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Wave => "wave",
            Self::Match(_) => "match",
            Self::Message(_) => "message",
        }
    }

    fn body(self, lang: Lang) -> &'static str {
        match (self, lang) {
            (Self::Wave, Lang::Cs) => "Někdo ti zamával 👋",
            (Self::Wave, Lang::En) => "Someone waved at you 👋",
            (Self::Match(_), Lang::Cs) => "Máte novou shodu!",
            (Self::Match(_), Lang::En) => "You have a new match!",
            (Self::Message(_), Lang::Cs) => "Nová zpráva",
            (Self::Message(_), Lang::En) => "New message",
        }
    }

    pub fn build(self, lang: Lang, topics: &TopicKey) -> Push {
        let (url, tag) = match self {
            Self::Wave => ("/nearby".to_owned(), "wave".to_owned()),
            Self::Match(id) => (format!("/matches/{id}"), format!("match-{id}")),
            Self::Message(id) => (format!("/matches/{id}"), format!("message-{id}")),
        };
        let payload = json!({
            "type": self.name(),
            "title": "localdate",
            "body": self.body(lang),
            "url": url,
            "tag": tag,
        });
        let (ttl, topic, urgency) = match self {
            Self::Wave => (SHORT_TTL, Some("wave".to_owned()), Urgency::Normal),
            Self::Match(_) => (SHORT_TTL, None, Urgency::High),
            Self::Message(id) => (MESSAGE_TTL, Some(topics.topic(id)), Urgency::High),
        };
        Push {
            payload: payload.to_string().into_bytes(),
            ttl,
            topic,
            urgency,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn payload(push: &Push) -> Value {
        serde_json::from_slice(&push.payload).expect("json payload")
    }

    #[test]
    fn wave_push_is_generic_and_short_lived() {
        let push = Kind::Wave.build(Lang::Cs, &TopicKey::random());
        let p = payload(&push);
        assert_eq!(p["type"], "wave");
        assert_eq!(p["body"], "Někdo ti zamával 👋");
        assert_eq!(p["url"], "/nearby");
        assert_eq!(p["tag"], "wave");
        assert_eq!(push.ttl, SHORT_TTL);
        assert_eq!(push.urgency, Urgency::Normal);
    }

    #[test]
    fn message_push_collapses_per_match_and_lives_a_day() {
        let id = Uuid::new_v4();
        let push = Kind::Message(id).build(Lang::En, &TopicKey::random());
        let p = payload(&push);
        assert_eq!(p["body"], "New message");
        assert_eq!(p["url"], format!("/matches/{id}"));
        assert_eq!(p["tag"], format!("message-{id}"));
        assert_eq!(push.ttl, MESSAGE_TTL);
        let topic = push.topic.expect("topic");
        let base64url = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
        assert!(
            topic.len() == TOPIC_LEN && topic.chars().all(base64url),
            "{topic}"
        );
    }

    #[test]
    fn match_push_links_the_chat() {
        let id = Uuid::new_v4();
        let p = payload(&Kind::Match(id).build(Lang::Cs, &TopicKey::random()));
        assert_eq!(p["body"], "Máte novou shodu!");
        assert_eq!(p["url"], format!("/matches/{id}"));
    }

    #[test]
    fn prefs_gate_each_kind() {
        let prefs = PrefsDto {
            waves: false,
            matches: true,
            messages: false,
        };
        assert!(!Kind::Wave.allowed(&prefs));
        assert!(Kind::Match(Uuid::nil()).allowed(&prefs));
        assert!(!Kind::Message(Uuid::nil()).allowed(&prefs));
        assert_eq!(Kind::of(&ServerEvent::Ready), None);
    }

    #[test]
    fn lang_from_db_defaults_to_czech() {
        assert_eq!(Lang::from_db("en"), Lang::En);
        assert_eq!(Lang::from_db("cs"), Lang::Cs);
        assert_eq!(Lang::from_db("de"), Lang::Cs);
    }

    #[test]
    fn topic_is_a_stable_keyed_hash_never_the_raw_id() {
        let id = Uuid::new_v4();
        let key = TopicKey::derive(b"server secret");
        let topic = key.topic(id);
        assert_eq!(topic, TopicKey::derive(b"server secret").topic(id));
        assert_ne!(topic, key.topic(Uuid::new_v4()));
        assert_ne!(topic, TopicKey::derive(b"other secret").topic(id));
        assert_eq!(topic.len(), TOPIC_LEN);
        let simple = id.simple().to_string();
        assert!(!topic.contains(&simple[..8]) && !topic.contains(&id.to_string()));
    }
}
