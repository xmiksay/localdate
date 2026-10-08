//! Password reset: `/auth/password/*` (docs/api/auth.md "Password reset"), and the password write
//! shared with `PUT /me/password`. Links go out by email and Telegram (`deliver`).

mod deliver;

pub use self::deliver::linked_addresses;

use anyhow::Context;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router, middleware};
use chrono::{Timelike, Utc};
use entity::{EmailTokenPurpose, email_token, push_subscription, user};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use self::deliver::{Login, Senders, deliver};
use super::email::message::Lang;
use super::email::{target, token, username_of};
use super::extractor::lock_user;
use super::{password, refresh, validation};
use crate::error::{AppError, AppJson};
use crate::rate_limit::{ClientIp, limit_by_ip};
use crate::state::AppState;
use crate::ws::CloseReason;

pub fn router() -> Router<AppState> {
    let limited = Router::new()
        .route("/auth/password/forgot", post(forgot))
        .route_layer(middleware::from_fn(limit_by_ip));
    Router::new()
        .merge(limited)
        .route("/auth/password/reset/preview", post(preview))
        .route("/auth/password/reset", post(reset))
}

#[derive(Deserialize)]
struct ForgotBody {
    login: String,
    lang: Option<String>,
}

async fn forgot(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    AppJson(body): AppJson<ForgotBody>,
) -> Result<StatusCode, AppError> {
    let login = Login::parse(&body.login)?;
    let senders = Senders {
        email: state.email.clone(),
        telegram: state.telegram.clone(),
    };
    // An address can only be reached by mail; a username by whatever channel is configured.
    match (&login, &senders.email, &senders.telegram) {
        (Login::Email(_), None, _) | (Login::Username(_), None, None) => {
            return Err(AppError::EmailDisabled);
        }
        _ => {}
    }
    // Silently dropped like /auth/email/start: a 429 would confirm the target is being used.
    if !state.reset_limiter.check_request(&login.limit_key(), ip) {
        return Ok(StatusCode::ACCEPTED);
    }
    let lang = Lang::parse(body.lang.as_deref());
    let (db, limiter) = (state.db.clone(), state.reset_limiter.clone());
    // The lookup runs after the response, so whether the account exists or has a linked channel
    // cannot show in the timing — unlike /auth/email/start, the paths do different work.
    state.detached.spawn(async move {
        if let Err(e) = deliver(&db, &senders, &limiter, login, lang).await {
            tracing::error!(error = ?e, "password reset delivery failed");
        }
    });
    Ok(StatusCode::ACCEPTED)
}

#[derive(Deserialize)]
struct TokenBody {
    token: String,
}

#[derive(Serialize)]
struct ResetPreview {
    username: String,
}

/// The account a live reset token leads to (see `email::target`), without consuming it.
async fn reset_target(db: &impl ConnectionTrait, raw: &str) -> Result<Uuid, AppError> {
    let row = token::peek(db, raw)
        .await?
        .filter(|r| r.purpose == EmailTokenPurpose::PasswordReset)
        .ok_or(AppError::InvalidToken)?;
    target(db, &row).await?.ok_or(AppError::InvalidToken)
}

/// Lets the page name the account before anything is spent (mail scanners open links too).
async fn preview(
    State(state): State<AppState>,
    AppJson(body): AppJson<TokenBody>,
) -> Result<Json<ResetPreview>, AppError> {
    let user = reset_target(&state.db, &body.token).await?;
    let username = username_of(&state.db, user)
        .await?
        .ok_or(AppError::InvalidToken)?;
    Ok(Json(ResetPreview { username }))
}

#[derive(Deserialize)]
struct ResetBody {
    token: String,
    new_password: String,
}

async fn reset(
    State(state): State<AppState>,
    AppJson(body): AppJson<ResetBody>,
) -> Result<StatusCode, AppError> {
    validation::validate_password(&body.new_password)?;
    // Refuse dead tokens before spending argon2 time on them.
    reset_target(&state.db, &body.token).await?;
    let hash = password::hash_async(body.new_password).await?;
    // One transaction: a ban or any error leaves the token unspent and the old password in place.
    let txn = state.db.begin().await.context("begin reset txn")?;
    let row = token::consume(&txn, &body.token, EmailTokenPurpose::PasswordReset, None).await?;
    let user_id = target(&txn, &row).await?.ok_or(AppError::InvalidToken)?;
    let user = match lock_user(&txn, user_id).await {
        Err(AppError::Unauthorized) => return Err(AppError::InvalidToken),
        other => other?,
    };
    replace_password(&txn, user, hash).await?;
    txn.commit().await.context("commit reset txn")?;
    state.hub.disconnect(user_id, CloseReason::Unauthorized);
    Ok(StatusCode::NO_CONTENT)
}

/// Stores `hash`, logs every session out (refresh tokens revoked, older access tokens refused from
/// now on via `credentials_changed_at`, push subscriptions deleted) and voids the account's unused mailed tokens (a link sent
/// before the change must not undo it or log in past it); run inside the caller's transaction,
/// then close the account's sockets once it has committed.
pub async fn replace_password(
    db: &impl ConnectionTrait,
    user: user::Model,
    hash: String,
) -> Result<user::Model, AppError> {
    let id = user.id;
    // Whole seconds, matching the JWT `iat` it is compared with (`extractor::superseded`).
    let changed_at = Utc::now()
        .with_nanosecond(0)
        .context("truncating to whole seconds")?;
    let mut active: user::ActiveModel = user.into();
    active.password_hash = Set(Some(hash));
    active.credentials_changed_at = Set(Some(changed_at.fixed_offset()));
    let updated = active.update(db).await?;
    refresh::revoke_all(db, id).await?;
    email_token::Entity::update_many()
        .col_expr(
            email_token::Column::UsedAt,
            Expr::current_timestamp().into(),
        )
        .filter(email_token::Column::UserId.eq(id))
        .filter(email_token::Column::UsedAt.is_null())
        .exec(db)
        .await?;
    // "Log out everywhere" includes a possibly stolen device's notifications; the caller's own
    // device re-subscribes with its fresh session (password change).
    push_subscription::Entity::delete_many()
        .filter(push_subscription::Column::UserId.eq(id))
        .exec(db)
        .await?;
    Ok(updated)
}
