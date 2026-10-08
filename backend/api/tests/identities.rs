mod common;

use axum::http::StatusCode;
use common::{TestApp, Tokens, tokens_from};
use serde_json::{Value, json};

async fn request_link(app: &TestApp, t: &Tokens, email: &str) {
    let (status, body) = app
        .post_as(
            "/api/me/identities/email",
            &t.access_token,
            json!({ "email": email }),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
}

async fn confirm(app: &TestApp, t: &Tokens, token: &str) -> (StatusCode, Value) {
    app.post_as(
        "/api/me/identities/email/confirm",
        &t.access_token,
        json!({ "token": token }),
    )
    .await
}

async fn identities(app: &TestApp, t: &Tokens) -> Value {
    let (status, body) = app.get("/api/me/identities", Some(&t.access_token)).await;
    assert_eq!(status, StatusCode::OK);
    body
}

#[tokio::test]
async fn password_account_starts_without_identities() {
    let app = TestApp::with_email().await;
    let t = app.register("alice").await;
    assert_eq!(
        identities(&app, &t).await,
        json!({ "has_password": true, "identities": [] })
    );
    let (status, _) = app.get("/api/me/identities", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn linked_email_becomes_a_login_method() {
    let app = TestApp::with_email().await;
    let t = app.register("alice").await;
    request_link(&app, &t, " Alice@Example.cz").await;
    let (path, token) = app.last_link("alice@example.cz").await;
    assert_eq!(path, "/auth/email/link");

    let (status, identity) = confirm(&app, &t, &token).await;
    assert_eq!(status, StatusCode::CREATED, "{identity}");
    assert_eq!(identity["provider"], "email");
    assert_eq!(identity["subject"], "alice@example.cz");
    let list = identities(&app, &t).await;
    assert_eq!(list["identities"].as_array().map(Vec::len), Some(1));
    assert_eq!(list["identities"][0]["id"], identity["id"]);

    let (status, _) = confirm(&app, &t, &token).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "single use");

    app.email_start("alice@example.cz").await;
    let (_, login) = app.last_link("alice@example.cz").await;
    let (status, body) = app
        .post("/api/auth/email/verify", json!({ "token": login }))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tokens_from(&body).user_id, t.user_id);
}

#[tokio::test]
async fn another_account_cannot_confirm_a_link_and_does_not_burn_it() {
    let app = TestApp::with_email().await;
    let alice = app.register("alice").await;
    let mallory = app.register("mallory").await;
    request_link(&app, &alice, "alice@example.cz").await;
    let (_, token) = app.last_link("alice@example.cz").await;

    let (status, body) = confirm(&app, &mallory, &token).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_token");
    assert_eq!(identities(&app, &mallory).await["identities"], json!([]));

    let (status, _) = confirm(&app, &alice, &token).await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn linking_a_taken_address_sends_a_notice_not_a_link() {
    let app = TestApp::with_email().await;
    let owner = app.email_signup("eva@example.cz", "eva").await;
    let other = app.register("alice").await;
    for t in [&other, &owner] {
        request_link(&app, t, "eva@example.cz").await;
        let notice = app.mails_to("eva@example.cz").await.pop().expect("notice");
        assert_eq!(notice.subject, "Tento e-mail už je propojený");
        assert!(!notice.text.contains(common::email::BASE_URL));
    }
}

#[tokio::test]
async fn login_tokens_cannot_confirm_a_link() {
    let app = TestApp::with_email().await;
    let t = app.email_signup("eva@example.cz", "eva").await;
    app.email_start("eva@example.cz").await;
    let (_, login) = app.last_link("eva@example.cz").await;
    let (status, _) = confirm(&app, &t, &login).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_last_login_method_cannot_be_removed() {
    let app = TestApp::with_email().await;
    let eva = app.email_signup("eva@example.cz", "eva").await;
    let only = identities(&app, &eva).await["identities"][0]["id"]
        .as_str()
        .expect("id")
        .to_owned();
    let path = format!("/api/me/identities/{only}");
    let (status, body) = app.delete(&path, &eva.access_token).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "last_login_method");

    // With a second address the first can go.
    request_link(&app, &eva, "eva2@example.cz").await;
    let (_, token) = app.last_link("eva2@example.cz").await;
    assert_eq!(confirm(&app, &eva, &token).await.0, StatusCode::CREATED);
    let (status, _) = app.delete(&path, &eva.access_token).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let list = identities(&app, &eva).await;
    assert_eq!(list["identities"][0]["subject"], "eva2@example.cz");
    assert_eq!(list["identities"].as_array().map(Vec::len), Some(1));
}

#[tokio::test]
async fn password_account_may_drop_its_email_and_others_cannot_touch_it() {
    let app = TestApp::with_email().await;
    let alice = app.register("alice").await;
    let bob = app.register("bob").await;
    request_link(&app, &alice, "alice@example.cz").await;
    let (_, token) = app.last_link("alice@example.cz").await;
    let (_, identity) = confirm(&app, &alice, &token).await;
    let path = format!(
        "/api/me/identities/{}",
        identity["id"].as_str().expect("id")
    );

    assert_eq!(
        app.delete(&path, &bob.access_token).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.delete("/api/me/identities/nope", &alice.access_token)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.delete(&path, &alice.access_token).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(identities(&app, &alice).await["identities"], json!([]));

    // The address is free again: a start now mails a sign-up link, not a login one.
    app.email_start("alice@example.cz").await;
    let mail = app.mails_to("alice@example.cz").await.pop().expect("mail");
    assert_eq!(mail.subject, "Dokonči registraci do localdate");
}

#[tokio::test]
async fn login_link_dies_when_the_address_is_unlinked() {
    let app = TestApp::with_email().await;
    let alice = app.register("alice").await;
    request_link(&app, &alice, "alice@example.cz").await;
    let (_, token) = app.last_link("alice@example.cz").await;
    let (_, identity) = confirm(&app, &alice, &token).await;
    app.email_start("alice@example.cz").await;
    let (_, login) = app.last_link("alice@example.cz").await;
    let path = format!(
        "/api/me/identities/{}",
        identity["id"].as_str().expect("id")
    );
    app.delete(&path, &alice.access_token).await;

    let (status, body) = app
        .post("/api/auth/email/verify", json!({ "token": login }))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_token");
}
