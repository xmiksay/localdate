use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use entity::user;
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{ConnectionTrait, EntityTrait, QuerySelect};
use uuid::Uuid;

use super::jwt;
use crate::error::AppError;
use crate::state::AppState;

/// Authenticated caller, taken from `Authorization: Bearer <access token>`.
/// Add it as a handler argument to protect a route. The account row is checked on every request
/// so a ban or account deletion cuts a still-valid access token immediately.
#[derive(Debug, Clone, Copy)]
pub struct AuthUser {
    pub id: Uuid,
    pub is_admin: bool,
    /// The access token's `iat`, for re-checks against a later password change (WS).
    pub issued_at: i64,
}

/// An [`AuthUser`] with `is_admin`; anyone else gets `403 forbidden`.
#[derive(Debug, Clone, Copy)]
pub struct AdminUser {
    pub id: Uuid,
}

/// What an account id currently stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Account {
    Missing,
    Banned,
    /// The token predates the last password reset/change (`credentials_changed_at`).
    Superseded,
    Active {
        is_admin: bool,
    },
}

/// Whether a token issued at `issued_at` (unix seconds) predates a password change. The change is
/// stored in whole seconds, so a token from the very second of the change still counts as newer:
/// the caller's fresh session is issued in that second, and refusing it would need a wait.
pub fn superseded(changed_at: Option<DateTimeWithTimeZone>, issued_at: i64) -> bool {
    changed_at.is_some_and(|c| issued_at < c.timestamp())
}

/// How [`load_user`] locks the row inside the caller's transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowLock {
    None,
    Share,
    Update,
}

async fn load_user(
    db: &impl ConnectionTrait,
    id: Uuid,
    lock: RowLock,
) -> Result<Option<user::Model>, AppError> {
    let query = user::Entity::find_by_id(id);
    let query = match lock {
        RowLock::None => query,
        RowLock::Share => query.lock_shared(),
        RowLock::Update => query.lock_exclusive(),
    };
    Ok(query.one(db).await?)
}

/// `issued_at` is the presented token's, when there is one.
fn account(user: Option<&user::Model>, issued_at: Option<i64>) -> Account {
    match user {
        None => Account::Missing,
        Some(u) if u.banned_at.is_some() => Account::Banned,
        Some(u) if issued_at.is_some_and(|i| superseded(u.credentials_changed_at, i)) => {
            Account::Superseded
        }
        Some(u) => Account::Active {
            is_admin: u.is_admin,
        },
    }
}

/// The row of a usable account: gone → `401`, banned → `403 banned`.
fn usable(user: Option<user::Model>) -> Result<user::Model, AppError> {
    match account(user.as_ref(), None) {
        Account::Active { .. } => user.ok_or(AppError::Unauthorized),
        Account::Banned => Err(AppError::Banned),
        Account::Missing | Account::Superseded => Err(AppError::Unauthorized),
    }
}

/// Unlocked read for a token issued at `issued_at`; enough wherever a stale answer only lasts one
/// request.
pub async fn account_status(
    db: &impl ConnectionTrait,
    id: Uuid,
    issued_at: i64,
) -> Result<Account, AppError> {
    Ok(account(
        load_user(db, id, RowLock::None).await?.as_ref(),
        Some(issued_at),
    ))
}

/// `FOR SHARE` read inside a transaction that creates something only an unbanned user may own
/// (window, wave, refresh token): it waits for a concurrent ban (`FOR UPDATE`) and then sees it,
/// or the ban waits for this transaction and cleans up what it wrote.
pub async fn lock_unbanned(db: &impl ConnectionTrait, id: Uuid) -> Result<(), AppError> {
    usable(load_user(db, id, RowLock::Share).await?).map(|_| ())
}

/// `FOR UPDATE` for a transaction that writes the user row itself (password): exclusive from the
/// start, since two share locks upgrading to write would deadlock. Same refusals as above.
pub async fn lock_user(db: &impl ConnectionTrait, id: Uuid) -> Result<user::Model, AppError> {
    usable(load_user(db, id, RowLock::Update).await?)
}

/// The usable account for `token`: bad token, deleted account or a token older than the last
/// password change → 401, banned → 403.
pub async fn verify_access(
    db: &impl ConnectionTrait,
    secret: &str,
    token: &str,
) -> Result<AuthUser, AppError> {
    let access = jwt::verify(secret, token).ok_or(AppError::Unauthorized)?;
    match account_status(db, access.user, access.issued_at).await? {
        Account::Missing | Account::Superseded => Err(AppError::Unauthorized),
        Account::Banned => Err(AppError::Banned),
        Account::Active { is_admin } => Ok(AuthUser {
            id: access.user,
            is_admin,
            issued_at: access.issued_at,
        }),
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(AppError::Unauthorized)?;
        verify_access(&state.db, &state.config.jwt_secret, token).await
    }
}

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = AuthUser::from_request_parts(parts, state).await?;
        if !user.is_admin {
            return Err(AppError::Forbidden);
        }
        Ok(Self { id: user.id })
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn at(secs: i64) -> Option<DateTimeWithTimeZone> {
        Utc.timestamp_opt(secs, 0)
            .single()
            .map(|t| t.fixed_offset())
    }

    #[test]
    fn tokens_before_the_change_second_are_superseded_from_it_on_not() {
        assert!(superseded(at(1_000), 999));
        assert!(
            !superseded(at(1_000), 1_000),
            "same second: issued with the change"
        );
        assert!(!superseded(at(1_000), 1_001));
        assert!(!superseded(None, 0), "never changed");
    }
}
