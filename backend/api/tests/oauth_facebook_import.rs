//! Re-importing the Facebook profile picture at any time (`POST /auth/oauth/facebook/import`).

mod common;

use std::collections::HashMap;
use std::sync::atomic::Ordering;

use axum::http::{Method, StatusCode};
use common::TestApp;
use common::facebook::FakeFacebook;
use common::oauth::Raw;
use serde_json::json;

/// Facebook identities of `user`.
async fn fb_identities(app: &TestApp, user: uuid::Uuid) -> i64 {
    common::count(
        app,
        &format!(
            "SELECT count(*) FROM user_identity WHERE user_id = '{user}' AND provider = 'facebook'"
        ),
    )
    .await
}

/// Import as `token`, consent as `id`; the callback's fragment.
async fn import_as(
    app: &TestApp,
    fake: &FakeFacebook,
    token: &str,
    id: &str,
) -> HashMap<String, String> {
    let started = app.fb_import(token).await;
    app.fb_return(fake, &started, id).await.fragment()
}

/// `POST {path}` with an empty body, as `token`.
async fn post_import(app: &TestApp, path: &str, token: Option<&str>) -> Raw {
    app.raw(Method::POST, path, None, token, Some(json!({})))
        .await
}

#[tokio::test]
async fn a_linked_account_imports_its_current_picture() {
    let (app, fake) = TestApp::with_facebook().await;
    let (t, _) = app.fb_signup(&fake, "60001", "eva", false).await;
    assert_eq!(app.photo_count(t.user_id).await, 0);

    let fragment = import_as(&app, &fake, &t.access_token, "60001").await;
    assert_eq!(fragment["imported"], "imported");
    assert_eq!(fragment["redirect"], "/profile");
    assert!(!fragment.contains_key("linked"), "{fragment:?}");
    assert_eq!(app.photo_count(t.user_id).await, 1);
    assert_eq!(fb_identities(&app, t.user_id).await, 1);

    // Any time again: a second import adds another copy.
    let fragment = import_as(&app, &fake, &t.access_token, "60001").await;
    assert_eq!(fragment["imported"], "imported");
    assert_eq!(app.photo_count(t.user_id).await, 2);
}

#[tokio::test]
async fn an_account_without_facebook_gets_it_linked_and_imports() {
    let (app, fake) = TestApp::with_facebook().await;
    let alice = app.register("alice").await;
    let fragment = import_as(&app, &fake, &alice.access_token, "60002").await;
    assert_eq!(fragment["imported"], "imported");
    assert_eq!(fb_identities(&app, alice.user_id).await, 1);
    assert_eq!(app.photo_count(alice.user_id).await, 1);
}

#[tokio::test]
async fn another_facebook_account_is_a_mismatch_and_imports_nothing() {
    let (app, fake) = TestApp::with_facebook().await;
    let (t, _) = app.fb_signup(&fake, "60003", "eva", false).await;
    let fragment = import_as(&app, &fake, &t.access_token, "60004").await;
    assert_eq!(fragment["error"], "identity_mismatch");
    assert_eq!(fake.picture_hits.load(Ordering::SeqCst), 0);
    assert_eq!(fb_identities(&app, t.user_id).await, 1);
    assert_eq!(app.photo_count(t.user_id).await, 0);
}

#[tokio::test]
async fn a_facebook_account_of_another_user_is_taken() {
    let (app, fake) = TestApp::with_facebook().await;
    app.fb_signup(&fake, "60005", "eva", false).await;
    let bob = app.register("bob").await;
    let fragment = import_as(&app, &fake, &bob.access_token, "60005").await;
    assert_eq!(fragment["error"], "identity_taken");
    assert_eq!(fake.picture_hits.load(Ordering::SeqCst), 0);
    assert_eq!(fb_identities(&app, bob.user_id).await, 0);
}

#[tokio::test]
async fn a_full_account_imports_nothing() {
    let (app, fake) = TestApp::with_facebook().await;
    let t = app.register("full").await;
    for _ in 0..6 {
        app.upload_photo(&t, 32, 32).await;
    }
    let fragment = import_as(&app, &fake, &t.access_token, "60006").await;
    assert_eq!(fragment["imported"], "full");
    assert_eq!(app.photo_count(t.user_id).await, 6);
}

