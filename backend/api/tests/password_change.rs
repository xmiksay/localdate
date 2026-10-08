mod common;

use std::net::SocketAddr;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Method, Request, StatusCode, header};
use common::{PASSWORD, TestApp, Tokens, tokens_from};
use serde_json::{Value, json};
use tower::ServiceExt;

const NEW_PASSWORD: &str = "brand new password 1";

async fn change(app: &TestApp, t: &Tokens, body: Value) -> (StatusCode, Value) {
    app.put("/api/me/password", &t.access_token, body).await
}

async fn login(app: &TestApp, username: &str, password: &str) -> (StatusCode, Value) {
    app.post(
        "/api/auth/login",
        json!({ "username": username, "password": password }),
    )
    .await
}

async fn refresh(app: &TestApp, t: &Tokens) -> StatusCode {
    app.post(
        "/api/auth/refresh",
        json!({ "refresh_token": t.refresh_token }),
    )
    .await
    .0
}

async fn has_password(app: &TestApp, t: &Tokens) -> Value {
    let (status, body) = app.get("/api/me/identities", Some(&t.access_token)).await;
    assert_eq!(status, StatusCode::OK);
    body["has_password"].clone()
}

#[tokio::test]
async fn change_with_the_current_password_replaces_every_session() {
    let app = TestApp::new().await;
    let first = app.register("alice").await;
    let second = tokens_from(&login(&app, "alice", PASSWORD).await.1);

    let (status, body) = change(
        &app,
        &first,
        json!({ "current_password": PASSWORD, "new_password": NEW_PASSWORD }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let fresh = tokens_from(&body);
    assert_eq!(body["user"]["username"], "alice");

    for old in [&first, &second] {
        assert_eq!(refresh(&app, old).await, StatusCode::UNAUTHORIZED);
    }
    assert_eq!(refresh(&app, &fresh).await, StatusCode::OK);
    assert_eq!(
        login(&app, "alice", PASSWORD).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(login(&app, "alice", NEW_PASSWORD).await.0, StatusCode::OK);
}

#[tokio::test]
async fn missing_or_wrong_current_password_changes_nothing() {
    let app = TestApp::new().await;
    let t = app.register("alice").await;
    let (status, body) = change(&app, &t, json!({ "new_password": NEW_PASSWORD })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "validation");

    for wrong in ["wrong password 1".to_owned(), "x".repeat(200)] {
        let (status, body) = change(
            &app,
            &t,
            json!({ "current_password": wrong, "new_password": NEW_PASSWORD }),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "invalid_credentials");
    }
    assert_eq!(login(&app, "alice", PASSWORD).await.0, StatusCode::OK);
    assert_eq!(refresh(&app, &t).await, StatusCode::OK, "sessions kept");
}

#[tokio::test]
async fn new_password_follows_the_register_policy() {
    let app = TestApp::new().await;
    let t = app.register("alice").await;
    for bad in ["short".to_owned(), "x".repeat(129)] {
        let (status, body) = change(
            &app,
            &t,
            json!({ "current_password": PASSWORD, "new_password": bad }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "validation");
    }
    let (status, _) = app
        .request(
            Method::PUT,
            "/api/me/password",
            None,
            Some(json!({ "new_password": NEW_PASSWORD })),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn passwordless_account_sets_a_first_password_without_current() {
    let app = TestApp::with_email().await;
    let t = app.email_signup("eva@example.cz", "eva").await;
    assert_eq!(has_password(&app, &t).await, json!(false));
    let (status, body) = change(&app, &t, json!({ "new_password": NEW_PASSWORD })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let fresh = tokens_from(&body);
    assert_eq!(has_password(&app, &fresh).await, json!(true));
    assert_eq!(login(&app, "eva", NEW_PASSWORD).await.0, StatusCode::OK);
    assert_eq!(refresh(&app, &t).await, StatusCode::UNAUTHORIZED);

    // From now on the current password is required.
    let (status, _) = change(&app, &fresh, json!({ "new_password": "third password 3" })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// A wrong-password change from one client address, as the real server would see it.
async fn change_from_client(app: &TestApp, t: &Tokens) -> StatusCode {
    let body = json!({ "current_password": "wrong password 1", "new_password": NEW_PASSWORD });
    let mut req = Request::builder()
        .method(Method::PUT)
        .uri("/api/me/password")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {}", t.access_token))
        .body(Body::from(body.to_string()))
        .expect("build request");
    let peer: SocketAddr = "10.0.0.7:40000".parse().expect("addr");
    req.extensions_mut().insert(ConnectInfo(peer));
    app.router
        .clone()
        .oneshot(req)
        .await
        .expect("infallible")
        .status()
}

#[tokio::test]
async fn guessing_the_current_password_is_rate_limited() {
    let app = TestApp::with_config(|c| c.rate_limit = true).await;
    let t = app.register("alice").await;
    let mut statuses = Vec::new();
    for _ in 0..6 {
        statuses.push(change_from_client(&app, &t).await);
    }
    assert!(
        statuses.contains(&StatusCode::TOO_MANY_REQUESTS),
        "{statuses:?}"
    );
}
