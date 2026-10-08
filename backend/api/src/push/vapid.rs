//! VAPID (RFC 8292) server identity: parsing the configured key pair and generating new ones.

use anyhow::{Context, Result, anyhow, bail, ensure};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use web_push_native::jwt_simple::algorithms::{ECDSAP256PublicKeyLike, ES256KeyPair};

use super::message::TopicKey;
use crate::config::VapidConfig;

pub struct Vapid {
    pub key_pair: ES256KeyPair,
    /// Uncompressed public key, base64url: the browser's `applicationServerKey`.
    pub public_key: String,
    /// `mailto:` or `https:` contact the push services may use.
    pub subject: String,
}

/// Browsers hand out base64url without padding; tolerate padding and surrounding whitespace.
pub fn decode_b64url(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    URL_SAFE_NO_PAD.decode(s.trim().trim_end_matches('='))
}

fn public_key_of(key_pair: &ES256KeyPair) -> String {
    URL_SAFE_NO_PAD.encode(key_pair.public_key().public_key().to_bytes_uncompressed())
}

/// A fresh `(public, private)` key pair, both base64url.
pub fn generate() -> (String, String) {
    let key_pair = ES256KeyPair::generate();
    (
        public_key_of(&key_pair),
        URL_SAFE_NO_PAD.encode(key_pair.to_bytes()),
    )
}

impl Vapid {
    /// Topic hashes are keyed off the private key, so they survive restarts like the key does.
    pub fn topic_key(&self) -> TopicKey {
        TopicKey::derive(&self.key_pair.to_bytes())
    }

    /// Refuses a public key that does not belong to the private key: browsers would subscribe with
    /// it and every push would then be rejected by the push service.
    pub fn from_config(config: &VapidConfig) -> Result<Self> {
        let raw =
            decode_b64url(&config.private_key).context("VAPID_PRIVATE_KEY is not base64url")?;
        let key_pair = ES256KeyPair::from_bytes(&raw)
            .map_err(|e| anyhow!("VAPID_PRIVATE_KEY is not a P-256 private key: {e}"))?;
        let public_key = public_key_of(&key_pair);
        ensure!(
            config.public_key.trim().trim_end_matches('=') == public_key,
            "VAPID_PUBLIC_KEY does not match VAPID_PRIVATE_KEY"
        );
        let subject = config.subject.trim().to_owned();
        if !(subject.starts_with("mailto:") || subject.starts_with("https://")) {
            bail!("VAPID_SUBJECT must be a mailto: or https:// URL");
        }
        Ok(Self {
            key_pair,
            public_key,
            subject,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(public_key: &str, private_key: &str, subject: &str) -> VapidConfig {
        VapidConfig {
            public_key: public_key.into(),
            private_key: private_key.into(),
            subject: subject.into(),
        }
    }

    #[test]
    fn generated_keys_round_trip() {
        let (public, private) = generate();
        assert_eq!(decode_b64url(&public).map(|b| b.len()), Ok(65));
        assert_eq!(decode_b64url(&private).map(|b| b.len()), Ok(32));
        let vapid = Vapid::from_config(&config(&public, &private, "mailto:ops@example.com"))
            .expect("generated keys parse");
        assert_eq!(vapid.public_key, public);
    }

    #[test]
    fn mismatched_or_bad_keys_and_subjects_are_refused() {
        let (public, private) = generate();
        let (other_public, _) = generate();
        let subject = "https://localdate.example";
        assert!(Vapid::from_config(&config(&public, &private, subject)).is_ok());
        assert!(Vapid::from_config(&config(&other_public, &private, subject)).is_err());
        assert!(Vapid::from_config(&config(&public, "not-a-key", subject)).is_err());
        assert!(Vapid::from_config(&config(&public, &private, "ops@example.com")).is_err());
        assert!(Vapid::from_config(&config(&public, &private, "http://x.example")).is_err());
    }
}
