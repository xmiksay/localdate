mod common;

use axum::http::StatusCode;
use common::{PASSWORD, TestApp, tokens_from};
use serde_json::json;

#[tokio::test]
async fn register_and_login_happy_path() {
    let app = TestApp::new().await;
    let (status, body) = app
        .post(
            "/api/auth/register",
            json!({ "username": "  Alice_1 ", "password": PASSWORD }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["user"]["username"], "Alice_1");
    assert!(body["user"]["created_at"].is_string());
    assert!(body["user"].get("password_hash").is_none());

    let (status, body) = app
        .post(
            "/api/auth/login",
            json!({ "username": "ALICE_1", "password": PASSWORD }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["access_token"].as_str().is_some_and(|t| !t.is_empty()));
    assert!(
        body["refresh_token"]
            .as_str()
            .is_some_and(|t| !t.is_empty())
    );
}

#[tokio::test]
async fn register_validates_input() {
    let app = TestApp::new().await;
    for body in [
        json!({ "username": "   ", "password": PASSWORD }),
        json!({ "username": "a\nb", "password": PASSWORD }),
        json!({ "username": "x".repeat(65), "password": PASSWORD }),
        json!({ "username": "valid name", "password": "sixsix" }),
    ] {
        let (status, resp) = app.post("/api/auth/register", body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(resp["error"]["code"], "validation");
    }
    let (status, resp) = app
        .post("/api/auth/register", json!({ "username": 5 }))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(resp["error"]["code"], "validation");
}

#[tokio::test]
async fn duplicate_username_conflicts() {
    let app = TestApp::new().await;
    app.register("alice").await;
    let (status, body) = app
        .post(
            "/api/auth/register",
            json!({ "username": "Alice", "password": PASSWORD }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "username_taken");
}

#[tokio::test]
async fn bad_credentials_are_rejected() {
    let app = TestApp::new().await;
    app.register("alice").await;
    for body in [
        json!({ "username": "alice", "password": "wrong password!" }),
        json!({ "username": "nobody", "password": PASSWORD }),
    ] {
        let (status, resp) = app.post("/api/auth/login", body).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(resp["error"]["code"], "invalid_credentials");
    }
}

#[tokio::test]
async fn refresh_rotates_tokens() {
    let app = TestApp::new().await;
    let first = app.register("alice").await;
    let (status, body) = app
        .post(
            "/api/auth/refresh",
            json!({ "refresh_token": first.refresh_token }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let second = tokens_from(&body);
    assert_ne!(second.refresh_token, first.refresh_token);
    assert_eq!(second.user_id, first.user_id);

    // The new token keeps working.
    let (status, _) = app
        .post(
            "/api/auth/refresh",
            json!({ "refresh_token": second.refresh_token }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn refresh_reuse_revokes_family() {
    let app = TestApp::new().await;
    let first = app.register("alice").await;
    let (_, body) = app
        .post(
            "/api/auth/refresh",
            json!({ "refresh_token": first.refresh_token }),
        )
        .await;
    let second = tokens_from(&body);

    let (status, resp) = app
        .post(
            "/api/auth/refresh",
            json!({ "refresh_token": first.refresh_token }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(resp["error"]["code"], "invalid_refresh_token");

    // The legitimate successor is now dead too.
    let (status, resp) = app
        .post(
            "/api/auth/refresh",
            json!({ "refresh_token": second.refresh_token }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(resp["error"]["code"], "invalid_refresh_token");
}

#[tokio::test]
async fn refresh_with_unknown_token_is_rejected() {
    let app = TestApp::new().await;
    let (status, resp) = app
        .post("/api/auth/refresh", json!({ "refresh_token": "nope" }))
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(resp["error"]["code"], "invalid_refresh_token");
}

#[tokio::test]
async fn logout_revokes_refresh_token() {
    let app = TestApp::new().await;
    let t = app.register("alice").await;
    let (status, _) = app
        .post(
            "/api/auth/logout",
            json!({ "refresh_token": t.refresh_token }),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, resp) = app
        .post(
            "/api/auth/refresh",
            json!({ "refresh_token": t.refresh_token }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(resp["error"]["code"], "invalid_refresh_token");

    // Logging out with an unknown token is a no-op.
    let (status, _) = app
        .post("/api/auth/logout", json!({ "refresh_token": "unknown" }))
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn protected_routes_need_a_valid_token() {
    use axum::Router;
    use axum::routing::get;
    use localdate_api::auth::AuthUser;

    let app = TestApp::new().await;
    let t = app.register("alice").await;

    // No protected domain route exists yet; mount a probe behind the real extractor.
    let probe = Router::new()
        .route(
            "/whoami",
            get(|u: AuthUser| async move { u.id.to_string() }),
        )
        .with_state(app.state.clone());
    let call = |auth: Option<String>| {
        let probe = probe.clone();
        async move {
            use axum::body::Body;
            use axum::http::{Request, header};
            use tower::ServiceExt;
            let mut req = Request::builder().uri("/whoami");
            if let Some(a) = auth {
                req = req.header(header::AUTHORIZATION, a);
            }
            probe
                .oneshot(req.body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status()
        }
    };
    assert_eq!(call(None).await, StatusCode::UNAUTHORIZED);
    assert_eq!(
        call(Some("Bearer garbage".into())).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(Some(format!("Bearer {}", t.access_token))).await,
        StatusCode::OK
    );
}
