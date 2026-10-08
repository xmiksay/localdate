mod common;

use std::sync::atomic::Ordering;
use std::time::Duration;

use axum::http::{Method, StatusCode};
use common::TestApp;
use common::oauth::{FakeProvider, Raw};
use serde_json::json;

/// Start → consent as `sub` → callback → exchange; the exchange answer.
async fn google_login(app: &TestApp, fake: &FakeProvider, sub: &str) -> Raw {
    let started = app.oauth_start(None).await;
    let resp = app
        .oauth_return(fake, &started, FakeProvider::claims(&started, sub))
        .await;
    let fragment = resp.fragment();
    let code = fragment
        .get("code")
        .unwrap_or_else(|| panic!("no code: {fragment:?}"));
    app.oauth_exchange(Some(&started.cookie), code).await
}

#[tokio::test]
async fn logout_clears_the_flow_cookie() {
    let (app, _fake) = TestApp::with_google().await;
    let t = app.register("alice").await;
    let resp = app
        .raw(
            Method::POST,
            "/api/auth/logout",
            None,
            None,
            Some(json!({ "refresh_token": t.refresh_token })),
        )
        .await;
    assert_eq!(resp.status, StatusCode::NO_CONTENT);
    let set = resp.set_cookie.expect("clearing cookie");
    for attr in [
        "ld_oauth=;",
        "Path=/api/auth/oauth",
        "Max-Age=0",
        "HttpOnly",
    ] {
        assert!(set.contains(attr), "{set} lacks {attr}");
    }
}

#[tokio::test]
async fn linking_mails_a_notice_to_the_accounts_addresses() {
    let (app, fake) = TestApp::with_google_opts(true, |_| {}).await;
    let alice = app.email_signup("alice@example.cz", "alice").await;
    let before = app.mails_to("alice@example.cz").await.len();

    let started = app.oauth_link(&alice.access_token).await;
    let resp = app
        .oauth_return(&fake, &started, FakeProvider::claims(&started, "g-1"))
        .await;
    assert_eq!(resp.fragment()["linked"], "google");
    let mails = app.mails_to("alice@example.cz").await;
    assert_eq!(mails.len(), before + 1);
    let notice = mails.last().expect("notice");
    assert!(notice.subject.contains("Google"), "{}", notice.subject);
    assert!(notice.text.contains("alice"));
    assert!(
        !notice.text.contains("#token="),
        "a notice carries no token"
    );

    // Linking the same Google account again changes nothing and mails nothing.
    let started = app.oauth_link(&alice.access_token).await;
    app.oauth_return(&fake, &started, FakeProvider::claims(&started, "g-1"))
        .await;
    assert_eq!(app.mails_to("alice@example.cz").await.len(), before + 1);

    // An account without an email gets no mail at all.
    let bob = app.register("bob").await;
    let total = app.outbox.sent().len();
    let started = app.oauth_link(&bob.access_token).await;
    app.oauth_return(&fake, &started, FakeProvider::claims(&started, "g-2"))
        .await;
    app.mails_to("alice@example.cz").await;
    assert_eq!(app.outbox.sent().len(), total);
}

#[tokio::test]
async fn azp_must_name_this_client_when_present() {
    let (app, fake) = TestApp::with_google().await;
    let started = app.oauth_start(None).await;
    let mut claims = FakeProvider::claims(&started, "g-1");
    claims.azp = Some("another-client".into());
    let resp = app.oauth_return(&fake, &started, claims).await;
    assert_eq!(resp.fragment()["error"], "oauth_failed");

    let started = app.oauth_start(None).await;
    let mut claims = FakeProvider::claims(&started, "g-1");
    claims.azp = Some(common::oauth::CLIENT_ID.into());
    let resp = app.oauth_return(&fake, &started, claims).await;
    assert!(resp.fragment().contains_key("code"));
}

#[tokio::test]
async fn concurrent_callbacks_fetch_the_signing_keys_once() {
    let (app, fake) = TestApp::with_google().await;
    let a = app.oauth_start(None).await;
    let b = app.oauth_start(None).await;
    let (ra, rb) = tokio::join!(
        app.oauth_return(&fake, &a, FakeProvider::claims(&a, "g-a")),
        app.oauth_return(&fake, &b, FakeProvider::claims(&b, "g-b")),
    );
    assert!(ra.fragment().contains_key("code"), "{:?}", ra.fragment());
    assert!(rb.fragment().contains_key("code"), "{:?}", rb.fragment());
    assert_eq!(fake.jwks_hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn expired_keys_are_still_used_when_the_refetch_fails() {
    let (app, fake) =
        TestApp::with_google_opts(false, |c| c.oauth[0].jwks_ttl = Duration::ZERO).await;
    assert_eq!(
        google_login(&app, &fake, "g-1").await.status,
        StatusCode::OK
    );
    fake.jwks_down.store(true, Ordering::SeqCst);
    let resp = google_login(&app, &fake, "g-1").await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.body);
    assert_eq!(
        fake.jwks_hits.load(Ordering::SeqCst),
        2,
        "a refetch was tried"
    );
}

#[tokio::test]
async fn only_start_is_rate_limited_not_the_callback() {
    let (app, fake) = TestApp::with_google_opts(false, |c| c.rate_limit = true).await;
    // The bucket holds 5: spend them all on starts.
    let flows: Vec<_> = futures_util::future::join_all((0..5).map(|_| app.oauth_start(None))).await;
    let resp = app
        .raw(
            Method::GET,
            "/api/auth/oauth/google/start",
            None,
            None,
            None,
        )
        .await;
    assert_eq!(resp.fragment()["error"], "rate_limited");

    // The provider's code still goes through: the flow was counted at start.
    let started = &flows[0];
    let resp = app
        .oauth_return(&fake, started, FakeProvider::claims(started, "g-1"))
        .await;
    assert!(
        resp.fragment().contains_key("code"),
        "{:?}",
        resp.fragment()
    );
}
