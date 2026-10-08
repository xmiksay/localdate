//! Handlers: `start` / `link` begin a flow, `callback` finishes it at the provider's redirect,
//! `exchange` turns a one-time code into a session or a sign-up token, `signup` creates the account.

use anyhow::Context;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::header::{CACHE_CONTROL, LOCATION, SET_COOKIE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use entity::{IdentityProvider, OAuthGrantPurpose, user};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use sea_orm::{ActiveModelTrait, EntityTrait, Set, SqlErr, TransactionTrait};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::cookie::{self, Flow, Linker, Mode};
use super::{Provider, grant, identity_for, insert_identity, notice};
use crate::auth::email::message::Lang;
use crate::auth::extractor::{Account, account_status, lock_unbanned};
use crate::auth::{AuthUser, Tokens, refresh, session, validation};
use crate::error::{AppError, AppJson};
use crate::rate_limit::ClientIp;
use crate::state::AppState;

const DONE_PATH: &str = "/auth/oauth/done";

#[derive(Deserialize)]
pub(super) struct StartQuery {
    redirect: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct LinkBody {
    redirect: Option<String>,
    lang: Option<String>,
}

#[derive(Serialize)]
pub(super) struct LinkUrl {
    url: String,
}

#[derive(Deserialize)]
pub(super) struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

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

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Exchange {
    Session(Tokens),
    Signup(PendingSignup),
}

/// `302` to the SPA's done page with `fragment` (+ the flow's redirect); `clear` drops the cookie.
fn done(fragment: &str, redirect: Option<&str>, clear: Option<bool>) -> Response {
    let mut location = format!("{DONE_PATH}#{fragment}");
    if let Some(r) = redirect {
        location.push_str("&redirect=");
        location.extend(utf8_percent_encode(r, NON_ALPHANUMERIC));
    }
    let mut headers = HeaderMap::new();
    match HeaderValue::try_from(location) {
        Ok(v) => headers.insert(LOCATION, v),
        Err(_) => return AppError::Internal.into_response(),
    };
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Some(secure) = clear {
        match cookie::set_cookie(None, secure) {
            Ok(v) => headers.insert(SET_COOKIE, v),
            Err(e) => return AppError::from(e).into_response(),
        };
    }
    (StatusCode::FOUND, headers).into_response()
}

fn rate_limited(state: &AppState, ip: Option<std::net::IpAddr>) -> bool {
    ip.is_some_and(|ip| !state.limiter.check(ip))
}

/// Login / sign-up: a plain browser navigation, answered with a redirect either way.
pub(super) async fn start(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    ClientIp(ip): ClientIp,
    Query(query): Query<StartQuery>,
) -> Response {
    let Some(provider) = Provider::parse(&provider) else {
        return AppError::NotFound.into_response();
    };
    if rate_limited(&state, ip) {
        return done("error=rate_limited", None, None);
    }
    match state
        .oauth
        .begin(provider, Mode::Login, query.redirect.as_deref(), None)
    {
        Ok((set, url)) => match HeaderValue::try_from(url) {
            Ok(url) => (StatusCode::FOUND, [(LOCATION, url), (SET_COOKIE, set)]).into_response(),
            Err(_) => AppError::Internal.into_response(),
        },
        Err(e) => done(&format!("error={}", e.code()), None, None),
    }
}

/// Linking needs the bearer token, so it is a fetch that hands the provider URL back.
pub(super) async fn link(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    auth: AuthUser,
    AppJson(body): AppJson<LinkBody>,
) -> Result<([(axum::http::HeaderName, HeaderValue); 1], Json<LinkUrl>), AppError> {
    let provider = Provider::parse(&provider).ok_or(AppError::NotFound)?;
    let (set, url) = state.oauth.begin(
        provider,
        Mode::Link,
        body.redirect.as_deref(),
        Some(Linker {
            user_id: auth.id,
            token_issued_at: auth.issued_at,
            lang: body.lang,
        }),
    )?;
    Ok(([(SET_COOKIE, set)], Json(LinkUrl { url })))
}

enum Done {
    Code(String),
    Linked(Provider),
}

pub(super) async fn callback(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    headers: HeaderMap,
    Query(query): Query<CallbackQuery>,
) -> Response {
    let Some(provider) = Provider::parse(&provider) else {
        return AppError::NotFound.into_response();
    };
    let flow = state.oauth.signer.read(&headers);
    let redirect = flow.as_ref().and_then(|f| f.redirect.clone());
    let clear = Some(state.oauth.secure);
    match finish(&state, provider, flow, query).await {
        // The cookie stays: `exchange` checks the code against it.
        Ok(Done::Code(code)) => done(&format!("code={code}"), redirect.as_deref(), None),
        Ok(Done::Linked(p)) => done(
            &format!("linked={}", p.as_str()),
            redirect.as_deref(),
            clear,
        ),
        Err(code) => done(&format!("error={code}"), redirect.as_deref(), clear),
    }
}

async fn finish(
    state: &AppState,
    provider: Provider,
    flow: Option<Flow>,
    query: CallbackQuery,
) -> Result<Done, &'static str> {
    let oidc = state.oauth.oidc(provider).map_err(|e| e.code())?;
    let flow = flow
        .filter(|f| f.provider == provider)
        .ok_or("invalid_state")?;
    // State first, also for error answers: only the browser that started the flow learns anything.
    if !query
        .state
        .as_deref()
        .is_some_and(|s| cookie::ct_eq(s, &flow.state))
    {
        return Err("invalid_state");
    }
    if let Some(error) = query.error {
        return Err(if error == "access_denied" {
            "cancelled"
        } else {
            "oauth_failed"
        });
    }
    let code = query.code.ok_or("oauth_failed")?;
    let subject = oidc
        .subject(&code, &flow.verifier, &flow.nonce)
        .await
        .map_err(|e| {
            tracing::warn!(
                provider = provider.as_str(),
                error = format!("{e:#}"),
                "OAuth callback rejected"
            );
            "oauth_failed"
        })?;
    let outcome = match flow.mode {
        Mode::Login => login_code(state, provider, &subject, &flow.state)
            .await
            .map(Done::Code),
        Mode::Link => link_identity(state, provider, &subject, flow.linker)
            .await
            .map(|()| Done::Linked(provider)),
    };
    outcome.map_err(|e| e.code())
}

/// A one-time code bound to this flow: `login` for a linked provider account, else `signup_code`.
async fn login_code(
    state: &AppState,
    provider: Provider,
    subject: &str,
    flow_state: &str,
) -> Result<String, AppError> {
    let binding = Some(refresh::hash_token(flow_state));
    let identity = identity_for(&state.db, provider.identity(), subject).await?;
    let (purpose, user_id) = match identity {
        // Banned → no code at all; `exchange` checks again under its transaction.
        Some(identity) => {
            lock_unbanned(&state.db, identity.user_id).await?;
            (OAuthGrantPurpose::Login, Some(identity.user_id))
        }
        None => (OAuthGrantPurpose::SignupCode, None),
    };
    let issued = grant::issue(
        &state.db,
        purpose,
        provider.identity(),
        subject,
        user_id,
        binding,
    )
    .await?;
    Ok(issued.token)
}

async fn link_identity(
    state: &AppState,
    provider: Provider,
    subject: &str,
    linker: Option<Linker>,
) -> Result<(), AppError> {
    let linker = linker.ok_or(AppError::Unauthorized)?;
    let user_id = linker.user_id;
    let lang = Lang::parse(linker.lang.as_deref());
    let txn = state.db.begin().await.context("begin oauth link txn")?;
    lock_unbanned(&txn, user_id).await?;
    // Under the row lock a password reset/change has either committed (and is seen here) or waits.
    if account_status(&txn, user_id, linker.token_issued_at).await? == Account::Superseded {
        return Err(AppError::Unauthorized);
    }
    match identity_for(&txn, provider.identity(), subject).await? {
        // Linking the same provider account twice is a no-op, not an error (and no new notice).
        Some(identity) if identity.user_id == user_id => return Ok(()),
        Some(_) => return Err(AppError::IdentityTaken),
        None => {
            insert_identity(
                &txn,
                user_id,
                provider.identity(),
                subject,
                AppError::IdentityTaken,
            )
            .await?;
        }
    }
    txn.commit().await.context("commit oauth link txn")?;
    notice::linked(state, user_id, provider, lang);
    Ok(())
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
) -> Result<(StatusCode, Json<Tokens>), AppError> {
    let username = validation::normalize_username(&body.username)?;
    // One transaction: a taken username rolls the token consumption back, so it can be retried.
    let txn = state.db.begin().await.context("begin oauth signup txn")?;
    let row = grant::consume(&txn, &body.token, &[OAuthGrantPurpose::Signup], None).await?;
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
    Ok((StatusCode::CREATED, Json(tokens)))
}
