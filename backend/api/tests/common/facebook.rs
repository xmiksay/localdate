//! Facebook test double: a local fake of the Graph token endpoint, `/me`, `/me/picture` and the
//! CDN, checking PKCE, the client secret, the bearer token and `appsecret_proof` like Graph does.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::Method;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use localdate_api::auth::oauth::config::{NonceCheck, PictureSource, Userinfo};
use localdate_api::auth::oauth::{OidcConfig, Provider, SubjectSource};
use serde_json::json;
use sha2::{Digest, Sha256};

use super::oauth::{Raw, Started, started};
use super::{TestApp, Tokens, tokens_from};

pub const APP_ID: &str = "fb-app";
pub const APP_SECRET: &str = "fb-test-secret";

/// What `/me/picture` answers.
#[derive(Clone)]
pub enum Picture {
    /// The fake CDN's `/cdn/pic` on 127.0.0.1 (allowlisted in tests).
    Cdn,
    Silhouette,
    /// Any URL, e.g. another host.
    Url(String),
}

/// What one Graph call (`/me`, `/me/picture`) carried.
#[derive(Clone)]
pub struct GraphCall {
    pub token: String,
    pub proof: String,
    pub fields: Option<String>,
    /// The access token also appeared as a query parameter.
    pub token_in_query: bool,
}

#[derive(Clone)]
pub struct FakeFacebook {
    pub base: String,
    /// code → (app-scoped id, PKCE challenge)
    codes: Arc<Mutex<HashMap<String, (String, String)>>>,
    /// access token → app-scoped id
    tokens: Arc<Mutex<HashMap<String, String>>>,
    pub graph_calls: Arc<Mutex<Vec<GraphCall>>>,
    pub picture: Arc<Mutex<Picture>>,
    /// Bytes `/cdn/pic` serves.
    pub image: Arc<Mutex<Vec<u8>>>,
    pub picture_hits: Arc<AtomicUsize>,
    pub cdn_hits: Arc<AtomicUsize>,
}

fn hex_proof(token: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(APP_SECRET.as_bytes()).expect("key");
    mac.update(token.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl FakeFacebook {
    pub async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake facebook");
        let fake = Self {
            base: format!("http://{}", listener.local_addr().expect("addr")),
            codes: Arc::default(),
            tokens: Arc::default(),
            graph_calls: Arc::default(),
            picture: Arc::new(Mutex::new(Picture::Cdn)),
            image: Arc::new(Mutex::new(super::photos::png_bytes(64, 48))),
            picture_hits: Arc::default(),
            cdn_hits: Arc::default(),
        };
        let router = Router::new()
            .route("/token", post(token))
            .route("/me", get(me))
            .route("/me/picture", get(picture))
            .route("/cdn/pic", get(cdn))
            .route(
                "/cdn/redirect",
                get(|| async { Redirect::temporary("/cdn/pic") }),
            )
            .with_state(fake.clone());
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        fake
    }

    pub fn config(&self) -> OidcConfig {
        OidcConfig {
            provider: Provider::Facebook,
            client_id: APP_ID.into(),
            client_secret: APP_SECRET.into(),
            redirect_uri: "https://app.test/api/auth/oauth/facebook/callback".into(),
            issuers: Vec::new(),
            auth_endpoint: format!("{}/dialog/oauth", self.base),
            token_endpoint: format!("{}/token", self.base),
            jwks_uri: String::new(),
            scope: "public_profile".into(),
            subject: SubjectSource::Userinfo(Userinfo {
                endpoint: format!("{}/me", self.base),
                field: "id".into(),
                appsecret_proof: true,
                picture: Some(PictureSource {
                    endpoint: format!("{}/me/picture", self.base),
                    hosts: vec!["127.0.0.1".into()],
                    https_only: false,
                }),
            }),
            nonce: NonceCheck::Required,
            jwks_ttl: Duration::ZERO,
        }
    }

    pub fn set_picture(&self, picture: Picture) {
        *self.picture.lock().expect("lock") = picture;
    }

    pub fn set_image(&self, bytes: Vec<u8>) {
        *self.image.lock().expect("lock") = bytes;
    }

    /// The fake's own address under a name that is not on the allowlist.
    pub fn localhost_url(&self, path: &str) -> String {
        format!("{}{path}", self.base.replace("127.0.0.1", "localhost"))
    }

    /// A successful consent as `id` for the started flow.
    pub fn authorize(&self, started: &Started, id: &str) -> String {
        let code = format!("fbcode-{}", uuid::Uuid::new_v4().simple());
        self.codes
            .lock()
            .expect("lock")
            .insert(code.clone(), (id.into(), started.challenge.clone()));
        code
    }

    /// Graph checks shared by `/me` and `/me/picture`: the caller's id, or the refusal status.
    fn graph_auth(
        &self,
        headers: &HeaderMap,
        query: &HashMap<String, String>,
    ) -> Result<String, StatusCode> {
        let denied = || StatusCode::BAD_REQUEST;
        let token = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(denied)?
            .to_owned();
        let proof = query.get("appsecret_proof").cloned().unwrap_or_default();
        self.graph_calls.lock().expect("lock").push(GraphCall {
            token: token.clone(),
            proof: proof.clone(),
            fields: query.get("fields").cloned(),
            token_in_query: query.contains_key("access_token"),
        });
        if proof != hex_proof(&token) {
            return Err(denied());
        }
        self.tokens
            .lock()
            .expect("lock")
            .get(&token)
            .cloned()
            .ok_or_else(denied)
    }
}

async fn token(
    State(fake): State<FakeFacebook>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let bad = (StatusCode::BAD_REQUEST, Json(json!({ "error": "invalid" }))).into_response();
    let Some((id, challenge)) = form
        .get("code")
        .and_then(|c| fake.codes.lock().expect("lock").remove(c))
    else {
        return bad;
    };
    let verifier = form.get("code_verifier").cloned().unwrap_or_default();
    if URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())) != challenge
        || form.get("client_id").map(String::as_str) != Some(APP_ID)
        || form.get("client_secret").map(String::as_str) != Some(APP_SECRET)
        || !form.contains_key("redirect_uri")
    {
        return bad;
    }
    let access = format!("fbtok-{}", uuid::Uuid::new_v4().simple());
    fake.tokens.lock().expect("lock").insert(access.clone(), id);
    Json(json!({ "access_token": access, "token_type": "bearer", "expires_in": 5183944 }))
        .into_response()
}

