//! "Act as": an admin gets a short-lived access token for another user (`ADMIN_IMPERSONATION`).
//! The token's checks live in `auth::extractor::authorize`; only `ActingUser` endpoints accept
//! it, every other endpoint refuses it (`AuthUser`).

use axum::Json;
use axum::extract::{Path, State};
use chrono::{DateTime, Utc};
use entity::user;
use sea_orm::EntityTrait;
use serde::Serialize;
use serde_json::json;

use super::audit;
use crate::auth::{AdminUser, UserDto, jwt};
use crate::error::{AppError, parse_id};
use crate::state::AppState;

#[derive(Serialize)]
pub struct SettingsDto {
    impersonation: bool,
}

pub async fn settings(State(state): State<AppState>, _admin: AdminUser) -> Json<SettingsDto> {
    Json(SettingsDto {
        impersonation: state.config.admin_impersonation,
    })
}

#[derive(Serialize)]
pub struct ImpersonationDto {
    access_token: String,
    expires_at: DateTime<Utc>,
    user: UserDto,
}

/// Admins are refused so an impersonation token can never carry admin rights; banned accounts
/// because their tokens are refused anyway.
fn impersonable(target: &user::Model) -> Result<(), AppError> {
    if target.is_admin || target.banned_at.is_some() {
        Err(AppError::CannotImpersonate)
    } else {
        Ok(())
    }
}

pub async fn impersonate(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(id): Path<String>,
) -> Result<Json<ImpersonationDto>, AppError> {
    // Off reads as "no such endpoint" rather than as a permission problem.
    if !state.config.admin_impersonation {
        return Err(AppError::NotFound);
    }
    let target = user::Entity::find_by_id(parse_id(&id)?)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    impersonable(&target)?;
    let (access_token, expires_at) =
        jwt::issue_impersonation(&state.config.jwt_secret, target.id, admin.id)?;
    audit::record(
        &state.db,
        admin.id,
        target.id,
        audit::IMPERSONATE,
        json!({}),
    )
    .await?;
    tracing::info!(admin = %admin.id, target = %target.id, "admin impersonation started");
    Ok(Json(ImpersonationDto {
        access_token,
        expires_at: DateTime::from_timestamp(expires_at, 0).ok_or(AppError::Internal)?,
        user: (&target).into(),
    }))
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;

    fn account(is_admin: bool, banned: bool) -> user::Model {
        user::Model {
            id: Uuid::new_v4(),
            username: "eva".into(),
            username_key: "eva".into(),
            password_hash: None,
            created_at: Utc::now().fixed_offset(),
            is_admin,
            banned_at: banned.then(|| Utc::now().fixed_offset()),
            credentials_changed_at: None,
            is_test: false,
        }
    }

    #[test]
    fn only_unbanned_non_admins_can_be_impersonated() {
        assert!(impersonable(&account(false, false)).is_ok());
        for (admin, banned) in [(true, false), (false, true), (true, true)] {
            assert!(matches!(
                impersonable(&account(admin, banned)),
                Err(AppError::CannotImpersonate)
            ));
        }
    }
}
