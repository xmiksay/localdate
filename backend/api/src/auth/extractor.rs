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
    Active { is_admin: bool },
}

async fn load_account(
    db: &impl ConnectionTrait,
    id: Uuid,
    lock: bool,
) -> Result<Account, AppError> {
    let mut query = user::Entity::find_by_id(id)
        .select_only()
        .column(user::Column::IsAdmin)
        .column(user::Column::BannedAt);
    if lock {
        query = query.lock_shared();
    }
    let row: Option<(bool, Option<DateTimeWithTimeZone>)> = query.into_tuple().one(db).await?;
    Ok(match row {
        None => Account::Missing,
        Some((_, Some(_))) => Account::Banned,
        Some((is_admin, None)) => Account::Active { is_admin },
    })
}

/// Unlocked read; enough wherever a stale answer only lasts one request.
pub async fn account_status(db: &impl ConnectionTrait, id: Uuid) -> Result<Account, AppError> {
    load_account(db, id, false).await
}

/// `FOR SHARE` read inside a transaction that creates something only an unbanned user may own
/// (window, wave, refresh token): it waits for a concurrent ban (`FOR UPDATE`) and then sees it,
/// or the ban waits for this transaction and cleans up what it wrote.
pub async fn lock_unbanned(db: &impl ConnectionTrait, id: Uuid) -> Result<(), AppError> {
    match load_account(db, id, true).await? {
        Account::Active { .. } => Ok(()),
        Account::Banned => Err(AppError::Banned),
        Account::Missing => Err(AppError::Unauthorized),
    }
}

/// The usable account for `token`: bad token or deleted account → 401, banned → 403.
pub async fn verify_access(
    db: &impl ConnectionTrait,
    secret: &str,
    token: &str,
) -> Result<AuthUser, AppError> {
    let id = jwt::verify(secret, token).ok_or(AppError::Unauthorized)?;
    match account_status(db, id).await? {
        Account::Missing => Err(AppError::Unauthorized),
        Account::Banned => Err(AppError::Banned),
        Account::Active { is_admin } => Ok(AuthUser { id, is_admin }),
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
