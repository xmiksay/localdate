//! The fake OpenID provider: token endpoint + JWKS on a local port, signing ID tokens with the
//! fixture key; per-provider configs (scope, subject claim, nonce policy) pointed at it.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use localdate_api::auth::oauth::{NonceCheck, OidcConfig, Provider, SubjectSource};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::oauth::Started;

pub const CLIENT_ID: &str = "test-client";
pub const ISSUER: &str = "https://issuer.test";
const KEY_PEM: &[u8] = include_bytes!("../fixtures/oidc_test_key.pem");
const JWKS: &str = include_str!("../fixtures/oidc_test_jwks.json");

/// The ID token the fake provider will hand out for one code; tests bend single claims.
#[derive(Clone, Serialize)]
pub struct IdClaims {
    pub sub: String,
    /// Empty = left out of the token (a provider that does not echo it).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub nonce: String,
    pub aud: String,
    pub iss: String,
    pub exp: i64,
    pub iat: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub azp: Option<String>,
    /// Telegram's numeric user id (its subject claim).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
}

struct Pending {
    claims: IdClaims,
    challenge: String,
}

#[derive(Clone, Default)]
pub struct FakeProvider {
    codes: Arc<Mutex<HashMap<String, Pending>>>,
    pub base: String,
    /// JWKS downloads so far.
    pub jwks_hits: Arc<AtomicUsize>,
    /// While set, the JWKS endpoint answers 500.
    pub jwks_down: Arc<AtomicBool>,
}

async fn jwks(State(fake): State<FakeProvider>) -> axum::response::Response {
    use axum::response::IntoResponse;
    fake.jwks_hits.fetch_add(1, Ordering::SeqCst);
    // Slow enough that concurrent callers overlap (single-flight test).
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    if fake.jwks_down.load(Ordering::SeqCst) {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    ([(header::CONTENT_TYPE, "application/json")], JWKS).into_response()
}

impl FakeProvider {
    pub async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake provider");
        let fake = Self {
            base: format!("http://{}", listener.local_addr().expect("addr")),
            ..Self::default()
        };
        let router = Router::new()
            .route("/token", post(token))
            .route("/jwks", get(jwks))
            .with_state(fake.clone());
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        fake
    }

    pub fn config(&self, jwks_ttl: Duration) -> OidcConfig {
        self.config_for(Provider::Google, jwks_ttl)
    }

    /// `provider` pointed at this fake, with its real scope and subject claim.
    pub fn config_for(&self, provider: Provider, jwks_ttl: Duration) -> OidcConfig {
        let (scope, subject, nonce) = match provider {
            Provider::Google => ("openid", "sub", NonceCheck::Required),
            Provider::Telegram => (
                "openid profile telegram:bot_access",
                "id",
                NonceCheck::IfPresent,
            ),
        };
        OidcConfig {
            provider,
            client_id: CLIENT_ID.into(),
            client_secret: "test-secret".into(),
            redirect_uri: format!(
                "https://app.test/api/auth/oauth/{}/callback",
                provider.as_str()
            ),
            issuers: vec![ISSUER.into()],
            auth_endpoint: format!("{}/authorize", self.base),
            token_endpoint: format!("{}/token", self.base),
            jwks_uri: format!("{}/jwks", self.base),
            scope: scope.into(),
            subject: SubjectSource::IdTokenClaim(subject.into()),
            nonce,
            jwks_ttl,
        }
    }

    /// What a successful consent screen does: remembers a code for the started flow.
    pub fn authorize(&self, started: &Started, claims: IdClaims) -> String {
        let code = format!("code-{}", uuid::Uuid::new_v4().simple());
        self.codes.lock().expect("lock").insert(
            code.clone(),
            Pending {
                claims,
                challenge: started.challenge.clone(),
            },
        );
        code
    }

    pub fn claims(started: &Started, sub: &str) -> IdClaims {
        let now = chrono::Utc::now().timestamp();
        IdClaims {
            sub: sub.into(),
            nonce: started.nonce.clone(),
            aud: CLIENT_ID.into(),
            iss: ISSUER.into(),
            exp: now + 300,
            iat: now,
            azp: None,
            id: None,
        }
    }
}

async fn token(
    State(fake): State<FakeProvider>,
    Form(form): Form<HashMap<String, String>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let bad = || {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid_grant" })),
        )
    };
    let code = form.get("code").ok_or_else(bad)?;
    let pending = fake
        .codes
        .lock()
        .expect("lock")
        .remove(code)
        .ok_or_else(bad)?;
    let verifier = form.get("code_verifier").ok_or_else(bad)?;
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    if challenge != pending.challenge
        || form.get("client_id").map(String::as_str) != Some(CLIENT_ID)
        || form.get("client_secret").map(String::as_str) != Some("test-secret")
    {
        return Err(bad());
    }
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("test-key-1".into());
    let key = EncodingKey::from_rsa_pem(KEY_PEM).expect("fixture key");
    let id_token = jsonwebtoken::encode(&header, &pending.claims, &key).expect("sign");
    Ok(Json(
        json!({ "id_token": id_token, "access_token": "x", "token_type": "Bearer" }),
    ))
}
