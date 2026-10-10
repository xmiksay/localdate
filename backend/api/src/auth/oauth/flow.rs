//! Handlers: `start` / `link` / `import_picture` begin a flow, `callback` finishes it at the provider's redirect
//! (`exchange.rs` takes over from there).

use anyhow::Context;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::header::{CACHE_CONTROL, LOCATION, SET_COOKIE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use entity::{OAuthGrantPurpose, user_identity};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::cookie::{self, Flow, Linker, Mode};
use super::import::{self, Outcome};
use super::oidc::Oidc;
use super::{Provider, grant, identity_for, insert_identity, notice};
use crate::auth::email::message::Lang;
use crate::auth::extractor::{Account, account_status, lock_unbanned, lock_user};
use crate::auth::{AuthUser, refresh};
use crate::error::{AppError, AppJson};
use crate::rate_limit::ClientIp;
use crate::state::AppState;

const DONE_PATH: &str = "/auth/oauth/done";

#[derive(Deserialize)]
pub(super) struct StartQuery {
    redirect: Option<String>,
    /// `1` / `true` asks for the profile picture import.
    import_photo: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct LinkBody {
    redirect: Option<String>,
    lang: Option<String>,
    #[serde(default)]
    import_photo: bool,
}

#[derive(Deserialize)]
pub(super) struct ImportBody {
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
    let import_photo = matches!(query.import_photo.as_deref(), Some("1" | "true"));
    match state.oauth.begin(
        provider,
        Mode::Login,
        query.redirect.as_deref(),
        None,
        import_photo,
    ) {
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
        body.import_photo,
    )?;
    Ok(([(SET_COOKIE, set)], Json(LinkUrl { url })))
}

/// Re-import the provider's current profile picture: the access token is never stored, so this is
/// another trip through the provider, bound to the caller like a link.
pub(super) async fn import_picture(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    auth: AuthUser,
    AppJson(body): AppJson<ImportBody>,
) -> Result<([(axum::http::HeaderName, HeaderValue); 1], Json<LinkUrl>), AppError> {
    let provider = Provider::parse(&provider).ok_or(AppError::NotFound)?;
    state.oauth.offers_picture(provider)?;
    let (set, url) = state.oauth.begin(
        provider,
        Mode::Import,
        body.redirect.as_deref(),
        Some(Linker {
            user_id: auth.id,
            token_issued_at: auth.issued_at,
            lang: body.lang,
        }),
        true,
    )?;
    Ok(([(SET_COOKIE, set)], Json(LinkUrl { url })))
}

enum Done {
    Code(String),
    Linked(Provider),
    Imported(Outcome),
}

/// `&photo=<outcome>` when an import was asked for.
fn photo_param(outcome: Option<Outcome>) -> String {
    outcome
        .map(|o| format!("&photo={}", o.as_str()))
        .unwrap_or_default()
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
        Ok((Done::Code(code), photo)) => done(
            &format!("code={code}{}", photo_param(photo)),
            redirect.as_deref(),
            None,
        ),
        Ok((Done::Linked(p), photo)) => done(
            &format!("linked={}{}", p.as_str(), photo_param(photo)),
            redirect.as_deref(),
            clear,
        ),
        Ok((Done::Imported(outcome), _)) => done(
            &format!("imported={}", outcome.as_str()),
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
) -> Result<(Done, Option<Outcome>), &'static str> {
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
    let verified = oidc
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
    let subject = verified.subject;
    // The access token exists only during this callback; it is used, if at all, after the account
    // checks passed, so a refused flow downloads nothing.
    let import = match (flow.import_photo, verified.access_token.as_deref()) {
        (true, Some(token)) => Some((oidc, token)),
        _ => None,
    };
    if flow.mode == Mode::Login {
        return login_code(state, provider, &subject, &flow.state, import)
            .await
            .map(|(code, photo)| (Done::Code(code), photo))
            .map_err(|e| e.code());
    }
    let user_id = link_identity(state, provider, &subject, flow.linker, flow.mode)
        .await
        .map_err(|e| e.code())?;
    let photo = match import {
        Some((oidc, token)) => Some(import::fetch_and_attach(state, oidc, token, user_id).await),
        None => None,
    };
    Ok(match flow.mode {
        // `begin` sets `import_photo` for every import flow; no access token is a failed import.
        Mode::Import => (Done::Imported(photo.unwrap_or(Outcome::Failed)), None),
        _ => (Done::Linked(provider), photo),
    })
}

/// A one-time code bound to this flow: `login` for a linked provider account, else `signup_code`.
/// Only a new account imports: its picture rides in the sign-up grant (`pending`) until `signup`.
async fn login_code(
    state: &AppState,
    provider: Provider,
    subject: &str,
    flow_state: &str,
    import: Option<(&Oidc, &str)>,
) -> Result<(String, Option<Outcome>), AppError> {
    let binding = Some(refresh::hash_token(flow_state));
    let identity = identity_for(&state.db, provider.identity(), subject).await?;
    let (purpose, user_id, pending, photo) = match identity {
        // Banned → no code at all; `exchange` checks again under its transaction.
        Some(identity) => {
            lock_unbanned(&state.db, identity.user_id).await?;
            (OAuthGrantPurpose::Login, Some(identity.user_id), None, None)
        }
        None => {
            let (pending, photo) = match import {
                Some((oidc, token)) => match import::fetch(state, oidc, token).await {
                    Ok(webp) => (Some(webp), Some(Outcome::Pending)),
                    Err(outcome) => (None, Some(outcome)),
                },
                None => (None, None),
            };
            (OAuthGrantPurpose::SignupCode, None, pending, photo)
        }
    };
    let issued = grant::issue(
        &state.db,
        purpose,
        provider.identity(),
        subject,
        user_id,
        binding,
        pending,
    )
    .await?;
    Ok((issued.token, photo))
}

/// Attaches the identity to the account that started the flow; returns that account. In import
/// mode the account may already hold that very identity, but not another one of the provider: the
/// re-import would otherwise quietly add a second provider account.
async fn link_identity(
    state: &AppState,
    provider: Provider,
    subject: &str,
    linker: Option<Linker>,
    mode: Mode,
) -> Result<Uuid, AppError> {
    let linker = linker.ok_or(AppError::Unauthorized)?;
    let user_id = linker.user_id;
    let lang = Lang::parse(linker.lang.as_deref());
    let txn = state.db.begin().await.context("begin oauth link txn")?;
    // Import also checks "no other identity of this provider" before inserting: two concurrent
    // imports under share locks would both pass it and link two provider accounts.
    if mode == Mode::Import {
        lock_user(&txn, user_id).await?;
    } else {
        lock_unbanned(&txn, user_id).await?;
    }
    // Under the row lock a password reset/change has either committed (and is seen here) or waits.
    if account_status(&txn, user_id, linker.token_issued_at).await? == Account::Superseded {
        return Err(AppError::Unauthorized);
    }
    let existing = identity_for(&txn, provider.identity(), subject).await?;
    // Linking the same provider account twice is a no-op, not an error (and no new notice).
    if existing.as_ref().is_some_and(|i| i.user_id == user_id) {
        return Ok(user_id);
    }
    if mode == Mode::Import && has_provider_identity(&txn, user_id, provider).await? {
        return Err(AppError::IdentityMismatch);
    }
    if existing.is_some() {
        return Err(AppError::IdentityTaken);
    }
    insert_identity(
        &txn,
        user_id,
        provider.identity(),
        subject,
        AppError::IdentityTaken,
    )
    .await?;
    txn.commit().await.context("commit oauth link txn")?;
    notice::linked(state, user_id, provider, lang);
    Ok(user_id)
}

async fn has_provider_identity(
    db: &impl ConnectionTrait,
    user_id: Uuid,
    provider: Provider,
) -> Result<bool, AppError> {
    Ok(user_identity::Entity::find()
        .filter(user_identity::Column::UserId.eq(user_id))
        .filter(user_identity::Column::Provider.eq(provider.identity()))
        .count(db)
        .await?
        > 0)
}
