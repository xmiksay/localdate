mod common;

use axum::http::StatusCode;
use common::{PASSWORD, TestApp, Tokens, tokens_from};
use serde_json::{Value, json};

const NEW_PASSWORD: &str = "brand new password 1";

async fn forgot(app: &TestApp, login: &str) -> StatusCode {
    app.post(
        "/api/auth/password/forgot",
        json!({ "login": login, "lang": "cs" }),
    )
    .await
    .0
}

async fn preview(app: &TestApp, token: &str) -> (StatusCode, Value) {
    app.post(
        "/api/auth/password/reset/preview",
        json!({ "token": token }),
    )
    .await
}

async fn reset(app: &TestApp, token: &str, new_password: &str) -> (StatusCode, Value) {
    app.post(
        "/api/auth/password/reset",
        json!({ "token": token, "new_password": new_password }),
    )
    .await
}

async fn login(app: &TestApp, username: &str, password: &str) -> StatusCode {
    app.post(
        "/api/auth/login",
        json!({ "username": username, "password": password }),
    )
    .await
    .0
}

async fn refresh(app: &TestApp, t: &Tokens) -> StatusCode {
    app.post(
        "/api/auth/refresh",
        json!({ "refresh_token": t.refresh_token }),
    )
    .await
    .0
}

