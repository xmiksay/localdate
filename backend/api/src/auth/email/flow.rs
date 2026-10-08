//! Using a mailed token: `preview` (never consumes), `verify` (login) and `signup`.

use anyhow::Context;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::Utc;
use entity::{EmailTokenPurpose, email_token, user};
use sea_orm::{ActiveModelTrait, ConnectionTrait, EntityTrait, Set, SqlErr, TransactionTrait};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{identity_for, insert_identity, token, username_of};
use crate::auth::extractor::lock_unbanned;
use crate::auth::{Tokens, session, validation};
use crate::error::{AppError, AppJson};
use crate::state::AppState;

#[derive(Deserialize)]
pub(super) struct TokenBody {
    token: String,
}

#[derive(Serialize)]
pub(super) struct Preview {
    purpose: EmailTokenPurpose,
    username: Option<String>,
    email: String,
}

/// The account a live token leads to (`None` for sign-up), or `invalid_token` when following it
/// could no longer work: the identity was removed, the account is gone, or (sign-up) the address
/// became an account meanwhile.
pub(crate) async fn target(
    db: &impl ConnectionTrait,
    row: &email_token::Model,
) -> Result<Option<Uuid>, AppError> {
    let identity = identity_for(db, &row.email).await?;
    match row.purpose {
        EmailTokenPurpose::Signup if identity.is_some() => Err(AppError::InvalidToken),
        EmailTokenPurpose::Signup => Ok(None),
        EmailTokenPurpose::Login | EmailTokenPurpose::PasswordReset => {
            match (row.user_id, identity) {
                (Some(user), Some(i)) if i.user_id == user => Ok(Some(user)),
                _ => Err(AppError::InvalidToken),
            }
        }
        EmailTokenPurpose::Link => row.user_id.map(Some).ok_or(AppError::InvalidToken),
    }
}

/// Lets the page ask "log in as <username>?" before anything is spent, so mail scanners and link
/// prefetchers that open the URL cannot burn the token.
pub(super) async fn preview(
    State(state): State<AppState>,
    AppJson(body): AppJson<TokenBody>,
) -> Result<Json<Preview>, AppError> {
    state.email()?;
    let row = token::peek(&state.db, &body.token)
        .await?
        .filter(|r| r.purpose != EmailTokenPurpose::PasswordReset)
        .ok_or(AppError::InvalidToken)?;
    let username = match target(&state.db, &row).await? {
        Some(id) => Some(
            username_of(&state.db, id)
                .await?
                .ok_or(AppError::InvalidToken)?,
        ),
        None => None,
    };
    Ok(Json(Preview {
        purpose: row.purpose,
        username,
        email: row.email,
    }))
}

/// Login: consuming, the account checks and the new session are one transaction, so any refusal
/// (ban, vanished account, DB error) leaves the token unspent.
pub(super) async fn verify(
    State(state): State<AppState>,
    AppJson(body): AppJson<TokenBody>,
) -> Result<Json<Tokens>, AppError> {
    state.email()?;
    let txn = state.db.begin().await.context("begin verify txn")?;
    let row = token::consume(&txn, &body.token, EmailTokenPurpose::Login, None).await?;
    let user_id = target(&txn, &row).await?.ok_or(AppError::InvalidToken)?;
    match lock_unbanned(&txn, user_id).await {
        Err(AppError::Unauthorized) => return Err(AppError::InvalidToken),
        other => other?,
    }
    let user = user::Entity::find_by_id(user_id)
        .one(&txn)
        .await?
        .ok_or(AppError::InvalidToken)?;
    let tokens = session(&state, &txn, &user).await?;
    txn.commit().await.context("commit verify txn")?;
    Ok(Json(tokens))
}

#[derive(Deserialize)]
pub(super) struct SignupBody {
    token: String,
    username: String,
}

pub(super) async fn signup(
    State(state): State<AppState>,
    AppJson(body): AppJson<SignupBody>,
) -> Result<(StatusCode, Json<Tokens>), AppError> {
    state.email()?;
    let username = validation::normalize_username(&body.username)?;
    // One transaction: a taken username rolls the token consumption back, so the link still works.
    let txn = state.db.begin().await.context("begin signup txn")?;
    let row = token::consume(&txn, &body.token, EmailTokenPurpose::Signup, None).await?;
    let user = user::ActiveModel {
        id: Set(Uuid::new_v4()),
        username: Set(username),
        password_hash: Set(None),
        created_at: Set(Utc::now().fixed_offset()),
        is_admin: Set(false),
        banned_at: Set(None),
        credentials_changed_at: Set(None),
    }
    .insert(&txn)
    .await
    .map_err(|e| match e.sql_err() {
        Some(SqlErr::UniqueConstraintViolation(_)) => AppError::UsernameTaken,
        _ => e.into(),
    })?;
    insert_identity(&txn, user.id, &row.email).await?;
    let tokens = session(&state, &txn, &user).await?;
    txn.commit().await.context("commit signup txn")?;
    Ok((StatusCode::CREATED, Json(tokens)))
}
