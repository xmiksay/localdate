mod common;

use axum::http::StatusCode;
use common::oauth::FakeProvider;
use common::telegram::claims;
use common::{TestApp, tokens_from};
use localdate_api::auth::oauth::Provider;

#[tokio::test]
async fn providers_report_telegram_only_when_configured() {
    let off = TestApp::new().await;
    assert_eq!(
        off.get("/api/auth/providers", None).await.1["telegram"],
        false
    );
    let (on, _fake) = TestApp::with_telegram(false, false).await;
    let (_, body) = on.get("/api/auth/providers", None).await;
    assert_eq!(body["telegram"], true);
    assert_eq!(body["google"], false);
}

#[tokio::test]
async fn start_asks_for_the_id_and_bot_access() {
    let (app, _fake) = TestApp::with_telegram(false, false).await;
    let started = app.oauth_start_as(Provider::Telegram, None).await;
    assert_eq!(started.scope, "openid profile telegram:bot_access");
}

#[tokio::test]
async fn new_telegram_user_signs_up_with_the_numeric_id_then_logs_in() {
    let (app, fake) = TestApp::with_telegram(false, false).await;
    let t = app.telegram_signup(&fake, 987_654_321, "Eva").await;
    let (_, ids) = app.get("/api/me/identities", Some(&t.access_token)).await;
    assert_eq!(ids["has_password"], false);
    assert_eq!(ids["identities"][0]["provider"], "telegram");
    assert_eq!(
        ids["identities"][0]["subject"], "987654321",
        "the chat id, not the opaque sub"
    );

    let resp = app.telegram_exchange(&fake, 987_654_321).await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.body);
    assert_eq!(tokens_from(&resp.body["session"]).user_id, t.user_id);
}

#[tokio::test]
async fn an_id_token_without_the_numeric_id_is_refused() {
    let (app, fake) = TestApp::with_telegram(false, false).await;
    let started = app.oauth_start_as(Provider::Telegram, None).await;
    // What an `openid`-only consent would give: `sub` but no `id`.
    let no_id = FakeProvider::claims(&started, "opaque-sub");
    let resp = app
        .oauth_return_as(Provider::Telegram, &fake, &started, no_id)
        .await;
    assert_eq!(resp.fragment()["error"], "oauth_failed");
}

#[tokio::test]
async fn banned_account_cannot_log_in_with_telegram() {
    let (app, fake) = TestApp::with_telegram(false, false).await;
    let t = app.telegram_signup(&fake, 11, "eva").await;
    app.sql(&format!(
        "UPDATE \"user\" SET banned_at = now() WHERE id = '{}'",
        t.user_id
    ))
    .await;
    let (_, resp) = app.telegram_return(&fake, 11).await;
    assert_eq!(resp.fragment()["error"], "banned");
}

#[tokio::test]
async fn linking_adds_telegram_and_refuses_one_taken_elsewhere() {
    let (app, fake) = TestApp::with_telegram(false, false).await;
    let eva = app.register("eva").await;
    let fragment = app.telegram_link(&fake, &eva, 21).await;
    assert_eq!(fragment.get("linked").map(String::as_str), Some("telegram"));
    let resp = app.telegram_exchange(&fake, 21).await;
    assert_eq!(tokens_from(&resp.body["session"]).user_id, eva.user_id);

    let adam = app.register("adam").await;
    let fragment = app.telegram_link(&fake, &adam, 21).await;
    assert_eq!(fragment["error"], "identity_taken");
}

#[tokio::test]
async fn the_same_number_from_google_is_another_identity() {
    // Subjects are unique per provider: a Google `sub` equal to a Telegram id links nothing.
    let (app, fake) = TestApp::with_telegram(false, false).await;
    let t = app.telegram_signup(&fake, 31, "eva").await;
    app.sql(&format!(
        "INSERT INTO user_identity (id, user_id, provider, subject, verified_at) \
         VALUES (gen_random_uuid(), '{}', 'google', '31', now())",
        t.user_id
    ))
    .await;
    let started = app.oauth_start_as(Provider::Telegram, None).await;
    let c = claims(&started, 31);
    let resp = app
        .oauth_return_as(Provider::Telegram, &fake, &started, c)
        .await;
    assert!(resp.fragment().contains_key("code"));
}

#[tokio::test]
async fn telegram_accepts_a_missing_nonce_but_never_a_wrong_one() {
    let (app, fake) = TestApp::with_telegram(false, false).await;
    let started = app.oauth_start_as(Provider::Telegram, None).await;
    let mut no_nonce = claims(&started, 41);
    no_nonce.nonce = String::new();
    let resp = app
        .oauth_return_as(Provider::Telegram, &fake, &started, no_nonce)
        .await;
    assert!(
        resp.fragment().contains_key("code"),
        "{:?}",
        resp.fragment()
    );

    let started = app.oauth_start_as(Provider::Telegram, None).await;
    let mut wrong = claims(&started, 41);
    wrong.nonce = "someone-elses".into();
    let resp = app
        .oauth_return_as(Provider::Telegram, &fake, &started, wrong)
        .await;
    assert_eq!(resp.fragment()["error"], "oauth_failed");
}

#[tokio::test]
async fn google_still_requires_the_nonce() {
    let (app, fake) = TestApp::with_google().await;
    let started = app.oauth_start(None).await;
    let mut no_nonce = FakeProvider::claims(&started, "g-1");
    no_nonce.nonce = String::new();
    let resp = app.oauth_return(&fake, &started, no_nonce).await;
    assert_eq!(resp.fragment()["error"], "oauth_failed");
}
