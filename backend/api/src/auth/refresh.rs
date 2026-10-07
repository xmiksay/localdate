use anyhow::Context;
use argon2::password_hash::rand_core::{OsRng, RngCore};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{Duration, Utc};
use entity::refresh_token;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    Set, TransactionTrait,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::AppError;

const REFRESH_TTL_DAYS: i64 = 30;

/// Fresh opaque token (32 random bytes, base64url) and its stored sha256 hex hash.
pub fn generate() -> (String, String) {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let token = URL_SAFE_NO_PAD.encode(bytes);
    let hash = hash_token(&token);
    (token, hash)
}

pub fn hash_token(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Stores a new token in `family_id` and returns the plaintext.
pub async fn issue(
    db: &impl ConnectionTrait,
    user_id: Uuid,
    family_id: Uuid,
) -> Result<String, AppError> {
    let (token, token_hash) = generate();
    let now = Utc::now();
    refresh_token::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(user_id),
        token_hash: Set(token_hash),
        family_id: Set(family_id),
        expires_at: Set((now + Duration::days(REFRESH_TTL_DAYS)).fixed_offset()),
        revoked_at: Set(None),
        created_at: Set(now.fixed_offset()),
    }
    .insert(db)
    .await?;
    Ok(token)
}

async fn revoke_family(db: &impl ConnectionTrait, family_id: Uuid) -> Result<(), AppError> {
    refresh_token::Entity::update_many()
        .col_expr(
            refresh_token::Column::RevokedAt,
            Expr::current_timestamp().into(),
        )
        .filter(refresh_token::Column::FamilyId.eq(family_id))
        .filter(refresh_token::Column::RevokedAt.is_null())
        .exec(db)
        .await?;
    Ok(())
}

/// Consumes `presented` and returns `(user_id, replacement token)`.
/// A revoked token being presented again means it leaked: the whole family is revoked.
pub async fn rotate(db: &DatabaseConnection, presented: &str) -> Result<(Uuid, String), AppError> {
    let row = refresh_token::Entity::find()
        .filter(refresh_token::Column::TokenHash.eq(hash_token(presented)))
        .one(db)
        .await?
        .ok_or(AppError::InvalidRefreshToken)?;

    if row.revoked_at.is_some() {
        revoke_family(db, row.family_id).await?;
        return Err(AppError::InvalidRefreshToken);
    }
    if row.expires_at <= Utc::now() {
        return Err(AppError::InvalidRefreshToken);
    }

    let txn = db.begin().await.context("begin refresh txn")?;
    // Conditional update makes concurrent use of one token race-safe: only one caller wins.
    let claimed = refresh_token::Entity::update_many()
        .col_expr(
            refresh_token::Column::RevokedAt,
            Expr::current_timestamp().into(),
        )
        .filter(refresh_token::Column::Id.eq(row.id))
        .filter(refresh_token::Column::RevokedAt.is_null())
        .exec(&txn)
        .await?
        .rows_affected;
    if claimed == 0 {
        txn.rollback().await.context("rollback refresh txn")?;
        revoke_family(db, row.family_id).await?;
        return Err(AppError::InvalidRefreshToken);
    }
    let next = issue(&txn, row.user_id, row.family_id).await?;
    txn.commit().await.context("commit refresh txn")?;
    Ok((row.user_id, next))
}

/// Logout: revokes the presented token's whole family. Unknown tokens are ignored.
pub async fn revoke_by_token(db: &DatabaseConnection, presented: &str) -> Result<(), AppError> {
    let row = refresh_token::Entity::find()
        .filter(refresh_token::Column::TokenHash.eq(hash_token(presented)))
        .one(db)
        .await?;
    if let Some(row) = row {
        revoke_family(db, row.family_id).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_43_char_base64url_and_unique() {
        let (a, _) = generate();
        let (b, _) = generate();
        assert_eq!(a.len(), 43);
        assert!(
            a.bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        );
        assert_ne!(a, b);
    }

    #[test]
    fn hash_is_stable_sha256_hex() {
        assert_eq!(
            hash_token("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let (token, hash) = generate();
        assert_eq!(hash_token(&token), hash);
        assert_ne!(token, hash);
    }
}
