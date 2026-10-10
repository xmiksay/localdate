//! Free-form usernames: case-insensitive (and NFC) uniqueness and lookup by `username_key`, forgot by a username
//! that looks like an address, and the 7-character password minimum.

mod common;

use axum::http::StatusCode;
use common::{PASSWORD, TestApp};
use serde_json::{Value, json};

async fn register(app: &TestApp, username: &str, password: &str) -> (StatusCode, Value) {
    app.post(
        "/api/auth/register",
        json!({ "username": username, "password": password }),
    )
    .await
}

async fn login(app: &TestApp, username: &str) -> (StatusCode, Value) {
    app.post(
        "/api/auth/login",
        json!({ "username": username, "password": PASSWORD }),
    )
    .await
}

async fn forgot(app: &TestApp, login: &str) {
    let (status, _) = app
        .post("/api/auth/password/forgot", json!({ "login": login }))
        .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{login}");
}

#[tokio::test]
async fn names_differing_in_case_or_composition_collide() {
    let app = TestApp::new().await;
    assert_eq!(
        register(&app, "Petr", PASSWORD).await.0,
        StatusCode::CREATED
    );
    for name in ["Novák", "Řeka"] {
        assert_eq!(register(&app, name, PASSWORD).await.0, StatusCode::CREATED);
    }
    for taken in ["petr", " PETR ", "Nova\u{0301}k", "NOVÁK", "řeka", "ŘEKA"] {
        let (status, body) = register(&app, taken, PASSWORD).await;
        assert_eq!(status, StatusCode::CONFLICT, "{taken:?}");
        assert_eq!(body["error"]["code"], "username_taken");
    }
}

#[tokio::test]
async fn login_ignores_case_and_keeps_the_display_form() {
    let app = TestApp::new().await;
    let (status, body) = register(&app, "  Petr Novák 🦊 ", PASSWORD).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["user"]["username"], "Petr Novák 🦊");
    for typed in ["petr novák 🦊", "PETR NOVÁK 🦊", "Petr Nova\u{0301}k 🦊"] {
        let (status, body) = login(&app, typed).await;
        assert_eq!(status, StatusCode::OK, "{typed:?}");
        assert_eq!(body["user"]["username"], "Petr Novák 🦊");
    }
    // Non-ASCII case folding comes from the app's key, not the database locale.
    register(&app, "Řeka", PASSWORD).await;
    let (status, body) = login(&app, "ŘEKA").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["username"], "Řeka");
    assert_eq!(login(&app, "řeka").await.0, StatusCode::OK);
}

#[tokio::test]
async fn password_minimum_is_seven_characters() {
    let app = TestApp::new().await;
    let (status, body) = register(&app, "six", "123456").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "validation");
    assert_eq!(
        register(&app, "seven", "1234567").await.0,
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn admin_cli_finds_the_name_in_any_case() {
    let app = TestApp::new().await;
    app.register("Eva Nová").await;
    localdate_api::admin::set_admin(&app.db, "  EVA NOVÁ ", true)
        .await
        .expect("granted");
    assert_eq!(
        common::count(&app, r#"SELECT count(*) FROM "user" WHERE is_admin"#).await,
        1
    );
    assert!(
        localdate_api::admin::set_admin(&app.db, "nobody", true)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn forgot_by_a_username_with_at_reaches_the_account_and_the_address() {
    let app = TestApp::with_email().await;
    // The username looks like an address but is not this account's email.
    app.linked("Eva@Example.cz", "eva.mail@example.cz").await;
    // A different account owns the address the username spells.
    app.linked("bob", "eva@example.cz").await;
    forgot(&app, "eva@EXAMPLE.cz").await;
    for to in ["eva.mail@example.cz", "eva@example.cz"] {
        let (path, _) = app.last_link(to).await;
        assert_eq!(path, "/auth/password/reset", "{to}");
    }
}

#[tokio::test]
async fn forgot_mails_an_address_once_when_username_and_email_agree() {
    let app = TestApp::with_email().await;
    app.linked("eva@example.cz", "eva@example.cz").await;
    let before = app.mails_to("eva@example.cz").await.len();
    forgot(&app, "eva@example.cz").await;
    assert_eq!(app.mails_to("eva@example.cz").await.len(), before + 1);
}
