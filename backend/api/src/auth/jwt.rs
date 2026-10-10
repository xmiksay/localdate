use anyhow::{Context, Result};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const ACCESS_TTL_SECS: i64 = 15 * 60;
/// Impersonation tokens come without a refresh token, so they live longer than a normal one.
pub const IMPERSONATION_TTL_SECS: i64 = 60 * 60;

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,
    iat: i64,
    exp: i64,
    /// RFC 8693 "actor": the admin acting as `sub`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    act: Option<String>,
}

pub fn issue(secret: &str, user_id: Uuid) -> Result<String> {
    issue_at(secret, user_id, None, chrono::Utc::now().timestamp())
}

/// An access token for `target` used by `admin`; returns it with its expiry (unix seconds).
pub fn issue_impersonation(secret: &str, target: Uuid, admin: Uuid) -> Result<(String, i64)> {
    let now = chrono::Utc::now().timestamp();
    let token = issue_at(secret, target, Some(admin), now)?;
    Ok((token, now + IMPERSONATION_TTL_SECS))
}

fn issue_at(secret: &str, user_id: Uuid, actor: Option<Uuid>, now: i64) -> Result<String> {
    let ttl = if actor.is_some() {
        IMPERSONATION_TTL_SECS
    } else {
        ACCESS_TTL_SECS
    };
    let claims = Claims {
        sub: user_id.to_string(),
        iat: now,
        exp: now + ttl,
        act: actor.map(|a| a.to_string()),
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
    /// The admin impersonating `user` (`act` claim).
    pub actor: Option<Uuid>,
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
    let actor = match data.claims.act {
        None => None,
        // A malformed actor must not degrade into a plain token for the target.
        Some(act) => Some(act.parse().ok()?),
    };
    Some(Access {
        user: data.claims.sub.parse().ok()?,
        issued_at: data.claims.iat,
        actor,
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
        let token = issue_at(SECRET, id, None, now).unwrap();
        assert_eq!(
            verify(SECRET, &token),
            Some(Access {
                user: id,
                issued_at: now,
                actor: None,
            })
        );
    }

    #[test]
    fn impersonation_carries_the_actor_and_lives_an_hour() {
        let (target, admin) = (Uuid::new_v4(), Uuid::new_v4());
        let (token, expires) = issue_impersonation(SECRET, target, admin).unwrap();
        let access = verify(SECRET, &token).expect("valid");
        assert_eq!((access.user, access.actor), (target, Some(admin)));
        assert_eq!(expires - access.issued_at, IMPERSONATION_TTL_SECS);
        // Past a normal token's lifetime it still holds.
        let older = chrono::Utc::now().timestamp() - ACCESS_TTL_SECS - 10;
        let token = issue_at(SECRET, target, Some(admin), older).unwrap();
        assert!(verify(SECRET, &token).is_some());
    }

    #[test]
    fn malformed_actor_rejected() {
        let now = chrono::Utc::now().timestamp();
        let claims = Claims {
            sub: Uuid::new_v4().to_string(),
            iat: now,
            exp: now + 60,
            act: Some("not-a-uuid".into()),
        };
        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(SECRET.as_bytes()),
        )
        .unwrap();
        assert_eq!(verify(SECRET, &token), None);
    }

    #[test]
    fn wrong_secret_rejected() {
        let token = issue(SECRET, Uuid::new_v4()).unwrap();
        assert_eq!(verify("another-secret-another-secret-xx", &token), None);
    }

    #[test]
    fn expired_rejected() {
        let long_ago = chrono::Utc::now().timestamp() - ACCESS_TTL_SECS - 10;
        let token = issue_at(SECRET, Uuid::new_v4(), None, long_ago).unwrap();
        assert_eq!(verify(SECRET, &token), None);
    }

    #[test]
    fn garbage_rejected() {
        assert_eq!(verify(SECRET, "not.a.jwt"), None);
    }
}
