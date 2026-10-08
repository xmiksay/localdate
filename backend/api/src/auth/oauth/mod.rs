//! Sign in with an OAuth / OpenID Connect provider (docs/api.md "Sign in with Google"):
//! server-side authorization code flow with PKCE, a signed flow cookie and one-time codes handed
//! to the SPA through the URL fragment. Google is the first provider; a new one is a `Provider`
//! variant, an `IdentityProvider` enum value and its `OidcConfig` preset in `config::from_env`.

pub mod config;
pub mod cookie;
mod flow;
mod grant;
mod notice;
pub mod oidc;

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::http::HeaderValue;
use axum::routing::{get, post};
use axum::{Router, middleware};
use chrono::Utc;
use entity::{IdentityProvider, user_identity};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set, SqlErr,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use config::{OidcConfig, SubjectSource};
use cookie::{Flow, Linker, Mode, Signer};
use oidc::Oidc;

use crate::config::Config;
use crate::error::AppError;
use crate::rate_limit::limit_by_ip;
use crate::state::AppState;

/// Provider calls must not hold a request (and a rate-limit token) for long.
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

/// A provider reachable under `/auth/oauth/{provider}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Google,
}

impl Provider {
    pub const ALL: [Self; 1] = [Self::Google];

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "google" => Some(Self::Google),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Google => "google",
        }
    }

    /// Name shown to people (mails).
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Google => "Google",
        }
    }

    pub fn identity(self) -> IdentityProvider {
        match self {
            Self::Google => IdentityProvider::Google,
        }
    }
}

/// Configured providers plus the cookie signer; a provider left unconfigured is disabled.
pub struct OAuthService {
    signer: Signer,
    providers: HashMap<Provider, Oidc>,
    /// `Secure` cookies unless `APP_BASE_URL` is plain http (dev), where browsers would drop them.
    secure: bool,
}

impl OAuthService {
    pub fn new(config: &Config) -> Result<Self> {
        // No redirects: a token endpoint that redirects is misconfigured, and following it would
        // resend the client secret elsewhere.
        let http = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .context("building OAuth HTTP client")?;
        let providers = config
            .oauth
            .iter()
            .map(|c| (c.provider, Oidc::new(c.clone(), http.clone())))
            .collect();
        Ok(Self {
            signer: Signer::new(&config.jwt_secret)?,
            providers,
            secure: config
                .app_base_url
                .as_deref()
                .is_some_and(|b| b.starts_with("https://")),
        })
    }

    pub fn enabled(&self, provider: Provider) -> bool {
        self.providers.contains_key(&provider)
    }

    fn oidc(&self, provider: Provider) -> Result<&Oidc, AppError> {
        self.providers
            .get(&provider)
            .ok_or(AppError::ProviderDisabled)
    }

    /// `Set-Cookie` that drops the flow cookie (exchange, finished link, logout).
    pub fn clear_cookie(&self) -> Result<HeaderValue> {
        cookie::set_cookie(None, self.secure)
    }

    /// A new flow for `provider`: the `Set-Cookie` value and the provider URL to send the browser to.
    fn begin(
        &self,
        provider: Provider,
        mode: Mode,
        redirect: Option<&str>,
        linker: Option<Linker>,
    ) -> Result<(HeaderValue, String), AppError> {
        let oidc = self.oidc(provider)?;
        let flow = Flow::new(provider, mode, cookie::safe_redirect(redirect), linker);
        let url =
            oidc.authorize_url(&flow.state, &flow.nonce, &cookie::challenge(&flow.verifier))?;
        let set = cookie::set_cookie(Some(&self.signer.sign(&flow)?), self.secure)?;
        Ok((set, url))
    }
}

pub fn router() -> Router<AppState> {
    let limited = Router::new()
        .route("/auth/oauth/{provider}/link", post(flow::link))
        .route("/auth/oauth/signup", post(flow::signup))
        .route_layer(middleware::from_fn(limit_by_ip));
    // `start` limits inside the handler so a refusal still lands on the SPA's done page. The
    // callback is not limited: its flow was already counted at start, and refusing it would waste
    // the provider's single-use code the user just consented for.
    Router::new()
        .merge(limited)
        .route("/auth/oauth/{provider}/start", get(flow::start))
        .route("/auth/oauth/{provider}/callback", get(flow::callback))
        .route("/auth/oauth/exchange", post(flow::exchange))
}

/// The identity holding `(provider, subject)`.
async fn identity_for(
    db: &impl ConnectionTrait,
    provider: IdentityProvider,
    subject: &str,
) -> Result<Option<user_identity::Model>, AppError> {
    Ok(user_identity::Entity::find()
        .filter(user_identity::Column::Provider.eq(provider))
        .filter(user_identity::Column::Subject.eq(subject))
        .one(db)
        .await?)
}

/// A verified identity; `(provider, subject)` already being anyone's becomes `taken`.
async fn insert_identity(
    db: &impl ConnectionTrait,
    user_id: Uuid,
    provider: IdentityProvider,
    subject: &str,
    taken: AppError,
) -> Result<user_identity::Model, AppError> {
    let now = Utc::now().fixed_offset();
    user_identity::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(user_id),
        provider: Set(provider),
        subject: Set(subject.to_owned()),
        verified_at: Set(now),
        created_at: Set(now),
    }
    .insert(db)
    .await
    .map_err(|e| match e.sql_err() {
        Some(SqlErr::UniqueConstraintViolation(_)) => taken,
        _ => e.into(),
    })
}
