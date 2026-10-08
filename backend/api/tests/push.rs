//! Push config, subscriptions and preferences endpoints.

mod common;

use axum::http::{Method, StatusCode};
use common::push::{AUTH, P256DH, endpoint};
use common::{TestApp, count};
use localdate_api::config::VapidConfig;
use serde_json::json;

async fn subs(app: &TestApp, extra: &str) -> i64 {
    count(
        app,
        &format!("SELECT count(*) FROM push_subscription {extra}"),
    )
    .await
}

#[tokio::test]
async fn without_vapid_push_is_off_but_prefs_work() {
    let app = TestApp::new().await;
    let (status, body) = app.get("/api/push/config", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "enabled": false, "public_key": null }));

    let t = app.register("anna").await;
    assert_eq!(
        app.subscribe_push(&t, &endpoint("a"), "cs").await,
        StatusCode::CONFLICT
    );
    let (status, body) = app
        .post_as(
            "/api/me/push/subscriptions",
            &t.access_token,
            json!({ "endpoint": endpoint("a"), "keys": { "p256dh": P256DH, "auth": AUTH } }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "push_disabled");
    let (status, _) = app.get("/api/me/push/prefs", Some(&t.access_token)).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn with_vapid_the_config_publishes_the_public_key() {
    let (public_key, private_key) = localdate_api::push::vapid::generate();
    let app = TestApp::with_config(|c| {
        c.vapid = Some(VapidConfig {
            public_key: public_key.clone(),
            private_key,
            subject: "mailto:ops@example.com".into(),
        })
    })
    .await;
    let (_, body) = app.get("/api/push/config", None).await;
    assert_eq!(body, json!({ "enabled": true, "public_key": public_key }));
}

#[tokio::test]
async fn subscribe_validates_endpoint_keys_and_lang() {
    let (app, _) = TestApp::with_push().await;
    let t = app.register("anna").await;
    let post = |body| app.post_as("/api/me/push/subscriptions", &t.access_token, body);
    for bad in [
        json!({ "endpoint": "http://fcm.googleapis.com/fcm/send/x", "keys": { "p256dh": P256DH, "auth": AUTH } }),
        json!({ "endpoint": "https://10.0.0.1/x", "keys": { "p256dh": P256DH, "auth": AUTH } }),
        json!({ "endpoint": "https://example.com/push", "keys": { "p256dh": P256DH, "auth": AUTH } }),
        json!({ "endpoint": endpoint("x"), "keys": { "p256dh": "AAAA", "auth": AUTH } }),
        json!({ "endpoint": endpoint("x"), "keys": { "p256dh": P256DH, "auth": "AAAA" } }),
        json!({ "endpoint": endpoint("x"), "keys": { "p256dh": P256DH, "auth": AUTH }, "lang": "de" }),
        json!({ "endpoint": endpoint("x") }),
    ] {
        let (status, body) = post(bad.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}");
        assert_eq!(body["error"]["code"], "validation");
    }
    assert_eq!(subs(&app, "").await, 0);
    let (status, _) = app
        .post(
            "/api/me/push/subscriptions",
            json!({ "endpoint": endpoint("x"), "keys": { "p256dh": P256DH, "auth": AUTH } }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn subscriptions_upsert_by_endpoint_and_move_between_accounts() {
    let (app, _) = TestApp::with_push().await;
    let a = app.register("anna").await;
    let b = app.register("bara").await;
    let e = endpoint("shared");
    assert_eq!(
        app.subscribe_push(&a, &e, "cs").await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.subscribe_push(&a, &e, "en").await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(subs(&app, "WHERE lang = 'en'").await, 1);
    assert_eq!(subs(&app, "").await, 1);

    // Same browser, other account logged in: the endpoint now notifies only that account.
    assert_eq!(
        app.subscribe_push(&b, &e, "cs").await,
        StatusCode::NO_CONTENT
    );
    let owned_by_b = format!("WHERE user_id = '{}'", b.user_id);
    assert_eq!(subs(&app, &owned_by_b).await, 1);
    assert_eq!(subs(&app, "").await, 1);

    // Deleting only ever touches the caller's own subscription.
    let body = json!({ "endpoint": e });
    let (status, _) = app
        .request(
            Method::DELETE,
            "/api/me/push/subscriptions",
            Some(&a.access_token),
            Some(body.clone()),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(subs(&app, "").await, 1);
    for _ in 0..2 {
        let (status, _) = app
            .request(
                Method::DELETE,
                "/api/me/push/subscriptions",
                Some(&b.access_token),
                Some(body.clone()),
            )
            .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }
    assert_eq!(subs(&app, "").await, 0);
}

#[tokio::test]
async fn only_the_newest_ten_subscriptions_are_kept() {
    let (app, _) = TestApp::with_push().await;
    let a = app.register("anna").await;
    for i in 0..12 {
        assert_eq!(
            app.subscribe_push(&a, &endpoint(&format!("d{i}")), "cs")
                .await,
            StatusCode::NO_CONTENT
        );
    }
    assert_eq!(subs(&app, "").await, 10);
    let oldest = format!(
        "WHERE endpoint IN ('{}', '{}')",
        endpoint("d0"),
        endpoint("d1")
    );
    assert_eq!(subs(&app, &oldest).await, 0);
}

#[tokio::test]
async fn prefs_default_on_patch_partially_and_go_with_the_account() {
    let (app, _) = TestApp::with_push().await;
    let a = app.register("anna").await;
    let all_on = json!({ "waves": true, "matches": true, "messages": true });
    assert_eq!(
        app.get("/api/me/push/prefs", Some(&a.access_token)).await.1,
        all_on
    );

    let (status, body) = app
        .patch(
            "/api/me/push/prefs",
            &a.access_token,
            json!({ "messages": false }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({ "waves": true, "matches": true, "messages": false })
    );
    let (_, body) = app
        .patch(
            "/api/me/push/prefs",
            &a.access_token,
            json!({ "waves": false }),
        )
        .await;
    assert_eq!(
        body,
        json!({ "waves": false, "matches": true, "messages": false })
    );
    assert_eq!(
        app.get("/api/me/push/prefs", Some(&a.access_token)).await.1,
        body
    );

    let (status, _) = app
        .patch(
            "/api/me/push/prefs",
            &a.access_token,
            json!({ "sms": true }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    app.subscribe_push(&a, &endpoint("a"), "cs").await;
    app.delete("/api/me", &a.access_token).await;
    assert_eq!(subs(&app, "").await, 0);
    assert_eq!(count(&app, "SELECT count(*) FROM push_prefs").await, 0);
}
