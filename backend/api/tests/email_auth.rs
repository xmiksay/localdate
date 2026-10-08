mod common;

use axum::http::StatusCode;
use chrono::Duration;
use common::{TestApp, count, tokens_from};
use localdate_api::cleanup::run_once;
use serde_json::{Value, json};

async fn verify(app: &TestApp, token: &str) -> (StatusCode, Value) {
    app.post("/api/auth/email/verify", json!({ "token": token }))
        .await
}

async fn preview(app: &TestApp, token: &str) -> (StatusCode, Value) {
    app.post("/api/auth/email/preview", json!({ "token": token }))
        .await
}

/// A fresh login token for an address that already has an account.
async fn login_token(app: &TestApp, email: &str) -> String {
    assert_eq!(app.email_start(email).await, StatusCode::ACCEPTED);
    app.last_link(email).await.1
}

#[tokio::test]
async fn providers_reflect_whether_email_is_configured() {
    let off = TestApp::new().await;
    assert_eq!(
        off.get("/api/auth/providers", None).await,
        (StatusCode::OK, json!({ "email": false, "google": false }))
    );
    let on = TestApp::with_email().await;
    assert_eq!(
        on.get("/api/auth/providers", None).await,
        (StatusCode::OK, json!({ "email": true, "google": false }))
    );
}

#[tokio::test]
async fn every_email_endpoint_is_503_when_disabled() {
    let app = TestApp::new().await;
    let t = app.register("alice").await;
    let anon = [
        ("/api/auth/email/start", json!({ "email": "a@b.cz" })),
        ("/api/auth/email/preview", json!({ "token": "x" })),
        ("/api/auth/email/verify", json!({ "token": "x" })),
        (
            "/api/auth/email/signup",
            json!({ "token": "x", "username": "bob" }),
        ),
    ];
    for (path, body) in anon {
        let (status, resp) = app.post(path, body).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{path}");
        assert_eq!(resp["error"]["code"], "email_disabled");
    }
    for (path, body) in [
        ("/api/me/identities/email", json!({ "email": "a@b.cz" })),
        ("/api/me/identities/email/confirm", json!({ "token": "x" })),
    ] {
        let (status, resp) = app.post_as(path, &t.access_token, body).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{path}");
        assert_eq!(resp["error"]["code"], "email_disabled");
    }
}

#[tokio::test]
async fn start_answers_the_same_for_unknown_and_known_addresses() {
    let app = TestApp::with_email().await;
    let (status, unknown) = app
        .post(
            "/api/auth/email/start",
            json!({ "email": " New@Example.CZ " }),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let signup_mail = app
        .mails_to("new@example.cz")
        .await
        .pop()
        .expect("signup mail");
    assert_eq!(signup_mail.subject, "Dokonči registraci do localdate");
    assert!(
        signup_mail
            .text
            .contains("https://app.test/auth/email#token=")
    );
    assert!(!signup_mail.text.contains("?token="));

    app.email_signup("known@example.cz", "known").await;
    let (status, known) = app
        .post(
            "/api/auth/email/start",
            json!({ "email": "known@example.cz" }),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(known, unknown, "response must not reveal the account");
    let login_mail = app
        .mails_to("known@example.cz")
        .await
        .pop()
        .expect("login mail");
    assert_eq!(login_mail.subject, "Přihlášení do localdate");
    assert!(login_mail.text.contains("Pro účet: known"));
    assert!(login_mail.html.contains("Pro účet: known"));
    assert_eq!(app.last_link("known@example.cz").await.0, "/auth/email");
}

#[tokio::test]
async fn only_plain_addresses_are_accepted_and_nothing_is_mailed_otherwise() {
    let app = TestApp::with_email().await;
    let t = app.register("alice").await;
    for bad in [
        "",
        "nope",
        "a@b",
        "a b@c.cz",
        "n<victim@b.cz>",
        "x <victim@b.cz>",
        "\"a\"@b.cz",
        "a,b@c.cz",
        "victim@b.cz,other@b.cz",
        "a@-b.cz",
        ".a@b.cz",
        "a..b@c.cz",
    ] {
        let (status, body) = app
            .post("/api/auth/email/start", json!({ "email": bad }))
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad:?}");
        assert_eq!(body["error"]["code"], "validation");
        let (status, _) = app
            .post_as(
                "/api/me/identities/email",
                &t.access_token,
                json!({ "email": bad }),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "link {bad:?}");
    }
    assert!(app.mails_to("victim@b.cz").await.is_empty());
    assert_eq!(count(&app, "SELECT count(*) FROM email_token").await, 0);
}

#[tokio::test]
async fn mail_language_follows_lang() {
    let app = TestApp::with_email().await;
    app.post(
        "/api/auth/email/start",
        json!({ "email": "en@example.cz", "lang": "en" }),
    )
    .await;
    let mail = app.mails_to("en@example.cz").await.pop().expect("mail");
    assert_eq!(mail.subject, "Finish signing up for localdate");
    assert!(mail.html.contains("https://app.test/auth/email#token="));
}

#[tokio::test]
async fn preview_names_the_account_and_verify_logs_in_once() {
    let app = TestApp::with_email().await;
    let created = app.email_signup("eva@example.cz", "eva").await;
    let token = login_token(&app, "eva@example.cz").await;

    for _ in 0..2 {
        let (status, body) = preview(&app, &token).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            json!({ "purpose": "login", "username": "eva", "email": "eva@example.cz" })
        );
    }
    let (status, body) = verify(&app, &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let t = tokens_from(&body);
    assert_eq!(t.user_id, created.user_id);
    let (status, me) = app.get("/api/me", Some(&t.access_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["user"]["username"], "eva");

    for (status, body) in [verify(&app, &token).await, preview(&app, &token).await] {
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "invalid_token");
    }
    assert_eq!(verify(&app, "not-a-token").await.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn expired_tokens_are_rejected_and_cleaned_up() {
    let app = TestApp::with_email().await;
    app.email_signup("eva@example.cz", "eva").await;
    let token = login_token(&app, "eva@example.cz").await;
    app.sql(
        "UPDATE email_token SET expires_at = now() - interval '1 second' WHERE used_at IS NULL",
    )
    .await;
    for (status, body) in [preview(&app, &token).await, verify(&app, &token).await] {
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "invalid_token");
    }

    // Both tokens (the used sign-up one too) have expired by then.
    let counts = run_once(&app.db, Duration::minutes(16))
        .await
        .expect("cleanup")
        .expect("lock");
    assert_eq!(counts.email_tokens_deleted, 2);
    assert_eq!(count(&app, "SELECT count(*) FROM email_token").await, 0);
}

#[tokio::test]
async fn banned_account_cannot_log_in_and_keeps_the_token_unspent() {
    let app = TestApp::with_email().await;
    let t = app.email_signup("eva@example.cz", "eva").await;
    let token = login_token(&app, "eva@example.cz").await;
    app.sql(&format!(
        "UPDATE \"user\" SET banned_at = now() WHERE id = '{}'",
        t.user_id
    ))
    .await;
    let (status, body) = verify(&app, &token).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "banned");
    assert_eq!(
        count(
            &app,
            "SELECT count(*) FROM email_token WHERE used_at IS NULL"
        )
        .await,
        1,
        "the refused login rolled back"
    );
    assert_eq!(
        count(
            &app,
            &format!(
                "SELECT count(*) FROM refresh_token WHERE user_id = '{}' AND revoked_at IS NULL",
                t.user_id
            )
        )
        .await,
        1,
        "only the session from sign-up"
    );
}

#[tokio::test]
async fn signup_token_is_previewed_then_spent_by_signup() {
    let app = TestApp::with_email().await;
    app.register("taken").await;
    app.email_start("new@example.cz").await;
    let (_, token) = app.last_link("new@example.cz").await;

    let (status, body) = preview(&app, &token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({ "purpose": "signup", "username": null, "email": "new@example.cz" })
    );
    let (status, body) = verify(&app, &token).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "verify is login-only");
    assert_eq!(body["error"]["code"], "invalid_token");

    let signup = |username: &str| {
        app.post(
            "/api/auth/email/signup",
            json!({ "token": token, "username": username }),
        )
    };
    let (status, body) = signup("x").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "validation");
    let (status, body) = signup("Taken").await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "username_taken");

    let (status, body) = signup("Newbie").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["user"]["username"], "newbie");
    let t = tokens_from(&body);
    let (_, ids) = app.get("/api/me/identities", Some(&t.access_token)).await;
    assert_eq!(ids["has_password"], false);
    assert_eq!(ids["identities"][0]["provider"], "email");
    assert_eq!(ids["identities"][0]["subject"], "new@example.cz");
    assert!(ids["identities"][0]["verified_at"].is_string());

    let (status, body) = signup("another").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_token");
}

