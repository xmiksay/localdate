mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use common::oauth::FakeProvider;
use localdate_api::cleanup::run_once;
use serde_json::{Value, json};

async fn identities(app: &TestApp, token: &str) -> Value {
    let (status, body) = app.get("/api/me/identities", Some(token)).await;
    assert_eq!(status, StatusCode::OK);
    body
}

#[tokio::test]
async fn link_attaches_google_to_the_caller() {
    let (app, fake) = TestApp::with_google().await;
    let alice = app.register("alice").await;
    let started = app.oauth_link(&alice.access_token).await;
    let resp = app
        .oauth_return(&fake, &started, FakeProvider::claims(&started, "g-alice"))
        .await;
    let fragment = resp.fragment();
    assert_eq!(fragment["linked"], "google");
    assert_eq!(fragment["redirect"], "/settings");
    assert!(resp.set_cookie.expect("cleared").contains("Max-Age=0"));
    let ids = identities(&app, &alice.access_token).await;
    assert_eq!(ids["identities"][0]["provider"], "google");

    // Same Google account again: idempotent.
    let started = app.oauth_link(&alice.access_token).await;
    let resp = app
        .oauth_return(&fake, &started, FakeProvider::claims(&started, "g-alice"))
        .await;
    assert_eq!(resp.fragment()["linked"], "google");
    let ids = identities(&app, &alice.access_token).await;
    assert_eq!(ids["identities"].as_array().map(Vec::len), Some(1));

    // Now Google logs into alice.
    let started = app.oauth_start(None).await;
    let resp = app
        .oauth_return(&fake, &started, FakeProvider::claims(&started, "g-alice"))
        .await;
    let resp = app
        .oauth_exchange(Some(&started.cookie), &resp.fragment()["code"])
        .await;
    assert_eq!(resp.body["session"]["user"]["username"], "alice");
}

#[tokio::test]
async fn google_account_of_another_user_is_identity_taken() {
    let (app, fake) = TestApp::with_google().await;
    let alice = app.register("alice").await;
    let bob = app.register("bob").await;
    let started = app.oauth_link(&alice.access_token).await;
    app.oauth_return(&fake, &started, FakeProvider::claims(&started, "g-1"))
        .await;

    let started = app.oauth_link(&bob.access_token).await;
    let resp = app
        .oauth_return(&fake, &started, FakeProvider::claims(&started, "g-1"))
        .await;
    assert_eq!(resp.fragment()["error"], "identity_taken");
    let ids = identities(&app, &bob.access_token).await;
    assert_eq!(ids["identities"], json!([]));
}

#[tokio::test]
async fn link_needs_auth_and_a_live_account() {
    let (app, fake) = TestApp::with_google().await;
    let resp = app
        .raw(
            Method::POST,
            "/api/auth/oauth/google/link",
            None,
            None,
            Some(json!({})),
        )
        .await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);

    let alice = app.register("alice").await;
    let started = app.oauth_link(&alice.access_token).await;
    app.sql("UPDATE \"user\" SET banned_at = now() WHERE username = 'alice'")
        .await;
    let resp = app
        .oauth_return(&fake, &started, FakeProvider::claims(&started, "g-1"))
        .await;
    assert_eq!(resp.fragment()["error"], "banned");

    let carol = app.register("carol").await;
    let started = app.oauth_link(&carol.access_token).await;
    app.sql("DELETE FROM \"user\" WHERE username = 'carol'")
        .await;
    let resp = app
        .oauth_return(&fake, &started, FakeProvider::claims(&started, "g-2"))
        .await;
    assert_eq!(resp.fragment()["error"], "unauthorized");
    assert_eq!(
        common::count(&app, "SELECT count(*) FROM user_identity").await,
        0
    );
}

#[tokio::test]
async fn password_change_after_link_start_voids_the_flow() {
    let (app, fake) = TestApp::with_google().await;
    let alice = app.register("alice").await;
    let started = app.oauth_link(&alice.access_token).await;
    // As a reset/change committed after the token that started the flow (past #18's same-second grace).
    app.sql("UPDATE \"user\" SET credentials_changed_at = date_trunc('second', now()) + interval '2 seconds'")
        .await;
    let resp = app
        .oauth_return(&fake, &started, FakeProvider::claims(&started, "g-1"))
        .await;
    assert_eq!(resp.fragment()["error"], "unauthorized");
    assert_eq!(
        common::count(&app, "SELECT count(*) FROM user_identity").await,
        0
    );
}

#[tokio::test]
async fn unsafe_redirect_is_dropped() {
    let (app, fake) = TestApp::with_google().await;
    let started = app.oauth_start(Some("//evil.example")).await;
    let resp = app
        .oauth_return(&fake, &started, FakeProvider::claims(&started, "g-1"))
        .await;
    let fragment = resp.fragment();
    assert!(fragment.contains_key("code"));
    assert!(!fragment.contains_key("redirect"));
}

#[tokio::test]
async fn last_google_identity_of_a_passwordless_account_cannot_be_unlinked() {
    let (app, fake) = TestApp::with_google().await;
    let started = app.oauth_start(None).await;
    let resp = app
        .oauth_return(&fake, &started, FakeProvider::claims(&started, "g-1"))
        .await;
    let resp = app
        .oauth_exchange(Some(&started.cookie), &resp.fragment()["code"])
        .await;
    let token = resp.body["signup"]["token"]
        .as_str()
        .expect("token")
        .to_owned();
    let (_, body) = app
        .post(
            "/api/auth/oauth/signup",
            json!({ "token": token, "username": "gina" }),
        )
        .await;
    let access = body["access_token"].as_str().expect("access").to_owned();
    let ids = identities(&app, &access).await;
    let id = ids["identities"][0]["id"].as_str().expect("id").to_owned();
    let (status, body) = app
        .delete(&format!("/api/me/identities/{id}"), &access)
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "last_login_method");
}

#[tokio::test]
async fn cleanup_drops_expired_grants() {
    let (app, fake) = TestApp::with_google().await;
    let started = app.oauth_start(None).await;
    app.oauth_return(&fake, &started, FakeProvider::claims(&started, "g-1"))
        .await;
    let counts = run_once(&app.db, chrono::Duration::minutes(2))
        .await
        .expect("cleanup")
        .expect("lock");
    assert_eq!(counts.oauth_grants_deleted, 1);
}
