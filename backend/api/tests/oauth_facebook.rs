mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use common::facebook::{APP_SECRET, FakeFacebook};
use hmac::{Hmac, Mac};
use sha2::Sha256;

#[tokio::test]
async fn providers_report_facebook_only_when_configured() {
    let off = TestApp::new().await;
    let (_, body) = off.get("/api/auth/providers", None).await;
    assert_eq!(body["facebook"], false);
    let resp = off
        .raw(
            Method::GET,
            "/api/auth/oauth/facebook/start",
            None,
            None,
            None,
        )
        .await;
    assert_eq!(resp.fragment()["error"], "provider_disabled");

    let (on, _fake) = TestApp::with_facebook().await;
    let (_, body) = on.get("/api/auth/providers", None).await;
    assert_eq!(body["facebook"], true);
    assert_eq!(body["google"], false);
}

#[tokio::test]
async fn start_asks_for_public_profile_with_pkce_and_state() {
    let (app, fake) = TestApp::with_facebook().await;
    let resp = app
        .raw(
            Method::GET,
            "/api/auth/oauth/facebook/start",
            None,
            None,
            None,
        )
        .await;
    let location = url::Url::parse(&resp.location.expect("provider url")).expect("url");
    assert!(location.as_str().starts_with(&fake.base));
    let q: std::collections::HashMap<_, _> = location.query_pairs().into_owned().collect();
    assert_eq!(q["client_id"], "fb-app");
    assert_eq!(q["scope"], "public_profile");
    assert_eq!(q["response_type"], "code");
    assert_eq!(q["code_challenge_method"], "S256");
    assert!(!q["state"].is_empty());
    assert_eq!(
        q["redirect_uri"],
        "https://app.test/api/auth/oauth/facebook/callback"
    );
}

#[tokio::test]
async fn new_facebook_account_signs_up_then_logs_in() {
    let (app, fake) = TestApp::with_facebook().await;
    let (t, fragment) = app.fb_signup(&fake, "10001", "Fiona", false).await;
    assert!(!fragment.contains_key("photo"), "no import asked");
    let (_, ids) = app.get("/api/me/identities", Some(&t.access_token)).await;
    assert_eq!(ids["has_password"], false);
    assert_eq!(ids["identities"][0]["provider"], "facebook");
    assert_eq!(ids["identities"][0]["subject"], "10001");

    let started = app.fb_start(false).await;
    let resp = app.fb_return(&fake, &started, "10001").await;
    let resp = app
        .oauth_exchange(Some(&started.cookie), &resp.fragment()["code"])
        .await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.body);
    assert_eq!(resp.body["session"]["user"]["username"], "Fiona");
    assert_eq!(
        fake.picture_hits.load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}

#[tokio::test]
async fn graph_calls_carry_a_correct_appsecret_proof_and_a_bearer_token() {
    let (app, fake) = TestApp::with_facebook().await;
    app.fb_signup(&fake, "10002", "proof", false).await;
    let calls = fake.graph_calls.lock().expect("lock").clone();
    assert_eq!(calls.len(), 1);
    let call = &calls[0];
    let mut mac = Hmac::<Sha256>::new_from_slice(APP_SECRET.as_bytes()).expect("key");
    mac.update(call.token.as_bytes());
    let expected: String = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(call.proof, expected);
    assert_eq!(call.fields.as_deref(), Some("id"));
    assert!(
        !call.token_in_query,
        "the access token goes in the header, not the URL"
    );
}

#[tokio::test]
async fn link_attaches_facebook_and_another_users_account_is_taken() {
    let (app, fake) = TestApp::with_facebook().await;
    let alice = app.register("alice").await;
    let started = app.fb_link(&alice.access_token, false).await;
    let resp = app.fb_return(&fake, &started, "20001").await;
    let fragment = resp.fragment();
    assert_eq!(fragment["linked"], "facebook");
    assert!(!fragment.contains_key("photo"));
    let (_, ids) = app
        .get("/api/me/identities", Some(&alice.access_token))
        .await;
    assert_eq!(ids["identities"][0]["provider"], "facebook");

    let bob = app.register("bob").await;
    let started = app.fb_link(&bob.access_token, false).await;
    let resp = app.fb_return(&fake, &started, "20001").await;
    assert_eq!(resp.fragment()["error"], "identity_taken");
}

#[tokio::test]
async fn banned_account_cannot_log_in_with_facebook() {
    let (app, fake) = TestApp::with_facebook().await;
    let (t, _) = app.fb_signup(&fake, "30001", "banme", false).await;
    app.sql(&format!(
        "UPDATE \"user\" SET banned_at = now() WHERE id = '{}'",
        t.user_id
    ))
    .await;
    let started = app.fb_start(true).await;
    let resp = app.fb_return(&fake, &started, "30001").await;
    assert_eq!(resp.fragment()["error"], "banned");
    assert_eq!(app.photo_count(t.user_id).await, 0, "no import for a ban");
    assert_eq!(
        fake.picture_hits.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "nothing downloaded for a refused flow"
    );
}

#[tokio::test]
async fn a_wrong_app_secret_fails_the_callback() {
    let fake = FakeFacebook::start().await;
    let mut config = fake.config();
    config.client_secret = "not-the-secret".into();
    let app = TestApp::with_config(|c| c.oauth = vec![config]).await;
    let started = app.fb_start(false).await;
    let resp = app.fb_return(&fake, &started, "40001").await;
    assert_eq!(resp.fragment()["error"], "oauth_failed");
}
