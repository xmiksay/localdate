//! What a password reset or change does to everything issued before it: open sockets and
//! mailed tokens.

mod common;

use axum::http::StatusCode;
use std::time::Duration;

use common::ws::{auth, connect, next, ready_socket};
use common::{PASSWORD, TestApp, Tokens, tokens_from};
use serde_json::json;

const NEW_PASSWORD: &str = "brand new password 1";

/// Waits until the wall clock enters a new second, so tokens issued so far are strictly older
/// than a change made next (`credentials_changed_at` and `iat` have whole-second precision).
async fn next_second() {
    let into = chrono::Utc::now().timestamp_subsec_millis();
    tokio::time::sleep(Duration::from_millis(u64::from(1000 - into.min(999)) + 20)).await;
}

/// The close code a WS auth with `t`'s access token gets instead of `ready`.
async fn ws_refusal(url: &str, t: &Tokens) -> u16 {
    let mut ws = connect(url).await;
    auth(&mut ws, &t.access_token).await;
    next(&mut ws).await.expect_err("socket refused")
}

/// A password account with `email` linked.
async fn linked(app: &TestApp, username: &str, email: &str) -> Tokens {
    let t = app.register(username).await;
    let (status, _) = app
        .post_as(
            "/api/me/identities/email",
            &t.access_token,
            json!({ "email": email }),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let (_, token) = app.last_link(email).await;
    let (status, _) = app
        .post_as(
            "/api/me/identities/email/confirm",
            &t.access_token,
            json!({ "token": token }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    t
}

async fn reset_token(app: &TestApp, login: &str, email: &str) -> String {
    let (status, _) = app
        .post("/api/auth/password/forgot", json!({ "login": login }))
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let (path, token) = app.last_link(email).await;
    assert_eq!(path, "/auth/password/reset");
    token
}

async fn login_token(app: &TestApp, email: &str) -> String {
    assert_eq!(app.email_start(email).await, StatusCode::ACCEPTED);
    app.last_link(email).await.1
}

async fn reset(app: &TestApp, token: &str) -> StatusCode {
    app.post(
        "/api/auth/password/reset",
        json!({ "token": token, "new_password": NEW_PASSWORD }),
    )
    .await
    .0
}

#[tokio::test]
async fn reset_closes_the_accounts_sockets_with_4401() {
    let app = TestApp::with_email().await;
    let alice = linked(&app, "alice", "alice@example.cz").await;
    let bob = app.register("bob").await;
    let url = common::ws::serve(&app.router).await;
    let mut ws_alice = ready_socket(&url, &alice).await;
    let mut ws_bob = ready_socket(&url, &bob).await;

    let token = reset_token(&app, "alice", "alice@example.cz").await;
    assert_eq!(reset(&app, &token).await, StatusCode::NO_CONTENT);
    assert_eq!(next(&mut ws_alice).await, Err(4401));

    // Only that account's sockets: bob's stays open (no frame, no close).
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(300), next(&mut ws_bob))
            .await
            .is_err(),
        "bob's socket stays open and quiet"
    );
}

#[tokio::test]
async fn password_change_refuses_older_access_tokens_but_not_the_fresh_session() {
    let app = TestApp::new().await;
    let alice = app.register("alice").await;
    let url = common::ws::serve(&app.router).await;
    let mut ws = ready_socket(&url, &alice).await;
    next_second().await;

    let (status, body) = app
        .put(
            "/api/me/password",
            &alice.access_token,
            json!({ "current_password": PASSWORD, "new_password": NEW_PASSWORD }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(next(&mut ws).await, Err(4401));

    // Issued in the same second as the change, after it: still accepted.
    let fresh = tokens_from(&body);
    assert_eq!(
        app.get("/api/me", Some(&fresh.access_token)).await.0,
        StatusCode::OK
    );
    drop(ready_socket(&url, &fresh).await);

    let (status, body) = app.get("/api/me", Some(&alice.access_token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "unauthorized");
    assert_eq!(ws_refusal(&url, &alice).await, 4401);
}

#[tokio::test]
async fn reset_refuses_the_accounts_older_access_tokens() {
    let app = TestApp::with_email().await;
    let alice = linked(&app, "alice", "alice@example.cz").await;
    let bob = app.register("bob").await;
    let url = common::ws::serve(&app.router).await;
    next_second().await;

    let token = reset_token(&app, "alice", "alice@example.cz").await;
    assert_eq!(reset(&app, &token).await, StatusCode::NO_CONTENT);
    let (status, body) = app.get("/api/me", Some(&alice.access_token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "unauthorized");
    assert_eq!(ws_refusal(&url, &alice).await, 4401);
    assert_eq!(
        app.get("/api/me", Some(&bob.access_token)).await.0,
        StatusCode::OK
    );

    let (status, body) = app
        .post(
            "/api/auth/login",
            json!({ "username": "alice", "password": NEW_PASSWORD }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let again = tokens_from(&body);
    assert_eq!(
        app.get("/api/me", Some(&again.access_token)).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn periodic_recheck_closes_a_socket_whose_token_a_change_superseded() {
    let app = TestApp::with_config(|c| c.ws_account_recheck = Duration::from_millis(200)).await;
    let alice = app.register("alice").await;
    let url = common::ws::serve(&app.router).await;
    let mut ws = ready_socket(&url, &alice).await;
    // Straight in the DB: no hub disconnect, as if the bridge had lost the Close op.
    app.sql(&format!(
        "UPDATE \"user\" SET credentials_changed_at = date_trunc('second', now()) + interval '1 second' \
         WHERE id = '{}'",
        alice.user_id
    ))
    .await;
    assert_eq!(next(&mut ws).await, Err(4401));
}

#[tokio::test]
async fn reset_voids_every_other_mailed_token_of_the_account() {
    let app = TestApp::with_email().await;
    linked(&app, "alice", "alice@example.cz").await;
    let login = login_token(&app, "alice@example.cz").await;
    let first = reset_token(&app, "alice", "alice@example.cz").await;
    let second = reset_token(&app, "alice", "alice@example.cz").await;

    assert_eq!(reset(&app, &second).await, StatusCode::NO_CONTENT);
    assert_eq!(reset(&app, &first).await, StatusCode::BAD_REQUEST);
    let (status, body) = app
        .post("/api/auth/email/verify", json!({ "token": login }))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_token");
}

#[tokio::test]
async fn password_change_voids_mailed_tokens_but_not_other_accounts() {
    let app = TestApp::with_email().await;
    let alice = linked(&app, "alice", "alice@example.cz").await;
    linked(&app, "bob", "bob@example.cz").await;
    let alice_reset = reset_token(&app, "alice", "alice@example.cz").await;
    let bob_login = login_token(&app, "bob@example.cz").await;

    let (status, _) = app
        .put(
            "/api/me/password",
            &alice.access_token,
            json!({ "current_password": PASSWORD, "new_password": NEW_PASSWORD }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reset(&app, &alice_reset).await, StatusCode::BAD_REQUEST);
    let (status, _) = app
        .post("/api/auth/email/verify", json!({ "token": bob_login }))
        .await;
    assert_eq!(status, StatusCode::OK);
}

async fn push_rows(app: &TestApp, t: &Tokens) -> i64 {
    common::count(
        app,
        &format!(
            "SELECT count(*) FROM push_subscription WHERE user_id = '{}'",
            t.user_id
        ),
    )
    .await
}

/// Two devices of `t`, straight in the DB (the email test app has no push sender).
async fn insert_push_rows(app: &TestApp, t: &Tokens, name: &str) {
    for device in ["phone", "laptop"] {
        app.sql(&format!(
            "INSERT INTO push_subscription (id, user_id, endpoint, p256dh, auth) \
             VALUES (gen_random_uuid(), '{}', 'https://push.example/{name}-{device}', 'k', 'a')",
            t.user_id
        ))
        .await;
    }
}

#[tokio::test]
async fn reset_deletes_the_accounts_push_subscriptions_only() {
    let app = TestApp::with_email().await;
    let alice = linked(&app, "alice", "alice@example.cz").await;
    let bob = app.register("bob").await;
    insert_push_rows(&app, &alice, "alice").await;
    insert_push_rows(&app, &bob, "bob").await;

    let token = reset_token(&app, "alice", "alice@example.cz").await;
    assert_eq!(reset(&app, &token).await, StatusCode::NO_CONTENT);
    assert_eq!(push_rows(&app, &alice).await, 0);
    assert_eq!(push_rows(&app, &bob).await, 2);
}

#[tokio::test]
async fn password_change_drops_subscriptions_and_the_caller_can_resubscribe() {
    let (app, _sent) = TestApp::with_push().await;
    let alice = app.register("alice").await;
    let endpoint = common::push::endpoint("alice-phone");
    let subscribed = app.subscribe_push(&alice, &endpoint, "cs").await;
    assert!(subscribed.is_success(), "{subscribed}");
    assert_eq!(push_rows(&app, &alice).await, 1);

    let (status, body) = app
        .put(
            "/api/me/password",
            &alice.access_token,
            json!({ "current_password": PASSWORD, "new_password": NEW_PASSWORD }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(push_rows(&app, &alice).await, 0);

    // What the client's resync does with the fresh session.
    let fresh = tokens_from(&body);
    assert_eq!(
        app.subscribe_push(&fresh, &endpoint, "cs").await,
        subscribed
    );
    assert_eq!(push_rows(&app, &alice).await, 1);
}
