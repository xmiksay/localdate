use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use uuid::Uuid;

use super::jwt;
use crate::error::AppError;
use crate::state::AppState;

/// Authenticated caller, taken from `Authorization: Bearer <access token>`.
/// Add it as a handler argument to protect a route. The JWT is trusted without a DB lookup,
/// so a deleted account's token stays valid until it expires (max 15 min).
#[derive(Debug, Clone, Copy)]
pub struct AuthUser {
    pub id: Uuid,
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
        let id = jwt::verify(&state.config.jwt_secret, token).ok_or(AppError::Unauthorized)?;
        Ok(Self { id })
    }
}
