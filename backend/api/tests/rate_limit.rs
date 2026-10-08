mod common;

use std::net::SocketAddr;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Method, Request, StatusCode, header};
use common::TestApp;
use serde_json::json;
use tower::ServiceExt;

/// Failed login from the single ingress peer, claiming client `xff`.
async fn login_via_proxy(app: &TestApp, xff: &str) -> StatusCode {
    let mut req = Request::builder()
        .method(Method::POST)
        .uri("/api/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-forwarded-for", xff)
        .body(Body::from(
            json!({ "username": "nobody", "password": "wrong password" }).to_string(),
        ))
        .expect("build request");
    // oneshot has no real socket; this is what `into_make_service_with_connect_info` would add.
    let peer: SocketAddr = "10.0.0.1:40000".parse().expect("addr");
    req.extensions_mut().insert(ConnectInfo(peer));
    app.router
        .clone()
        .oneshot(req)
        .await
        .expect("infallible")
        .status()
}

async fn app(trust_proxy_headers: bool) -> TestApp {
    TestApp::with_config(|c| {
        c.rate_limit = true;
        c.trust_proxy_headers = trust_proxy_headers;
    })
    .await
}

/// Burst is 5: the sixth request from the same bucket is throttled.
async fn exhaust(app: &TestApp, xff: &str) {
    for _ in 0..5 {
        assert_eq!(login_via_proxy(app, xff).await, StatusCode::UNAUTHORIZED);
    }
    assert_eq!(
        login_via_proxy(app, xff).await,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn trusted_forwarded_for_gives_each_client_its_own_bucket() {
    let app = app(true).await;
    exhaust(&app, "203.0.113.1").await;
    assert_eq!(
        login_via_proxy(&app, "203.0.113.2").await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn untrusted_forwarded_for_is_ignored() {
    let app = app(false).await;
    exhaust(&app, "203.0.113.1").await;
    assert_eq!(
        login_via_proxy(&app, "203.0.113.2").await,
        StatusCode::TOO_MANY_REQUESTS
    );
}
