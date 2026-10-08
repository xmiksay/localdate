//! Browser side of the OAuth flow against the fake provider (`oidc_fake`): start / link, the
//! provider's redirect back, the done-page fragment and `exchange`.

use std::collections::HashMap;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use localdate_api::auth::oauth::Provider;
use serde_json::{Value, json};
use tower::ServiceExt;

use localdate_api::config::Config;

use super::TestApp;

pub use super::oidc_fake::{FakeProvider, IdClaims};

/// A flow as the browser holds it after `start` / `link`.
pub struct Started {
    pub cookie: String,
    pub state: String,
    pub nonce: String,
    pub challenge: String,
    pub scope: String,
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
        Self::with_oauth(Provider::Google, email, tweak).await
    }

    /// `provider` pointed at a fresh fake (keys cached 1 h), like [`with_google_opts`].
    pub async fn with_oauth(
        provider: Provider,
        email: bool,
        tweak: impl FnOnce(&mut Config),
    ) -> (Self, FakeProvider) {
        let fake = FakeProvider::start().await;
        let config = fake.config_for(provider, Duration::from_secs(3600));
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
        self.oauth_start_as(Provider::Google, redirect).await
    }

    pub async fn oauth_start_as(&self, provider: Provider, redirect: Option<&str>) -> Started {
        let start = format!("/api/auth/oauth/{}/start", provider.as_str());
        let path = match redirect {
            Some(r) => format!(
                "{start}?redirect={}",
                url::form_urlencoded::byte_serialize(r.as_bytes()).collect::<String>()
            ),
            None => start,
        };
        let resp = self.raw(Method::GET, &path, None, None, None).await;
        assert_eq!(resp.status, StatusCode::FOUND);
        started(&resp, resp.location.as_deref().expect("provider url"))
    }

    /// `POST link` as `token`.
    pub async fn oauth_link(&self, token: &str) -> Started {
        self.oauth_link_as(Provider::Google, token).await
    }

    pub async fn oauth_link_as(&self, provider: Provider, token: &str) -> Started {
        let resp = self
            .raw(
                Method::POST,
                &format!("/api/auth/oauth/{}/link", provider.as_str()),
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
        self.oauth_callback_as(Provider::Google, started, query)
            .await
    }

    pub async fn oauth_callback_as(
        &self,
        provider: Provider,
        started: &Started,
        query: &str,
    ) -> Raw {
        self.raw(
            Method::GET,
            &format!("/api/auth/oauth/{}/callback?{query}", provider.as_str()),
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
        self.oauth_return_as(Provider::Google, fake, started, claims)
            .await
    }

    pub async fn oauth_return_as(
        &self,
        provider: Provider,
        fake: &FakeProvider,
        started: &Started,
        claims: IdClaims,
    ) -> Raw {
        let code = fake.authorize(started, claims);
        let query = format!("code={code}&state={}", started.state);
        self.oauth_callback_as(provider, started, &query).await
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
        scope: q.get("scope").cloned().unwrap_or_default(),
    }
}
