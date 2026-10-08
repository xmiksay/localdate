//! `PUT /me/password`: change the password, or set a first one on an account created by email.

use anyhow::Context;
use axum::extract::State;
use axum::routing::put;
use axum::{Json, Router, middleware};
use entity::user;
use sea_orm::{EntityTrait, TransactionTrait};
use serde::Deserialize;

use crate::auth::extractor::lock_user;
use crate::auth::reset::replace_password;
use crate::auth::{AuthUser, Tokens, password, session, validation};
use crate::error::{AppError, AppJson};
use crate::rate_limit::limit_by_ip;
use crate::state::AppState;
use crate::ws::CloseReason;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me/password", put(put_password))
        .route_layer(middleware::from_fn(limit_by_ip))
}

#[derive(Deserialize)]
struct PasswordBody {
    current_password: Option<String>,
    new_password: String,
}

/// Revokes every session and answers with a fresh one: the access JWT does not say which refresh
/// family made the call, so "all but the current" cannot be told apart.
async fn put_password(
    State(state): State<AppState>,
    auth: AuthUser,
    AppJson(body): AppJson<PasswordBody>,
) -> Result<Json<Tokens>, AppError> {
    validation::validate_password(&body.new_password)?;
    let before = user::Entity::find_by_id(auth.id)
        .one(&state.db)
        .await?
        .ok_or(AppError::Unauthorized)?;
    if let Some(stored) = before.password_hash.clone() {
        let current = body
            .current_password
            .ok_or_else(|| AppError::validation("current_password is required"))?;
        let ok = current.chars().count() <= validation::MAX_PASSWORD_CHARS
            && password::verify_async(current, stored).await?;
        if !ok {
            return Err(AppError::InvalidCredentials);
        }
    }
    let hash = password::hash_async(body.new_password).await?;
    let txn = state.db.begin().await.context("begin password txn")?;
    let locked = lock_user(&txn, auth.id).await?;
    // Changed since it was verified (outside the lock, argon2 is slow): the check no longer holds.
    if locked.password_hash != before.password_hash {
        return Err(AppError::InvalidCredentials);
    }
    let updated = replace_password(&txn, locked, hash).await?;
    let tokens = session(&state, &txn, &updated).await?;
    txn.commit().await.context("commit password txn")?;
    // The caller's own socket too: it cannot be told apart, and reconnects with its access token.
    state.hub.disconnect(auth.id, CloseReason::Unauthorized);
    Ok(Json(tokens))
}