async fn me(
    State(fake): State<FakeFacebook>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    match fake.graph_auth(&headers, &query) {
        Ok(id) => Json(json!({ "id": id })).into_response(),
        Err(status) => status.into_response(),
    }
}

async fn picture(
    State(fake): State<FakeFacebook>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    fake.picture_hits.fetch_add(1, Ordering::SeqCst);
    if let Err(status) = fake.graph_auth(&headers, &query) {
        return status.into_response();
    }
    if query.get("redirect").map(String::as_str) != Some("false") {
        return StatusCode::FOUND.into_response();
    }
    let picture = fake.picture.lock().expect("lock").clone();
    let data = match picture {
        Picture::Cdn => json!({ "url": format!("{}/cdn/pic", fake.base), "is_silhouette": false }),
        Picture::Silhouette => {
            json!({ "url": format!("{}/cdn/pic", fake.base), "is_silhouette": true })
        }
        Picture::Url(url) => json!({ "url": url, "is_silhouette": false }),
    };
    Json(json!({ "data": data })).into_response()
}

async fn cdn(State(fake): State<FakeFacebook>) -> Response {
    fake.cdn_hits.fetch_add(1, Ordering::SeqCst);
    let bytes = fake.image.lock().expect("lock").clone();
    ([(header::CONTENT_TYPE, "image/png")], bytes).into_response()
}

impl TestApp {
    /// Like `new`, with Facebook pointed at a fresh fake.
    pub async fn with_facebook() -> (Self, FakeFacebook) {
        let fake = FakeFacebook::start().await;
        let config = fake.config();
        let app = Self::with_config(|c| c.oauth = vec![config]).await;
        (app, fake)
    }

    /// `GET /auth/oauth/facebook/start`, optionally asking for the picture import.
    pub async fn fb_start(&self, import_photo: bool) -> Started {
        let path = if import_photo {
            "/api/auth/oauth/facebook/start?import_photo=1"
        } else {
            "/api/auth/oauth/facebook/start"
        };
        let resp = self.raw(Method::GET, path, None, None, None).await;
        assert_eq!(resp.status, StatusCode::FOUND);
        started(&resp, resp.location.as_deref().expect("provider url"))
    }

    /// `POST /auth/oauth/facebook/link` as `token`.
    pub async fn fb_link(&self, token: &str, import_photo: bool) -> Started {
        let resp = self
            .raw(
                Method::POST,
                "/api/auth/oauth/facebook/link",
                None,
                Some(token),
                Some(json!({ "redirect": "/settings", "import_photo": import_photo })),
            )
            .await;
        assert_eq!(resp.status, StatusCode::OK, "{}", resp.body);
        let url = resp.body["url"].as_str().expect("url").to_owned();
        started(&resp, &url)
    }

    /// `POST /auth/oauth/facebook/import` as `token`, returning to the profile page.
    pub async fn fb_import(&self, token: &str) -> Started {
        let resp = self
            .raw(
                Method::POST,
                "/api/auth/oauth/facebook/import",
                None,
                Some(token),
                Some(json!({ "redirect": "/profile" })),
            )
            .await;
        assert_eq!(resp.status, StatusCode::OK, "{}", resp.body);
        let url = resp.body["url"].as_str().expect("url").to_owned();
        started(&resp, &url)
    }

    /// Start → consent as `id` → exchange → username; the account and the callback's fragment.
    pub async fn fb_signup(
        &self,
        fake: &FakeFacebook,
        id: &str,
        username: &str,
        import_photo: bool,
    ) -> (Tokens, HashMap<String, String>) {
        let started = self.fb_start(import_photo).await;
        let fragment = self.fb_return(fake, &started, id).await.fragment();
        let code = fragment.get("code").expect("code").clone();
        let resp = self.oauth_exchange(Some(&started.cookie), &code).await;
        assert_eq!(resp.status, StatusCode::OK, "{}", resp.body);
        assert_eq!(resp.body["signup"]["provider"], "facebook");
        let token = resp.body["signup"]["token"].as_str().expect("signup token");
        let (status, body) = self
            .post(
                "/api/auth/oauth/signup",
                json!({ "token": token, "username": username }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        (tokens_from(&body), fragment)
    }

    /// Photos stored for `user`.
    pub async fn photo_count(&self, user: uuid::Uuid) -> i64 {
        super::count(
            self,
            &format!("SELECT count(*) FROM photo WHERE user_id = '{user}'"),
        )
        .await
    }

    /// Consent as `id`, then Facebook's redirect back to the callback.
    pub async fn fb_return(&self, fake: &FakeFacebook, started: &Started, id: &str) -> Raw {
        let code = fake.authorize(started, id);
        self.raw(
            Method::GET,
            &format!(
                "/api/auth/oauth/facebook/callback?code={code}&state={}",
                started.state
            ),
            Some(&started.cookie),
            None,
            None,
        )
        .await
    }
}
