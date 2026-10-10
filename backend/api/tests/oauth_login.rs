mod common;

use axum::http::{Method, StatusCode};
use common::oauth::{FakeProvider, IdClaims, Started};
use common::{TestApp, Tokens, tokens_from};
use serde_json::json;

/// Names one claim and how to break it.
type Bend = (&'static str, fn(&mut IdClaims));

/// Start → consent as `sub` → callback; the one-time code from the done page.
async fn code_for(app: &TestApp, fake: &FakeProvider, sub: &str) -> (Started, String) {
    let started = app.oauth_start(None).await;
    let resp = app
        .oauth_return(fake, &started, FakeProvider::claims(&started, sub))
        .await;
    let fragment = resp.fragment();
    let code = fragment
        .get("code")
        .unwrap_or_else(|| panic!("no code: {fragment:?}"));
    (started, code.clone())
}

/// Full sign-up through Google; returns the new account's tokens.
async fn google_signup(app: &TestApp, fake: &FakeProvider, sub: &str, username: &str) -> Tokens {
    let (started, code) = code_for(app, fake, sub).await;
    let resp = app.oauth_exchange(Some(&started.cookie), &code).await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.body);
    let token = resp.body["signup"]["token"].as_str().expect("signup token");
    assert_eq!(resp.body["signup"]["provider"], "google");
    let (status, body) = app
        .post(
            "/api/auth/oauth/signup",
            json!({ "token": token, "username": username }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    tokens_from(&body)
}

#[tokio::test]
async fn providers_report_google_only_when_configured() {
    let off = TestApp::new().await;
    assert_eq!(
        off.get("/api/auth/providers", None).await.1["google"],
        false
    );
    let (on, _fake) = TestApp::with_google().await;
    assert_eq!(on.get("/api/auth/providers", None).await.1["google"], true);
}

#[tokio::test]
async fn disabled_provider_redirects_with_an_error_and_link_is_503() {
    let app = TestApp::new().await;
    let resp = app
        .raw(
            Method::GET,
            "/api/auth/oauth/google/start",
            None,
            None,
            None,
        )
        .await;
    assert_eq!(resp.status, StatusCode::FOUND);
    assert_eq!(resp.fragment()["error"], "provider_disabled");
    assert!(resp.set_cookie.is_none());

    let t = app.register("alice").await;
    let (status, body) = app
        .post_as("/api/auth/oauth/google/link", &t.access_token, json!({}))
        .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "provider_disabled");

    let resp = app
        .raw(Method::GET, "/api/auth/oauth/nope/start", None, None, None)
        .await;
    assert_eq!(resp.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn start_sets_a_scoped_httponly_cookie_and_sends_pkce() {
    let (app, _fake) = TestApp::with_google().await;
    let resp = app
        .raw(
            Method::GET,
            "/api/auth/oauth/google/start",
            None,
            None,
            None,
        )
        .await;
    let set = resp.set_cookie.expect("cookie");
    for attr in ["HttpOnly", "SameSite=Lax", "Path=/api/auth/oauth", "Secure"] {
        assert!(set.contains(attr), "{set} lacks {attr}");
    }
    let location = resp.location.expect("provider url");
    assert!(location.contains("code_challenge_method=S256"));
    assert!(location.contains("scope=openid&"));
}

#[tokio::test]
async fn new_google_account_signs_up_then_logs_in() {
    let (app, fake) = TestApp::with_google().await;
    let t = google_signup(&app, &fake, "g-123", "Gina").await;
    let (_, me) = app.get("/api/me", Some(&t.access_token)).await;
    assert_eq!(me["user"]["username"], "Gina");
    let (_, ids) = app.get("/api/me/identities", Some(&t.access_token)).await;
    assert_eq!(ids["has_password"], false);
    assert_eq!(ids["identities"][0]["provider"], "google");
    assert_eq!(ids["identities"][0]["subject"], "g-123");

    let started = app.oauth_start(Some("/chat/1")).await;
    let resp = app
        .oauth_return(&fake, &started, FakeProvider::claims(&started, "g-123"))
        .await;
    assert!(resp.set_cookie.is_none(), "cookie kept for the exchange");
    let fragment = resp.fragment();
    assert_eq!(fragment["redirect"], "/chat/1");
    let resp = app
        .oauth_exchange(Some(&started.cookie), &fragment["code"])
        .await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.body);
    assert_eq!(tokens_from(&resp.body["session"]).user_id, t.user_id);
    assert!(resp.set_cookie.expect("cleared").contains("Max-Age=0"));
}

#[tokio::test]
async fn code_is_single_use_and_bound_to_the_flow_cookie() {
    let (app, fake) = TestApp::with_google().await;
    google_signup(&app, &fake, "g-1", "gina").await;
    let (started, code) = code_for(&app, &fake, "g-1").await;

    let resp = app.oauth_exchange(None, &code).await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);
    assert_eq!(resp.body["error"]["code"], "invalid_token");
    let other = app.oauth_start(None).await;
    let resp = app.oauth_exchange(Some(&other.cookie), &code).await;
    assert_eq!(
        resp.status,
        StatusCode::BAD_REQUEST,
        "another browser's flow"
    );

    let resp = app.oauth_exchange(Some(&started.cookie), &code).await;
    assert_eq!(resp.status, StatusCode::OK, "not burnt by the mismatches");
    let resp = app.oauth_exchange(Some(&started.cookie), &code).await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST, "reuse");
}