/// A password account with `email` linked through the real link flow.
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
    let (status, body) = app
        .post_as(
            "/api/me/identities/email/confirm",
            &t.access_token,
            json!({ "token": token }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    t
}

/// The token of the newest reset mail to `email`.
async fn reset_token(app: &TestApp, email: &str) -> String {
    let (path, token) = app.last_link(email).await;
    assert_eq!(path, "/auth/password/reset");
    token
}

#[tokio::test]
async fn reset_by_username_sets_the_password_and_logs_every_device_out() {
    let app = TestApp::with_email().await;
    let first = linked(&app, "alice", "alice@example.cz").await;
    let (_, body) = app
        .post(
            "/api/auth/login",
            json!({ "username": "alice", "password": PASSWORD }),
        )
        .await;
    let second = tokens_from(&body);

    assert_eq!(forgot(&app, " Alice ").await, StatusCode::ACCEPTED);
    let mail = app.mails_to("alice@example.cz").await.pop().expect("mail");
    assert_eq!(mail.subject, "Obnovení hesla do localdate");
    assert!(mail.text.contains("Pro účet: alice"), "{}", mail.text);
    let token = reset_token(&app, "alice@example.cz").await;

    for _ in 0..2 {
        assert_eq!(
            preview(&app, &token).await,
            (StatusCode::OK, json!({ "username": "alice" })),
            "preview never consumes"
        );
    }
    assert_eq!(
        reset(&app, &token, NEW_PASSWORD).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        login(&app, "alice", PASSWORD).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(login(&app, "alice", NEW_PASSWORD).await, StatusCode::OK);
    for t in [&first, &second] {
        assert_eq!(refresh(&app, t).await, StatusCode::UNAUTHORIZED);
    }

    let (status, body) = reset(&app, &token, "another password 2").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "single use");
    assert_eq!(body["error"]["code"], "invalid_token");
    assert_eq!(preview(&app, &token).await.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn reset_by_email_mails_that_address_in_the_asked_language() {
    let app = TestApp::with_email().await;
    linked(&app, "alice", "alice@example.cz").await;
    let (status, _) = app
        .post(
            "/api/auth/password/forgot",
            json!({ "login": "Alice@Example.CZ", "lang": "en" }),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let mail = app.mails_to("alice@example.cz").await.pop().expect("mail");
    assert_eq!(mail.subject, "Reset your localdate password");
    assert!(mail.text.contains("For account: alice"));
    let token = reset_token(&app, "alice@example.cz").await;
    assert_eq!(
        reset(&app, &token, NEW_PASSWORD).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(login(&app, "alice", NEW_PASSWORD).await, StatusCode::OK);
}

#[tokio::test]
async fn unknown_or_emailless_accounts_get_the_same_answer_and_no_mail() {
    let app = TestApp::with_email().await;
    app.register("bob").await;
    for who in ["bob", "nobody", "ghost@example.cz"] {
        assert_eq!(forgot(&app, who).await, StatusCode::ACCEPTED, "{who}");
    }
    app.mails_to("ghost@example.cz").await; // waits for the detached lookups
    assert!(app.outbox.sent().is_empty());
}

#[tokio::test]
async fn malformed_login_is_rejected_and_disabled_email_is_503() {
    let app = TestApp::with_email().await;
    for bad in ["", "a b", "eva@", "x"] {
        let (status, body) = app
            .post("/api/auth/password/forgot", json!({ "login": bad }))
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad:?}");
        assert_eq!(body["error"]["code"], "validation");
    }
    let off = TestApp::new().await;
    let (status, body) = off
        .post("/api/auth/password/forgot", json!({ "login": "alice" }))
        .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "email_disabled");
}

async fn reset_mails(app: &TestApp, to: &str) -> usize {
    app.mails_to(to)
        .await
        .iter()
        .filter(|m| m.subject.starts_with("Obnovení"))
        .count()
}

#[tokio::test]
async fn reset_mails_have_their_own_budget() {
    let app = TestApp::with_email().await;
    linked(&app, "alice", "alice@example.cz").await;
    for _ in 0..4 {
        assert_eq!(forgot(&app, "alice").await, StatusCode::ACCEPTED);
    }
    assert_eq!(
        reset_mails(&app, "alice@example.cz").await,
        3,
        "three per (username, client)"
    );
    for _ in 0..3 {
        assert_eq!(forgot(&app, "alice@example.cz").await, StatusCode::ACCEPTED);
    }
    assert_eq!(
        reset_mails(&app, "alice@example.cz").await,
        5,
        "five per account and hour"
    );

    let before = app.mails_to("alice@example.cz").await.len();
    assert_eq!(
        app.email_start("alice@example.cz").await,
        StatusCode::ACCEPTED
    );
    assert_eq!(
        app.mails_to("alice@example.cz").await.len(),
        before + 1,
        "the magic-link budget is untouched"
    );
}

#[tokio::test]
async fn login_and_reset_tokens_do_not_cross() {
    let app = TestApp::with_email().await;
    linked(&app, "alice", "alice@example.cz").await;
    assert_eq!(
        app.email_start("alice@example.cz").await,
        StatusCode::ACCEPTED
    );
    let (_, login_token) = app.last_link("alice@example.cz").await;
    assert_eq!(preview(&app, &login_token).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(
        reset(&app, &login_token, NEW_PASSWORD).await.0,
        StatusCode::BAD_REQUEST
    );
    let (status, _) = app
        .post("/api/auth/email/preview", json!({ "token": login_token }))
        .await;
    assert_eq!(status, StatusCode::OK, "the refusals did not spend it");

    assert_eq!(forgot(&app, "alice").await, StatusCode::ACCEPTED);
    let token = reset_token(&app, "alice@example.cz").await;
    for path in ["/api/auth/email/preview", "/api/auth/email/verify"] {
        let (status, body) = app.post(path, json!({ "token": token })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}");
        assert_eq!(body["error"]["code"], "invalid_token");
    }
    assert_eq!(
        reset(&app, &token, NEW_PASSWORD).await.0,
        StatusCode::NO_CONTENT,
        "refusals elsewhere did not spend it"
    );
    let (status, _) = app
        .post("/api/auth/email/preview", json!({ "token": login_token }))
        .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "voided by the reset itself"
    );
}

#[tokio::test]
async fn expired_token_is_rejected() {
    let app = TestApp::with_email().await;
    linked(&app, "alice", "alice@example.cz").await;
    forgot(&app, "alice").await;
    let token = reset_token(&app, "alice@example.cz").await;
    app.sql("UPDATE email_token SET expires_at = now() - interval '1 second'")
        .await;
    assert_eq!(preview(&app, &token).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(
        reset(&app, &token, NEW_PASSWORD).await.0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn banned_account_cannot_reset_and_keeps_the_token() {
    let app = TestApp::with_email().await;
    let t = linked(&app, "alice", "alice@example.cz").await;
    forgot(&app, "alice").await;
    let token = reset_token(&app, "alice@example.cz").await;
    let set_ban = |on: bool| {
        format!(
            "UPDATE \"user\" SET banned_at = {} WHERE id = '{}'",
            if on { "now()" } else { "NULL" },
            t.user_id
        )
    };
    app.sql(&set_ban(true)).await;
    let (status, body) = reset(&app, &token, NEW_PASSWORD).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "banned");
    assert_eq!(login(&app, "alice", PASSWORD).await, StatusCode::FORBIDDEN);

    app.sql(&set_ban(false)).await;
    assert_eq!(
        reset(&app, &token, NEW_PASSWORD).await.0,
        StatusCode::NO_CONTENT
    );
}

#[tokio::test]
async fn weak_password_is_refused_without_spending_the_token() {
    let app = TestApp::with_email().await;
    linked(&app, "alice", "alice@example.cz").await;
    forgot(&app, "alice").await;
    let token = reset_token(&app, "alice@example.cz").await;
    for bad in ["short".to_owned(), "x".repeat(129)] {
        let (status, body) = reset(&app, &token, &bad).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "validation");
    }
    assert_eq!(
        reset(&app, &token, NEW_PASSWORD).await.0,
        StatusCode::NO_CONTENT
    );
}

#[tokio::test]
async fn unlinking_the_address_kills_its_reset_link() {
    let app = TestApp::with_email().await;
    let t = linked(&app, "alice", "alice@example.cz").await;
    forgot(&app, "alice").await;
    let token = reset_token(&app, "alice@example.cz").await;
    let (_, list) = app.get("/api/me/identities", Some(&t.access_token)).await;
    let id = list["identities"][0]["id"].as_str().expect("identity id");
    let (status, _) = app
        .delete(&format!("/api/me/identities/{id}"), &t.access_token)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(preview(&app, &token).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(
        reset(&app, &token, NEW_PASSWORD).await.0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn passwordless_account_gets_its_first_password_by_reset() {
    let app = TestApp::with_email().await;
    let t = app.email_signup("eva@example.cz", "eva").await;
    forgot(&app, "eva").await;
    let token = reset_token(&app, "eva@example.cz").await;
    assert_eq!(
        reset(&app, &token, NEW_PASSWORD).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(login(&app, "eva", NEW_PASSWORD).await, StatusCode::OK);
    assert_eq!(refresh(&app, &t).await, StatusCode::UNAUTHORIZED);
}
