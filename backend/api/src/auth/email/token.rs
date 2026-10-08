//! Mailed single-use tokens (`email_token`) and email address normalization.

use chrono::{Duration, Utc};
use entity::{EmailTokenPurpose, IdentityProvider, email_token};
use lettre::Address;
use sea_orm::sea_query::Expr;
use sea_orm::{ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set};
use uuid::Uuid;

use crate::auth::refresh;
use crate::error::AppError;

pub const TOKEN_TTL: Duration = Duration::minutes(15);
const MAX_EMAIL_LEN: usize = 254;
const MAX_LOCAL_LEN: usize = 64;
const MAX_LABEL_LEN: usize = 63;

fn local_ok(local: &str) -> bool {
    (1..=MAX_LOCAL_LEN).contains(&local.len())
        && local
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._+-".contains(&b))
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
}

fn domain_ok(domain: &str) -> bool {
    let labels: Vec<&str> = domain.split('.').collect();
    // A letter in the TLD keeps out IP literals (`1.2.3.4`) and all-numeric hosts.
    labels.len() >= 2
        && labels
            .last()
            .is_some_and(|tld| tld.bytes().any(|b| b.is_ascii_lowercase()))
        && labels.iter().all(|l| {
            (1..=MAX_LABEL_LEN).contains(&l.len())
                && l.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                && !l.starts_with('-')
                && !l.ends_with('-')
        })
}

/// Trim + lowercase, then a strict plain `local@domain` (charset in docs/api.md): no display
/// names, quotes, commas or brackets, so the stored subject is exactly the inbox mailed to and
/// one inbox cannot hide behind many spellings. The result is what gets stored and sent to.
pub fn normalize_email(raw: &str) -> Result<Address, AppError> {
    let email = raw.trim().to_lowercase();
    let invalid = || AppError::validation("email must be a plain address like name@example.com");
    if email.len() > MAX_EMAIL_LEN {
        return Err(invalid());
    }
    let (local, domain) = email.split_once('@').ok_or_else(invalid)?;
    if !local_ok(local) || !domain_ok(domain) {
        return Err(invalid());
    }
    Address::new(local, domain).map_err(|_| invalid())
}

/// Stores a fresh token for an email address and returns its plaintext (only ever put into the email).
pub async fn issue(
    db: &impl ConnectionTrait,
    purpose: EmailTokenPurpose,
    user_id: Option<Uuid>,
    email: &str,
) -> Result<String, AppError> {
    issue_for(db, purpose, user_id, IdentityProvider::Email, email).await
}

/// [`issue`] for the identity `(provider, subject)`, e.g. a Telegram account.
pub async fn issue_for(
    db: &impl ConnectionTrait,
    purpose: EmailTokenPurpose,
    user_id: Option<Uuid>,
    provider: IdentityProvider,
    subject: &str,
) -> Result<String, AppError> {
    let (token, token_hash) = refresh::generate();
    let now = Utc::now();
    email_token::ActiveModel {
        id: Set(Uuid::new_v4()),
        token_hash: Set(token_hash),
        purpose: Set(purpose),
        user_id: Set(user_id),
        email: Set(subject.to_owned()),
        provider: Set(provider),
        expires_at: Set((now + TOKEN_TTL).fixed_offset()),
        used_at: Set(None),
        created_at: Set(now.fixed_offset()),
    }
    .insert(db)
    .await?;
    Ok(token)
}

fn live(hash: String) -> sea_orm::Condition {
    sea_orm::Condition::all()
        .add(email_token::Column::TokenHash.eq(hash))
        .add(email_token::Column::UsedAt.is_null())
        .add(Expr::col(email_token::Column::ExpiresAt).gt(Expr::current_timestamp()))
}

/// The unused, unexpired token row, without consuming it.
pub async fn peek(
    db: &impl ConnectionTrait,
    token: &str,
) -> Result<Option<email_token::Model>, AppError> {
    Ok(email_token::Entity::find()
        .filter(live(refresh::hash_token(token)))
        .one(db)
        .await?)
}

/// Marks the token used and returns it — one conditional UPDATE, so of concurrent callers only
/// one wins. `owner` additionally requires the token to belong to that user (link confirm).
pub async fn consume(
    db: &impl ConnectionTrait,
    token: &str,
    purpose: EmailTokenPurpose,
    owner: Option<Uuid>,
) -> Result<email_token::Model, AppError> {
    let mut update = email_token::Entity::update_many()
        .col_expr(
            email_token::Column::UsedAt,
            Expr::current_timestamp().into(),
        )
        .filter(live(refresh::hash_token(token)))
        .filter(email_token::Column::Purpose.eq(purpose));
    if let Some(user) = owner {
        update = update.filter(email_token::Column::UserId.eq(user));
    }
    update
        .exec_with_returning(db)
        .await?
        .into_iter()
        .next()
        .ok_or(AppError::InvalidToken)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn norm(raw: &str) -> Option<String> {
        normalize_email(raw).ok().map(|a| a.to_string())
    }

    #[test]
    fn email_is_trimmed_and_lowercased() {
        assert_eq!(
            norm("  Eva.Novak@Example.CZ ").as_deref(),
            Some("eva.novak@example.cz")
        );
        assert!(norm("a+tag_1.x-y@sub.example-mail.com").is_some());
        assert!(norm("a@b.c1").is_some(), "a TLD only needs one letter");
    }

    #[test]
    fn email_rejects_anything_but_a_plain_address() {
        let long = format!("{}@example.cz", "a".repeat(65));
        let longer = format!("a@{}.cz", "b".repeat(250));
        for bad in [
            "",
            "plain",
            "@example.cz",
            "a@",
            "a@localhost",
            "a@b@c.cz",
            "a b@c.cz",
            "a@.cz",
            "a@c.cz.",
            "a@c..cz",
            "a@c\n.cz",
            "n<victim@b.cz>",
            "x <a@b.cz>",
            "\"a\"@b.cz",
            "a,b@c.cz",
            "a@b.cz,c@d.cz",
            "a@-b.cz",
            "a@b-.cz",
            ".a@b.cz",
            "a.@b.cz",
            "a..b@c.cz",
            "a(c)@b.cz",
            "a@[1.2.3.4]",
            "a%b@c.cz",
            "a@1.2.3.4",
            "a@b.123",
            "ä@b.cz",
            &long,
            &longer,
        ] {
            assert!(normalize_email(bad).is_err(), "{bad:?} should fail");
        }
    }
}
