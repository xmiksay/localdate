use std::sync::LazyLock;

use axum::extract::State;
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::routing::post;
use axum::{Json, Router, middleware};
use chrono::{DateTime, Utc};
use entity::user;
use sea_orm::{ActiveModelTrait, ConnectionTrait, EntityTrait, QueryFilter, Set, SqlErr};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{jwt, password, refresh, validation};
use crate::error::{AppError, AppJson};
use crate::rate_limit::limit_by_ip;
use crate::state::AppState;

/// Verified against when the username is unknown or has no password, so login timing reveals neither.
static DUMMY_HASH: LazyLock<String> =
    LazyLock::new(|| password::hash("dummy password for timing").unwrap_or_default());

pub fn router() -> Router<AppState> {
    let limited = Router::new()
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route_layer(middleware::from_fn(limit_by_ip));
    Router::new()
        .merge(limited)
        .route("/auth/refresh", post(refresh))
        .route("/auth/logout", post(logout))
        .merge(super::email::router())
        .merge(super::reset::router())
        .merge(super::oauth::router())
}

#[derive(Deserialize)]
struct Credentials {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct RefreshBody {
    refresh_token: String,
}

#[derive(Serialize)]
pub struct UserDto {
    pub id: Uuid,
    pub username: String,
    pub created_at: DateTime<Utc>,
}

impl From<&user::Model> for UserDto {
    fn from(u: &user::Model) -> Self {
        Self {
            id: u.id,
            username: u.username.clone(),
            created_at: u.created_at.to_utc(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct Tokens {
    access_token: String,
    refresh_token: String,
    user: UserDto,
}

fn access_token(state: &AppState, user_id: Uuid) -> Result<String, AppError> {
    Ok(jwt::issue(&state.config.jwt_secret, user_id)?)
}

/// A new session (fresh refresh-token family) for `user`, written through `db` so callers can put
/// it in their transaction; callers check the ban first.
pub(crate) async fn session(
    state: &AppState,
    db: &impl ConnectionTrait,
    user: &user::Model,
) -> Result<Tokens, AppError> {
    let refresh_token = refresh::issue(db, user.id, Uuid::new_v4()).await?;
    Ok(Tokens {
        access_token: access_token(state, user.id)?,
        refresh_token,
        user: user.into(),
    })
}

async fn register(
    State(state): State<AppState>,
    AppJson(body): AppJson<Credentials>,
) -> Result<(StatusCode, Json<Tokens>), AppError> {
    let username = validation::normalize_username(&body.username)?;
    validation::validate_password(&body.password)?;
    let password_hash = password::hash_async(body.password).await?;

    let inserted = user::ActiveModel {
        id: Set(Uuid::new_v4()),
        username: Set(username.name),
        username_key: Set(username.key),
        password_hash: Set(Some(password_hash)),
        created_at: Set(Utc::now().fixed_offset()),
        is_admin: Set(false),
        banned_at: Set(None),
        credentials_changed_at: Set(None),
        is_test: Set(false),
    }
    .insert(&state.db)
    .await
    .map_err(|e| match e.sql_err() {
        Some(SqlErr::UniqueConstraintViolation(_)) => AppError::UsernameTaken,
        _ => e.into(),
    })?;

    Ok((
        StatusCode::CREATED,
        Json(session(&state, &state.db, &inserted).await?),
    ))
}

async fn login(
    State(state): State<AppState>,
    AppJson(body): AppJson<Credentials>,
) -> Result<Json<Tokens>, AppError> {
    let found = user::Entity::find()
        .filter(validation::username_matches(&body.username))
        .one(&state.db)
        .await?;

    // Unknown and passwordless accounts verify against the dummy hash, so all three failures take
    // the same time; `has_password` keeps someone who types the dummy password out of the latter.
    let stored = found.as_ref().and_then(|u| u.password_hash.clone());
    let has_password = stored.is_some();
    let hash = stored.unwrap_or_else(|| DUMMY_HASH.clone());
    // Oversized passwords can't be valid; refuse before spending argon2 time on them.
    let ok = body.password.chars().count() <= validation::MAX_PASSWORD_CHARS
        && password::verify_async(body.password, hash).await?;
    let user = found
        .filter(|_| ok && has_password)
        .ok_or(AppError::InvalidCredentials)?;
    // Only after the password matched, so the ban doesn't reveal that the account exists.
    if user.banned_at.is_some() {
        return Err(AppError::Banned);
    }

    Ok(Json(session(&state, &state.db, &user).await?))
}

async fn refresh(
    State(state): State<AppState>,
    AppJson(body): AppJson<RefreshBody>,
) -> Result<Json<Tokens>, AppError> {
    let (user_id, refresh_token) = refresh::rotate(&state.db, &body.refresh_token).await?;
    let user = user::Entity::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::InvalidRefreshToken)?;
    Ok(Json(Tokens {
        access_token: access_token(&state, user_id)?,
        refresh_token,
        user: (&user).into(),
    }))
}

async fn logout(
    State(state): State<AppState>,
    AppJson(body): AppJson<RefreshBody>,
) -> Result<(StatusCode, [(HeaderName, HeaderValue); 1]), AppError> {
    refresh::revoke_by_token(&state.db, &body.refresh_token).await?;
    // A half-finished OAuth flow must not outlive the session on a shared device.
    let clear = state.oauth.clear_cookie()?;
    Ok((StatusCode::NO_CONTENT, [(SET_COOKIE, clear)]))
}
