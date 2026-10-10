mod common;

use axum::http::StatusCode;
use common::oauth::FakeProvider;
use common::{TestApp, Tokens};
use serde_json::json;

const NEW_PASSWORD: &str = "brand new password 1";

async fn forgot(app: &TestApp, login: &str, lang: &str) -> StatusCode {
    app.post(
        "/api/auth/password/forgot",
        json!({ "login": login, "lang": lang }),
    )
    .await
    .0
}

/// The token in the newest bot message to `chat`, checking it is a reset link.
async fn reset_token(app: &TestApp, chat: i64) -> String {
    let text = app.bot_messages_to(chat).await.pop().expect("a message");
    let link = text
        .split_whitespace()
        .find(|w| w.starts_with(common::email::BASE_URL))
        .expect("the message has a link");
    let (path, token) = link[common::email::BASE_URL.len()..]
        .split_once("#token=")
        .expect("token in the fragment");
    assert_eq!(path, "/auth/password/reset");
    token.to_owned()
}

/// A password account with Telegram user `chat` linked through the real OAuth link flow.
async fn linked(app: &TestApp, fake: &FakeProvider, username: &str, chat: i64) -> Tokens {
    let t = app.register(username).await;
    let fragment = app.telegram_link(fake, &t, chat).await;
    assert_eq!(fragment.get("linked").map(String::as_str), Some("telegram"));
    t
}

#[tokio::test]
async fn username_reset_link_arrives_by_telegram_and_works() {
    let (app, fake) = TestApp::with_telegram(false, true).await;
    linked(&app, &fake, "eva", 501).await;
    assert_eq!(forgot(&app, "Eva", "en").await, StatusCode::ACCEPTED);
    let text = app.bot_messages_to(501).await.pop().expect("message");
    assert!(text.contains("eva") && text.contains("password"), "{text}");
    let token = reset_token(&app, 501).await;

    let (status, body) = app
        .post(
            "/api/auth/password/reset/preview",
            json!({ "token": token }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["username"], "eva");
    let (status, _) = app
        .post(
            "/api/auth/password/reset",
            json!({ "token": token, "new_password": NEW_PASSWORD }),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = app
        .post(
            "/api/auth/login",
            json!({ "username": "eva", "password": NEW_PASSWORD }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn telegram_only_server_still_offers_reset_but_not_by_address() {
    let (app, _fake) = TestApp::with_telegram(false, true).await;
    // Too long for a username, so it can only be an address, and nothing can mail it.
    let address = format!("{}@example.cz", "e".repeat(60));
    let (status, body) = app
        .post("/api/auth/password/forgot", json!({ "login": address }))
        .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "email_disabled");
    // Usernames may contain `@`: this one is looked up as a username.
    assert_eq!(
        forgot(&app, "eva@example.cz", "cs").await,
        StatusCode::ACCEPTED
    );
    assert_eq!(forgot(&app, "nobody", "cs").await, StatusCode::ACCEPTED);
    app.settle_messages().await;
    assert!(app.bot.sent().is_empty());
}

#[tokio::test]
async fn both_channels_get_a_link_within_one_account_budget() {
    let (app, fake) = TestApp::with_telegram(true, true).await;
    let t = linked(&app, &fake, "eva", 502).await;
    let (status, _) = app
        .post_as(
            "/api/me/identities/email",
            &t.access_token,
            json!({ "email": "eva@example.cz" }),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let (_, link) = app.last_link("eva@example.cz").await;
    let (status, _) = app
        .post_as(
            "/api/me/identities/email/confirm",
            &t.access_token,
            json!({ "token": link }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    assert_eq!(forgot(&app, "eva", "cs").await, StatusCode::ACCEPTED);
    assert_eq!(app.bot_messages_to(502).await.len(), 1);
    assert_eq!(
        app.mails_to("eva@example.cz").await.len(),
        2,
        "link + reset"
    );

    // An address reaches only itself.
    assert_eq!(
        forgot(&app, "eva@example.cz", "cs").await,
        StatusCode::ACCEPTED
    );
    assert_eq!(app.bot_messages_to(502).await.len(), 1);

    // 5 per account per hour, mails and messages together: 3 used, one more request = 2.
    assert_eq!(forgot(&app, "EVA", "cs").await, StatusCode::ACCEPTED);
    assert_eq!(app.bot_messages_to(502).await.len(), 2);
    assert_eq!(app.mails_to("eva@example.cz").await.len(), 4);
    // Third request for this username (still under the per-request limit): the account is spent.
    assert_eq!(forgot(&app, " eva ", "cs").await, StatusCode::ACCEPTED);
    assert_eq!(app.bot_messages_to(502).await.len(), 2, "budget spent");
    assert_eq!(app.mails_to("eva@example.cz").await.len(), 4);
}

#[tokio::test]
async fn unlinking_telegram_kills_its_pending_reset_link() {
    let (app, fake) = TestApp::with_telegram(false, true).await;
    let t = linked(&app, &fake, "eva", 503).await;
    forgot(&app, "eva", "cs").await;
    let token = reset_token(&app, 503).await;
    let (_, ids) = app.get("/api/me/identities", Some(&t.access_token)).await;
    let id = ids["identities"][0]["id"].as_str().expect("id").to_owned();
    let (status, _) = app
        .delete(&format!("/api/me/identities/{id}"), &t.access_token)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "the password remains");
    let (status, _) = app
        .post(
            "/api/auth/password/reset/preview",
            json!({ "token": token }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn telegram_reset_tokens_do_not_log_in_by_email() {
    let (app, fake) = TestApp::with_telegram(true, true).await;
    linked(&app, &fake, "eva", 504).await;
    forgot(&app, "eva", "cs").await;
    let token = reset_token(&app, 504).await;
    for path in ["/api/auth/email/preview", "/api/auth/email/verify"] {
        let (status, _) = app.post(path, json!({ "token": token })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}");
    }
}

#[tokio::test]
async fn without_a_bot_token_telegram_accounts_get_no_reset_message() {
    let (app, fake) = TestApp::with_telegram(true, false).await;
    linked(&app, &fake, "eva", 505).await;
    let (status, _) = app
        .post("/api/auth/password/forgot", json!({ "login": "eva" }))
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert!(app.bot_messages_to(505).await.is_empty());
}

#[tokio::test]
async fn google_identities_are_not_a_reset_channel() {
    let (app, fake) = TestApp::with_telegram(false, true).await;
    let t = linked(&app, &fake, "eva", 506).await;
    app.sql(&format!(
        "INSERT INTO user_identity (id, user_id, provider, subject, verified_at) \
         VALUES (gen_random_uuid(), '{}', 'google', '507', now())",
        t.user_id
    ))
    .await;
    assert_eq!(forgot(&app, "eva", "cs").await, StatusCode::ACCEPTED);
    assert_eq!(app.bot_messages_to(506).await.len(), 1);
    assert!(app.bot_messages_to(507).await.is_empty());
}

#[tokio::test]
async fn providers_offer_password_reset_with_the_bot_alone() {
    let (bot_only, _fake) = TestApp::with_telegram(false, true).await;
    let (_, body) = bot_only.get("/api/auth/providers", None).await;
    assert_eq!(
        (body["email"].clone(), body["password_reset"].clone()),
        (json!(false), json!(true))
    );
    let (login_only, _fake) = TestApp::with_telegram(false, false).await;
    let (_, body) = login_only.get("/api/auth/providers", None).await;
    assert_eq!(
        body["password_reset"], false,
        "Telegram login without the bot sends nothing"
    );
}
