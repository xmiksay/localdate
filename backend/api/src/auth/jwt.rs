use anyhow::{Context, Result};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const ACCESS_TTL_SECS: i64 = 15 * 60;

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,
    iat: i64,
    exp: i64,
}

pub fn issue(secret: &str, user_id: Uuid) -> Result<String> {
    issue_at(secret, user_id, chrono::Utc::now().timestamp())
}

fn issue_at(secret: &str, user_id: Uuid, now: i64) -> Result<String> {
    let claims = Claims {
        sub: user_id.to_string(),
        iat: now,
        exp: now + ACCESS_TTL_SECS,
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .context("encoding access token")
}

/// What a valid access token says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Access {
    pub user: Uuid,
    /// Unix seconds.
    pub issued_at: i64,
}

/// `None` for any invalid, expired or tampered token.
pub fn verify(secret: &str, token: &str) -> Option<Access> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.leeway = 0;
    validation.set_required_spec_claims(&["exp", "sub", "iat"]);
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .ok()?;
    Some(Access {
        user: data.claims.sub.parse().ok()?,
        issued_at: data.claims.iat,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn roundtrip() {
        let id = Uuid::new_v4();
        let now = chrono::Utc::now().timestamp();
        let token = issue_at(SECRET, id, now).unwrap();
        assert_eq!(
            verify(SECRET, &token),
            Some(Access {
                user: id,
                issued_at: now
            })
        );
    }

    #[test]
    fn wrong_secret_rejected() {
        let token = issue(SECRET, Uuid::new_v4()).unwrap();
        assert_eq!(verify("another-secret-another-secret-xx", &token), None);
    }

    #[test]
    fn expired_rejected() {
        let long_ago = chrono::Utc::now().timestamp() - ACCESS_TTL_SECS - 10;
        let token = issue_at(SECRET, Uuid::new_v4(), long_ago).unwrap();
        assert_eq!(verify(SECRET, &token), None);
    }

    #[test]
    fn garbage_rejected() {
        assert_eq!(verify(SECRET, "not.a.jwt"), None);
    }
}
