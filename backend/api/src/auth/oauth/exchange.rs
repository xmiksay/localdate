//! Handlers after the callback: `exchange` turns a one-time code into a session or a sign-up token,
//! `signup` creates the account (and attaches a picture imported during the callback).

use anyhow::Context;
use axum::Json;
use axum::extract::State;
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use chrono::{DateTime, Utc};
use entity::{IdentityProvider, OAuthGrantPurpose, user};
use sea_orm::{ActiveModelTrait, EntityTrait, Set, SqlErr, TransactionTrait};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{grant, identity_for, import, insert_identity};
use crate::auth::extractor::lock_unbanned;
use crate::auth::{Tokens, refresh, session, validation};
use crate::error::{AppError, AppJson};
use crate::state::AppState;

#[derive(Deserialize)]
pub(super) struct CodeBody {
    code: String,
}

#[derive(Deserialize)]
pub(super) struct SignupBody {
    token: String,
    username: String,
}

#[derive(Serialize)]
pub(super) struct PendingSignup {
    token: String,
    provider: IdentityProvider,
    expires_at: DateTime<Utc>,
}

/// `Tokens` plus, when the sign-up held an imported picture, what became of it.
#[derive(Serialize)]
pub(super) struct SignedUp {
    #[serde(flatten)]
    tokens: Tokens,
    #[serde(skip_serializing_if = "Option::is_none")]
    photo: Option<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Exchange {
    Session(Tokens),
    Signup(PendingSignup),
}

/// Consuming, the account checks and the session are one transaction, so a refusal (ban,
/// vanished identity) leaves the code unspent; a code from another browser's flow never matches.
pub(super) async fn exchange(
    State(state): State<AppState>,
    headers: HeaderMap,
    AppJson(body): AppJson<CodeBody>,
) -> Result<([(axum::http::HeaderName, HeaderValue); 1], Json<Exchange>), AppError> {
    let flow = state
        .oauth
        .signer
        .read(&headers)
        .ok_or(AppError::InvalidToken)?;
    let binding = refresh::hash_token(&flow.state);
    let txn = state.db.begin().await.context("begin oauth exchange txn")?;
    let row = grant::consume(
        &txn,
        &body.code,
        &[OAuthGrantPurpose::Login, OAuthGrantPurpose::SignupCode],
        Some(&binding),
    )
    .await?;
    let answer = match (row.purpose, row.user_id) {
        (OAuthGrantPurpose::Login, Some(user_id)) => {
            // Ban first: a ban since the callback must read as `banned`, whatever else it changed.
            match lock_unbanned(&txn, user_id).await {
                Err(AppError::Unauthorized) => return Err(AppError::InvalidToken),
                other => other?,
            }
            let linked = identity_for(&txn, row.provider, &row.subject)
                .await?
                .is_some_and(|i| i.user_id == user_id);
            if !linked {
                return Err(AppError::InvalidToken);
            }
            let user = user::Entity::find_by_id(user_id)
                .one(&txn)
                .await?
                .ok_or(AppError::InvalidToken)?;
            Exchange::Session(session(&state, &txn, &user).await?)
        }
        (OAuthGrantPurpose::SignupCode, _) => {
            let issued = grant::issue(
                &txn,
                OAuthGrantPurpose::Signup,
                row.provider,
                &row.subject,
                None,
                None,
                row.photo,
            )
            .await?;
            Exchange::Signup(PendingSignup {
                token: issued.token,
                provider: row.provider,
                expires_at: issued.expires_at,
            })
        }
        _ => return Err(AppError::InvalidToken),
    };
    txn.commit().await.context("commit oauth exchange txn")?;
    let clear = state.oauth.clear_cookie()?;
    Ok(([(SET_COOKIE, clear)], Json(answer)))
}

pub(super) async fn signup(
    State(state): State<AppState>,
    AppJson(body): AppJson<SignupBody>,
) -> Result<(StatusCode, Json<SignedUp>), AppError> {
    let username = validation::normalize_username(&body.username)?;
    // One transaction: a taken username rolls the token consumption back, so it can be retried.
    let txn = state.db.begin().await.context("begin oauth signup txn")?;
    let row = grant::consume(&txn, &body.token, &[OAuthGrantPurpose::Signup], None).await?;
    let user = user::ActiveModel {
        id: Set(Uuid::new_v4()),
        username: Set(username.name),
        username_key: Set(username.key),
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
    insert_identity(
        &txn,
        user.id,
        row.provider,
        &row.subject,
        AppError::InvalidToken,
    )
    .await?;
    let tokens = session(&state, &txn, &user).await?;
    txn.commit().await.context("commit oauth signup txn")?;
    // After the commit: the photo needs the user row, and a failure here must not undo the account.
    let photo = match row.photo {
        Some(webp) => Some(import::attach(&state, user.id, webp).await.as_str()),
        None => None,
    };
    Ok((StatusCode::CREATED, Json(SignedUp { tokens, photo })))
}
