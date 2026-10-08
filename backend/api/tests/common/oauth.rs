//! OAuth test double: a local fake OpenID provider (token endpoint + JWKS) signing ID tokens with
//! the fixture key, and helpers that walk the browser side of the flow (cookie, redirects).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::extract::State;
use axum::http::{Method, Request, StatusCode, header};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use http_body_util::BodyExt;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use localdate_api::auth::oauth::{OidcConfig, Provider, SubjectSource};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

use localdate_api::config::Config;

use super::TestApp;

pub const CLIENT_ID: &str = "test-client";
pub const ISSUER: &str = "https://issuer.test";
const KEY_PEM: &[u8] = include_bytes!("../fixtures/oidc_test_key.pem");
const JWKS: &str = include_str!("../fixtures/oidc_test_jwks.json");

/// The ID token the fake provider will hand out for one code; tests bend single claims.
#[derive(Clone, Serialize)]
pub struct IdClaims {
    pub sub: String,
    pub nonce: String,
    pub aud: String,
    pub iss: String,
    pub exp: i64,
    pub iat: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub azp: Option<String>,
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
        OidcConfig {
            provider: Provider::Google,
            client_id: CLIENT_ID.into(),
            client_secret: "test-secret".into(),
            redirect_uri: "https://app.test/api/auth/oauth/google/callback".into(),
            issuers: vec![ISSUER.into()],
            auth_endpoint: format!("{}/authorize", self.base),
            token_endpoint: format!("{}/token", self.base),
            jwks_uri: format!("{}/jwks", self.base),
            scope: "openid".into(),
            subject: SubjectSource::IdTokenClaim("sub".into()),
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

/// A flow as the browser holds it after `start` / `link`.
pub struct Started {
    pub cookie: String,
    pub state: String,
    pub nonce: String,
    pub challenge: String,
}

/// Status, headers and the `Location` of a raw request.
pub struct Raw {
    pub status: StatusCode,
    pub location: Option<String>,
    pub set_cookie: Option<String>,
    pub body: Value,
}

impl Raw {
    /// The done page's fragment as key → value.
    pub fn fragment(&self) -> HashMap<String, String> {
        let location = self.location.as_deref().expect("a redirect");
        let (path, fragment) = location.split_once('#').expect("a fragment");
        assert_eq!(path, "/auth/oauth/done");
        url::form_urlencoded::parse(fragment.as_bytes())
            .into_owned()
            .collect()
    }
}

impl TestApp {
    /// Like `new`, with Google pointed at `fake`.
    pub async fn with_google() -> (Self, FakeProvider) {
        Self::with_google_opts(false, |_| {}).await
    }

    /// Google pointed at a fresh fake (signing keys cached 1 h); `email` also turns on the
    /// in-memory mailer; `tweak` adjusts the `Config` afterwards (e.g. `c.oauth[0].jwks_ttl`).
    pub async fn with_google_opts(
        email: bool,
        tweak: impl FnOnce(&mut Config),
    ) -> (Self, FakeProvider) {
        let fake = FakeProvider::start().await;
        let config = fake.config(Duration::from_secs(3600));
        let setup = |c: &mut Config| {
            c.oauth = vec![config];
            tweak(c);
        };
        let app = Self::try_new(localdate_api::app, setup, email)
            .await
            .expect("test app setup");
        (app, fake)
    }

    pub async fn raw(
        &self,
        method: Method,
        path: &str,
        cookie: Option<&str>,
        token: Option<&str>,
        body: Option<Value>,
    ) -> Raw {
        let mut builder = Request::builder().method(method).uri(path);
        if let Some(c) = cookie {
            builder = builder.header(header::COOKIE, c);
        }
        if let Some(t) = token {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        let mut req = match body {
            Some(b) => builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string())),
            None => builder.body(Body::empty()),
        }
        .expect("request");
        // One fixed client, so a test that turns the rate limiter on sees a single bucket.
        let peer: std::net::SocketAddr = "10.0.0.1:40000".parse().expect("addr");
        req.extensions_mut()
            .insert(axum::extract::ConnectInfo(peer));
        let resp = self.router.clone().oneshot(req).await.expect("infallible");
        let text = |name| {
            resp.headers()
                .get(name)
                .and_then(|v: &header::HeaderValue| v.to_str().ok())
                .map(str::to_owned)
        };
        let (status, location, set_cookie) = (
            resp.status(),
            text(header::LOCATION),
            text(header::SET_COOKIE),
        );
        let bytes = resp.into_body().collect().await.expect("body").to_bytes();
        Raw {
            status,
            location,
            set_cookie,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        }
    }

    /// `GET start` (login) and what the browser keeps from it.
    pub async fn oauth_start(&self, redirect: Option<&str>) -> Started {
        let path = match redirect {
            Some(r) => format!(
                "/api/auth/oauth/google/start?redirect={}",
                url::form_urlencoded::byte_serialize(r.as_bytes()).collect::<String>()
            ),
            None => "/api/auth/oauth/google/start".into(),
        };
        let resp = self.raw(Method::GET, &path, None, None, None).await;
        assert_eq!(resp.status, StatusCode::FOUND);
        started(&resp, resp.location.as_deref().expect("provider url"))
    }

    /// `POST link` as `token`.
    pub async fn oauth_link(&self, token: &str) -> Started {
        let resp = self
            .raw(
                Method::POST,
                "/api/auth/oauth/google/link",
                None,
                Some(token),
                Some(json!({ "redirect": "/settings" })),
            )
            .await;
        assert_eq!(resp.status, StatusCode::OK, "{}", resp.body);
        let url = resp.body["url"].as_str().expect("url").to_owned();
        started(&resp, &url)
    }

    pub async fn oauth_callback(&self, started: &Started, query: &str) -> Raw {
        self.raw(
            Method::GET,
            &format!("/api/auth/oauth/google/callback?{query}"),
            Some(&started.cookie),
            None,
            None,
        )
        .await
    }

    /// Consent with `claims`, then the provider's redirect back with the right state.
    pub async fn oauth_return(
        &self,
        fake: &FakeProvider,
        started: &Started,
        claims: IdClaims,
    ) -> Raw {
        let code = fake.authorize(started, claims);
        self.oauth_callback(started, &format!("code={code}&state={}", started.state))
            .await
    }

    pub async fn oauth_exchange(&self, cookie: Option<&str>, code: &str) -> Raw {
        self.raw(
            Method::POST,
            "/api/auth/oauth/exchange",
            cookie,
            None,
            Some(json!({ "code": code })),
        )
        .await
    }
}

fn started(resp: &Raw, provider_url: &str) -> Started {
    let set = resp.set_cookie.as_deref().expect("flow cookie");
    let cookie = set.split(';').next().expect("cookie pair").to_owned();
    let url = url::Url::parse(provider_url).expect("provider url");
    let q: HashMap<_, _> = url.query_pairs().into_owned().collect();
    Started {
        cookie,
        state: q["state"].clone(),
        nonce: q["nonce"].clone(),
        challenge: q["code_challenge"].clone(),
    }
}
