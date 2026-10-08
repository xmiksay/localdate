//! Email magic link: `/auth/providers` and `/auth/email/*` (docs/api.md "Login providers and
//! email magic link"). Linking an address to an existing account lives in `me::identities`.

mod flow;
mod limit;
pub mod message;
pub mod token;

use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router, middleware};
use chrono::Utc;
use entity::{EmailTokenPurpose, IdentityProvider, user, user_identity};
use lettre::Address;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set, SqlErr,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use uuid::Uuid;

pub use limit::EmailLimiter;
use message::{Kind, Lang};

use crate::error::{AppError, AppJson};
use crate::mail::Mailer;
use crate::rate_limit::{ClientIp, limit_by_ip};
use crate::state::AppState;

/// Mails in flight at once; further requests wait for a slot before answering.
const MAIL_CONCURRENCY: usize = 4;
/// Longest a request waits for a send slot; past it the mail is dropped (the answer is 202 anyway).
const SLOT_WAIT: Duration = Duration::from_secs(5);
/// A hung SMTP server must not hold a slot forever.
const SEND_TIMEOUT: Duration = Duration::from_secs(30);

/// What the email endpoints need; absent from `AppState` when email is disabled.
#[derive(Clone)]
pub struct EmailService {
    mailer: Arc<dyn Mailer>,
    /// PWA origin without trailing slash (`APP_BASE_URL`).
    base_url: String,
    permits: Arc<Semaphore>,
}

impl EmailService {
    pub fn new(mailer: Arc<dyn Mailer>, base_url: String) -> Self {
        Self {
            mailer,
            base_url,
            permits: Arc::new(Semaphore::new(MAIL_CONCURRENCY)),
        }
    }

    /// Queues `kind` for `to` and returns once a send slot is taken. The SMTP round trip runs in
    /// its own task, so the answer (always 202) neither waits for nor reveals it; failures are logged.
    /// `token` goes into the URL fragment, which browsers never send, so it stays out of access logs.
    pub async fn send(
        &self,
        kind: Kind,
        lang: Lang,
        to: Address,
        token: Option<&str>,
        username: Option<&str>,
    ) {
        let path = if kind == Kind::Link {
            "/auth/email/link"
        } else {
            "/auth/email"
        };
        let link = token.map(|t| format!("{}{path}#token={t}", self.base_url));
        let email = message::render(kind, lang, to, link.as_deref(), username);
        let permit =
            match tokio::time::timeout(SLOT_WAIT, self.permits.clone().acquire_owned()).await {
                Ok(Ok(permit)) => permit,
                Ok(Err(_)) => return, // semaphore is never closed
                Err(_) => {
                    tracing::warn!("all mail slots busy, email dropped");
                    return;
                }
            };
        let mailer = self.mailer.clone();
        tokio::spawn(async move {
            match tokio::time::timeout(SEND_TIMEOUT, mailer.send(email)).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => tracing::error!(error = ?e, "sending email failed"),
                Err(_) => tracing::error!("sending email timed out"),
            }
            drop(permit);
        });
    }

    /// No send in flight (tests wait for this before reading the outbox).
    pub fn idle(&self) -> bool {
        self.permits.available_permits() == MAIL_CONCURRENCY
    }
}

impl AppState {
    /// The email service, or `503 email_disabled`.
    pub fn email(&self) -> Result<&EmailService, AppError> {
        self.email.as_ref().ok_or(AppError::EmailDisabled)
    }
}

/// The identity holding `email`.
pub async fn identity_for(
    db: &impl ConnectionTrait,
    email: &str,
) -> Result<Option<user_identity::Model>, AppError> {
    Ok(user_identity::Entity::find()
        .filter(user_identity::Column::Provider.eq(IdentityProvider::Email))
        .filter(user_identity::Column::Subject.eq(email))
        .one(db)
        .await?)
}

pub async fn username_of(db: &impl ConnectionTrait, id: Uuid) -> Result<Option<String>, AppError> {
    Ok(user::Entity::find_by_id(id)
        .one(db)
        .await?
        .map(|u| u.username))
}

/// A verified email identity; the address already being someone's is `400 invalid_token`
/// (the caller holds a token for it, but that token no longer leads anywhere).
pub async fn insert_identity(
    db: &impl ConnectionTrait,
    user_id: Uuid,
    email: &str,
) -> Result<user_identity::Model, AppError> {
    let now = Utc::now().fixed_offset();
    user_identity::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(user_id),
        provider: Set(IdentityProvider::Email),
        subject: Set(email.to_owned()),
        verified_at: Set(now),
        created_at: Set(now),
    }
    .insert(db)
    .await
    .map_err(|e| match e.sql_err() {
        Some(SqlErr::UniqueConstraintViolation(_)) => AppError::InvalidToken,
        _ => e.into(),
    })
}

pub fn router() -> Router<AppState> {
    let limited = Router::new()
        .route("/auth/email/start", post(start))
        .route("/auth/email/signup", post(flow::signup))
        .route_layer(middleware::from_fn(limit_by_ip));
    Router::new()
        .merge(limited)
        .route("/auth/providers", get(providers))
        .route("/auth/email/preview", post(flow::preview))
        .route("/auth/email/verify", post(flow::verify))
}

#[derive(Serialize)]
struct Providers {
    email: bool,
}

async fn providers(State(state): State<AppState>) -> Json<Providers> {
    Json(Providers {
        email: state.email.is_some(),
    })
}

#[derive(Deserialize)]
pub struct EmailRequest {
    pub email: String,
    pub lang: Option<String>,
}

async fn start(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    AppJson(body): AppJson<EmailRequest>,
) -> Result<StatusCode, AppError> {
    let service = state.email()?;
    let email = token::normalize_email(&body.email)?;
    // Silently dropped: a 429 here would confirm the address is being used.
    if !state.email_limiter.check(email.as_ref(), ip) {
        tracing::debug!("email limit hit, nothing sent");
        return Ok(StatusCode::ACCEPTED);
    }
    let lang = Lang::parse(body.lang.as_deref());
    match identity_for(&state.db, email.as_ref()).await? {
        Some(identity) => {
            let username = username_of(&state.db, identity.user_id).await?;
            let token = token::issue(
                &state.db,
                EmailTokenPurpose::Login,
                Some(identity.user_id),
                email.as_ref(),
            )
            .await?;
            service
                .send(Kind::Login, lang, email, Some(&token), username.as_deref())
                .await;
        }
        None => {
            let token =
                token::issue(&state.db, EmailTokenPurpose::Signup, None, email.as_ref()).await?;
            service
                .send(Kind::Signup, lang, email, Some(&token), None)
                .await;
        }
    }
    Ok(StatusCode::ACCEPTED)
}