#[tokio::test]
async fn banned_superseded_and_gone_accounts_download_nothing() {
    let (app, fake) = TestApp::with_facebook().await;
    let alice = app.register("alice").await;
    let started = app.fb_import(&alice.access_token).await;
    app.sql("UPDATE \"user\" SET banned_at = now() WHERE username = 'alice'")
        .await;
    let fragment = app.fb_return(&fake, &started, "60007").await.fragment();
    assert_eq!(fragment["error"], "banned");

    let bob = app.register("bob").await;
    let started = app.fb_import(&bob.access_token).await;
    // As a reset/change committed after the token that started the flow (past #18's same-second grace).
    app.sql(
        "UPDATE \"user\" SET credentials_changed_at = date_trunc('second', now()) + interval '2 seconds' \
         WHERE username = 'bob'",
    )
    .await;
    let fragment = app.fb_return(&fake, &started, "60008").await.fragment();
    assert_eq!(fragment["error"], "unauthorized");

    let carol = app.register("carol").await;
    let started = app.fb_import(&carol.access_token).await;
    app.sql("DELETE FROM \"user\" WHERE username = 'carol'")
        .await;
    let fragment = app.fb_return(&fake, &started, "60009").await.fragment();
    assert_eq!(fragment["error"], "unauthorized");

    assert_eq!(fake.picture_hits.load(Ordering::SeqCst), 0);
    assert_eq!(
        common::count(&app, "SELECT count(*) FROM user_identity").await,
        0
    );
}

#[tokio::test]
async fn import_needs_auth_and_an_enabled_provider() {
    let (app, _fake) = TestApp::with_facebook().await;
    let resp = post_import(&app, "/api/auth/oauth/facebook/import", None).await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
    let t = app.register("alice").await;
    let token = Some(t.access_token.as_str());
    let resp = post_import(&app, "/api/auth/oauth/nope/import", token).await;
    assert_eq!(resp.status, StatusCode::NOT_FOUND);
    // Google is not configured here: disabled is answered before "no picture".
    let resp = post_import(&app, "/api/auth/oauth/google/import", token).await;
    assert_eq!(resp.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(resp.body["error"]["code"], "provider_disabled");

    let app = TestApp::new().await;
    let t = app.register("bob").await;
    let resp = post_import(
        &app,
        "/api/auth/oauth/facebook/import",
        Some(&t.access_token),
    )
    .await;
    assert_eq!(resp.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(resp.body["error"]["code"], "provider_disabled");
}

#[tokio::test]
async fn google_offers_no_import() {
    let (app, _fake) = TestApp::with_google().await;
    let t = app.register("alice").await;
    let resp = post_import(&app, "/api/auth/oauth/google/import", Some(&t.access_token)).await;
    assert_eq!(resp.status, StatusCode::NOT_FOUND, "{}", resp.body);
    assert!(resp.set_cookie.is_none(), "no flow is started");
}

#[tokio::test]
async fn an_import_callback_with_a_foreign_state_does_nothing() {
    let (app, fake) = TestApp::with_facebook().await;
    let alice = app.register("alice").await;
    let mine = app.fb_import(&alice.access_token).await;
    let other = app.fb_import(&alice.access_token).await;
    // The cookie of one flow with the state of another, and that state without any cookie.
    for cookie in [Some(mine.cookie.as_str()), None] {
        let code = fake.authorize(&mine, "60010");
        let resp = app
            .raw(
                Method::GET,
                &format!(
                    "/api/auth/oauth/facebook/callback?code={code}&state={}",
                    other.state
                ),
                cookie,
                None,
                None,
            )
            .await;
        assert_eq!(resp.fragment()["error"], "invalid_state");
    }
    assert_eq!(fake.picture_hits.load(Ordering::SeqCst), 0);
    assert_eq!(fb_identities(&app, alice.user_id).await, 0);
    assert_eq!(app.photo_count(alice.user_id).await, 0);
}