#[tokio::test]
async fn a_signup_token_dies_once_its_address_has_an_account() {
    let app = TestApp::with_email().await;
    app.email_start("dup@example.cz").await;
    let (_, first) = app.last_link("dup@example.cz").await;
    app.email_start("dup@example.cz").await;
    let (_, second) = app.last_link("dup@example.cz").await;
    let (status, _) = app
        .post(
            "/api/auth/email/signup",
            json!({ "token": first, "username": "one" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = preview(&app, &second).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "fails early, before the form"
    );
    assert_eq!(body["error"]["code"], "invalid_token");
    let (status, body) = app
        .post(
            "/api/auth/email/signup",
            json!({ "token": second, "username": "two" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_token");
    assert_eq!(
        count(&app, "SELECT count(*) FROM \"user\" WHERE username = 'two'").await,
        0
    );
}

#[tokio::test]
async fn at_most_three_mails_per_address_and_client() {
    let app = TestApp::with_email().await;
    for _ in 0..4 {
        assert_eq!(
            app.email_start("spam@example.cz").await,
            StatusCode::ACCEPTED
        );
    }
    assert_eq!(app.mails_to("spam@example.cz").await.len(), 3);
    assert_eq!(
        app.email_start("other@example.cz").await,
        StatusCode::ACCEPTED
    );
    assert_eq!(app.mails_to("other@example.cz").await.len(), 1);
}

#[tokio::test]
async fn passwordless_account_refuses_password_login_like_a_wrong_password() {
    let app = TestApp::with_email().await;
    app.email_signup("eva@example.cz", "eva").await;
    for password in ["dummy password for timing", "correct horse battery", ""] {
        let (status, body) = app
            .post(
                "/api/auth/login",
                json!({ "username": "eva", "password": password }),
            )
            .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{password:?}");
        assert_eq!(body["error"]["code"], "invalid_credentials");
    }
}

#[tokio::test]
async fn link_tokens_preview_but_do_not_log_in() {
    let app = TestApp::with_email().await;
    let t = app.register("alice").await;
    app.post_as(
        "/api/me/identities/email",
        &t.access_token,
        json!({ "email": "alice@example.cz" }),
    )
    .await;
    let mail = app.mails_to("alice@example.cz").await.pop().expect("mail");
    assert!(mail.text.contains("Pro účet: alice"));
    let (path, token) = app.last_link("alice@example.cz").await;
    assert_eq!(path, "/auth/email/link");
    let (status, body) = preview(&app, &token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({ "purpose": "link", "username": "alice", "email": "alice@example.cz" })
    );
    let (status, body) = verify(&app, &token).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_token");
}