#[tokio::test]
async fn expired_code_is_refused() {
    let (app, fake) = TestApp::with_google().await;
    let (started, code) = code_for(&app, &fake, "g-1").await;
    app.sql("UPDATE oauth_grant SET expires_at = now() - interval '1 second'")
        .await;
    let resp = app.oauth_exchange(Some(&started.cookie), &code).await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn state_mismatch_and_missing_cookie_fail_before_any_token_call() {
    let (app, fake) = TestApp::with_google().await;
    let started = app.oauth_start(None).await;
    let code = fake.authorize(&started, FakeProvider::claims(&started, "g-1"));
    let resp = app
        .oauth_callback(&started, &format!("code={code}&state=wrong"))
        .await;
    assert_eq!(resp.fragment()["error"], "invalid_state");
    assert!(resp.set_cookie.expect("cleared").contains("Max-Age=0"));

    let resp = app
        .raw(
            Method::GET,
            &format!(
                "/api/auth/oauth/google/callback?code={code}&state={}",
                started.state
            ),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(resp.fragment()["error"], "invalid_state");

    let tampered = Started {
        cookie: format!("{}x", started.cookie),
        ..started
    };
    let resp = app
        .oauth_callback(&tampered, &format!("code={code}&state={}", tampered.state))
        .await;
    assert_eq!(resp.fragment()["error"], "invalid_state");
}

#[tokio::test]
async fn denied_consent_is_cancelled() {
    let (app, _fake) = TestApp::with_google().await;
    let started = app.oauth_start(None).await;
    let resp = app
        .oauth_callback(
            &started,
            &format!("error=access_denied&state={}", started.state),
        )
        .await;
    assert_eq!(resp.fragment()["error"], "cancelled");
}

#[tokio::test]
async fn id_token_with_bad_nonce_audience_issuer_or_expiry_is_rejected() {
    let (app, fake) = TestApp::with_google().await;
    let bend: [Bend; 4] = [
        ("nonce", |c| c.nonce = "other".into()),
        ("aud", |c| c.aud = "someone-else".into()),
        ("iss", |c| c.iss = "https://evil.test".into()),
        ("exp", |c| c.exp = chrono::Utc::now().timestamp() - 3600),
    ];
    for (what, f) in bend {
        let started = app.oauth_start(None).await;
        let mut claims = FakeProvider::claims(&started, "g-1");
        f(&mut claims);
        let resp = app.oauth_return(&fake, &started, claims).await;
        assert_eq!(resp.fragment()["error"], "oauth_failed", "{what}");
    }
    assert_eq!(
        common::count(&app, "SELECT count(*) FROM oauth_grant").await,
        0
    );
}

#[tokio::test]
async fn banned_account_cannot_log_in_with_google() {
    let (app, fake) = TestApp::with_google().await;
    let t = google_signup(&app, &fake, "g-1", "gina").await;
    let (started, code) = code_for(&app, &fake, "g-1").await;
    app.sql(&format!(
        "UPDATE \"user\" SET banned_at = now() WHERE id = '{}'",
        t.user_id
    ))
    .await;
    let resp = app.oauth_exchange(Some(&started.cookie), &code).await;
    assert_eq!(resp.status, StatusCode::FORBIDDEN);
    assert_eq!(resp.body["error"]["code"], "banned");

    let started = app.oauth_start(None).await;
    let resp = app
        .oauth_return(&fake, &started, FakeProvider::claims(&started, "g-1"))
        .await;
    assert_eq!(resp.fragment()["error"], "banned");
}

#[tokio::test]
async fn signup_with_a_taken_username_keeps_the_token() {
    let (app, fake) = TestApp::with_google().await;
    app.register("taken").await;
    let (started, code) = code_for(&app, &fake, "g-9").await;
    let resp = app.oauth_exchange(Some(&started.cookie), &code).await;
    let token = resp.body["signup"]["token"].as_str().expect("token");
    let (status, _) = app
        .post(
            "/api/auth/oauth/signup",
            json!({ "token": token, "username": "taken" }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = app
        .post(
            "/api/auth/oauth/signup",
            json!({ "token": token, "username": "a\nb" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "validation");
    let (status, _) = app
        .post(
            "/api/auth/oauth/signup",
            json!({ "token": token, "username": "fresh" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, body) = app
        .post(
            "/api/auth/oauth/signup",
            json!({ "token": token, "username": "again" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_token");
}
