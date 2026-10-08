//! `oauth_grant`: one-time callback codes and pending sign-up tokens.

use chrono::{DateTime, Duration, Utc};
use entity::{IdentityProvider, OAuthGrantPurpose, oauth_grant};
use sea_orm::sea_query::Expr;
use sea_orm::{ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set};
use uuid::Uuid;

use crate::auth::refresh;
use crate::error::AppError;

/// The SPA exchanges a callback code right after the redirect lands.
pub const CODE_TTL: Duration = Duration::seconds(60);
/// Picking a username may take a few tries; same as a mailed sign-up link.
pub const SIGNUP_TTL: Duration = Duration::minutes(15);

pub struct Issued {
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

/// Stores a fresh grant and returns its plaintext. `binding` must be set for codes (it is the
/// sha256 of the flow state) and `None` for sign-up tokens; `user_id` only for `Login`.
pub async fn issue(
    db: &impl ConnectionTrait,
    purpose: OAuthGrantPurpose,
    provider: IdentityProvider,
    subject: &str,
    user_id: Option<Uuid>,
    binding: Option<String>,
) -> Result<Issued, AppError> {
    let (token, token_hash) = refresh::generate();
    let now = Utc::now();
    let ttl = match purpose {
        OAuthGrantPurpose::Signup => SIGNUP_TTL,
        OAuthGrantPurpose::Login | OAuthGrantPurpose::SignupCode => CODE_TTL,
    };
    let expires_at = now + ttl;
    oauth_grant::ActiveModel {
        id: Set(Uuid::new_v4()),
        token_hash: Set(token_hash),
        purpose: Set(purpose),
        provider: Set(provider),
        subject: Set(subject.to_owned()),
        user_id: Set(user_id),
        binding: Set(binding),
        expires_at: Set(expires_at.fixed_offset()),
        used_at: Set(None),
        created_at: Set(now.fixed_offset()),
    }
    .insert(db)
    .await?;
    Ok(Issued { token, expires_at })
}

/// Marks a live grant of one of `purposes` used and returns it — one conditional UPDATE, so of
/// concurrent callers only one wins. `binding` additionally requires the grant to belong to that
/// browser's flow; a mismatch leaves the grant untouched for its rightful browser.
pub async fn consume(
    db: &impl ConnectionTrait,
    token: &str,
    purposes: &[OAuthGrantPurpose],
    binding: Option<&str>,
) -> Result<oauth_grant::Model, AppError> {
    let mut update = oauth_grant::Entity::update_many()
        .col_expr(
            oauth_grant::Column::UsedAt,
            Expr::current_timestamp().into(),
        )
        .filter(oauth_grant::Column::TokenHash.eq(refresh::hash_token(token)))
        .filter(oauth_grant::Column::UsedAt.is_null())
        .filter(Expr::col(oauth_grant::Column::ExpiresAt).gt(Expr::current_timestamp()))
        .filter(oauth_grant::Column::Purpose.is_in(purposes.iter().copied()));
    if let Some(binding) = binding {
        update = update.filter(oauth_grant::Column::Binding.eq(binding));
    }
    update
        .exec_with_returning(db)
        .await?
        .into_iter()
        .next()
        .ok_or(AppError::InvalidToken)
}
