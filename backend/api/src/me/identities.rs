//! `/me/identities`: the caller's login methods besides the password, and email linking.

use anyhow::Context;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router, middleware};
use chrono::{DateTime, Utc};
use entity::{EmailTokenPurpose, IdentityProvider, user, user_identity};
use sea_orm::{
    ColumnTrait, EntityTrait, ModelTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect,
    TransactionTrait,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::auth::email::message::{Kind, Lang};
use crate::auth::email::{EmailRequest, identity_for, insert_identity, token, username_of};
use crate::error::{AppError, AppJson, parse_id};
use crate::rate_limit::{ClientIp, limit_by_ip};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    let limited = Router::new()
        .route("/me/identities/email", post(link_email))
        .route_layer(middleware::from_fn(limit_by_ip));
    Router::new()
        .merge(limited)
        .route("/me/identities", get(list))
        .route("/me/identities/email/confirm", post(confirm_email))
        .route("/me/identities/{id}", delete(remove))
}

#[derive(Serialize)]
struct IdentityDto {
    id: Uuid,
    provider: IdentityProvider,
    subject: String,
    verified_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

impl From<user_identity::Model> for IdentityDto {
    fn from(i: user_identity::Model) -> Self {
        Self {
            id: i.id,
            provider: i.provider,
            subject: i.subject,
            verified_at: i.verified_at.to_utc(),
            created_at: i.created_at.to_utc(),
        }
    }
}

#[derive(Serialize)]
struct IdentitiesDto {
    has_password: bool,
    identities: Vec<IdentityDto>,
}

async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<IdentitiesDto>, AppError> {
    let user = user::Entity::find_by_id(auth.id)
        .one(&state.db)
        .await?
        .ok_or(AppError::Unauthorized)?;
    let identities = user
        .find_related(user_identity::Entity)
        .order_by_asc(user_identity::Column::CreatedAt)
        .order_by_asc(user_identity::Column::Id)
        .all(&state.db)
        .await?;
    Ok(Json(IdentitiesDto {
        has_password: user.password_hash.is_some(),
        identities: identities.into_iter().map(Into::into).collect(),
    }))
}

async fn link_email(
    State(state): State<AppState>,
    auth: AuthUser,
    ClientIp(ip): ClientIp,
    AppJson(body): AppJson<EmailRequest>,
) -> Result<StatusCode, AppError> {
    let service = state.email()?;
    let email = token::normalize_email(&body.email)?;
    if !state.email_limiter.check(email.as_ref(), ip) {
        return Ok(StatusCode::ACCEPTED);
    }
    let lang = Lang::parse(body.lang.as_deref());
    // Linked anywhere (the caller included): a notice instead of a link, so the response and the
    // work done look the same whoever owns the address.
    if identity_for(&state.db, email.as_ref()).await?.is_some() {
        service
            .send(Kind::AlreadyLinked, lang, email, None, None)
            .await;
    } else {
        let username = username_of(&state.db, auth.id).await?;
        let token = token::issue(
            &state.db,
            EmailTokenPurpose::Link,
            Some(auth.id),
            email.as_ref(),
        )
        .await?;
        service
            .send(Kind::Link, lang, email, Some(&token), username.as_deref())
            .await;
    }
    Ok(StatusCode::ACCEPTED)
}

#[derive(Deserialize)]
struct TokenBody {
    token: String,
}

async fn confirm_email(
    State(state): State<AppState>,
    auth: AuthUser,
    AppJson(body): AppJson<TokenBody>,
) -> Result<(StatusCode, Json<IdentityDto>), AppError> {
    state.email()?;
    let txn = state.db.begin().await.context("begin link txn")?;
    // Filtering on the owner leaves another account's token unconsumed for its rightful user.
    let row = token::consume(&txn, &body.token, EmailTokenPurpose::Link, Some(auth.id)).await?;
    let identity = insert_identity(&txn, auth.id, &row.email).await?;
    txn.commit().await.context("commit link txn")?;
    Ok((StatusCode::CREATED, Json(identity.into())))
}

async fn remove(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let id = parse_id(&id)?;
    let txn = state.db.begin().await.context("begin unlink txn")?;
    // Row lock serializes concurrent removals, which could otherwise each see "one left" and
    // together strand the account.
    let user = user::Entity::find_by_id(auth.id)
        .lock_exclusive()
        .one(&txn)
        .await?
        .ok_or(AppError::Unauthorized)?;
    let identity = user_identity::Entity::find_by_id(id)
        .filter(user_identity::Column::UserId.eq(auth.id))
        .one(&txn)
        .await?
        .ok_or(AppError::NotFound)?;
    let others = user_identity::Entity::find()
        .filter(user_identity::Column::UserId.eq(auth.id))
        .filter(user_identity::Column::Id.ne(identity.id))
        .count(&txn)
        .await?;
    if user.password_hash.is_none() && others == 0 {
        return Err(AppError::LastLoginMethod);
    }
    identity.delete(&txn).await?;
    txn.commit().await.context("commit unlink txn")?;
    Ok(StatusCode::NO_CONTENT)
}
