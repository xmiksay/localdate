//! When events become Web Push: offline recipients only, per preference, coalesced, never to the actor.

mod common;

use axum::http::StatusCode;
use common::push::endpoint;
use common::{TestApp, Tokens, count, ws};
use serde_json::json;

const LAT: f64 = 50.087;
const LON: f64 = 14.421;

async fn pair(app: &TestApp) -> (Tokens, Tokens) {
    (
        app.visible_user("anna", LAT, LON).await,
        app.visible_user("bara", LAT, LON + 0.001).await,
    )
}

async fn wave(app: &TestApp, from: &Tokens, to: &Tokens) {
    let (status, body) = app
        .post_as(
            "/api/waves",
            &from.access_token,
            json!({ "to_user_id": to.user_id }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

async fn message(app: &TestApp, from: &Tokens, match_id: &str, text: &str) -> StatusCode {
    app.post_as(
        &format!("/api/matches/{match_id}/messages"),
        &from.access_token,
        json!({ "body": text }),
    )
    .await
    .0
}

#[tokio::test]
async fn three_rejections_in_a_row_drop_a_subscription_and_success_resets_the_count() {
    let (app, pushes) = TestApp::with_push().await;
    let b = app.visible_user("bara", LAT, LON).await;
    let flaky = endpoint("flaky");
    app.subscribe_push(&b, &flaky, "cs").await;
    let failures =
        format!("SELECT failure_count::bigint FROM push_subscription WHERE endpoint = '{flaky}'");
    let rows = "SELECT count(*) FROM push_subscription";

    pushes.reject(&flaky, true);
    wave_from_new_user(&app, &b, 1).await;
    wave_from_new_user(&app, &b, 2).await;
    assert_eq!(count(&app, &failures).await, 2);
    pushes.reject(&flaky, false);
    wave_from_new_user(&app, &b, 3).await;
    assert_eq!(
        count(&app, &failures).await,
        0,
        "a success resets the count"
    );

    pushes.reject(&flaky, true);
    wave_from_new_user(&app, &b, 4).await;
    wave_from_new_user(&app, &b, 5).await;
    assert_eq!(count(&app, rows).await, 1);
    wave_from_new_user(&app, &b, 6).await;
    assert_eq!(count(&app, rows).await, 0);
    assert_eq!(pushes.sent().len(), 6);
}

/// Repeat waves within one window send nothing, so every push needs a fresh waver.
async fn wave_from_new_user(app: &TestApp, to: &Tokens, n: usize) {
    let from = app
        .visible_user(&format!("waver{n}"), LAT, LON + 0.001)
        .await;
    wave(app, &from, to).await;
    app.pushes_settled().await;
}

#[tokio::test]
async fn offline_recipient_gets_a_wave_push_on_every_device() {
    let (app, pushes) = TestApp::with_push().await;
    let (a, b) = pair(&app).await;
    app.subscribe_push(&a, &endpoint("anna"), "cs").await;
    app.subscribe_push(&b, &endpoint("bara-phone"), "cs").await;
    app.subscribe_push(&b, &endpoint("bara-laptop"), "en").await;

    wave(&app, &a, &b).await;
    app.pushes_settled().await;

    assert!(
        pushes.sent_to(&endpoint("anna")).is_empty(),
        "never to the actor"
    );
    let phone = pushes.sent_to(&endpoint("bara-phone"));
    assert_eq!(
        phone,
        vec![
            json!({ "type": "wave", "title": "localdate", "body": "Někdo ti zamával 👋", "url": "/nearby", "tag": "wave" })
        ]
    );
    let laptop = pushes.sent_to(&endpoint("bara-laptop"));
    assert_eq!(laptop.len(), 1);
    assert_eq!(laptop[0]["body"], "Someone waved at you 👋");
    let delivered = "SELECT count(*) FROM push_subscription WHERE last_success_at IS NOT NULL";
    assert_eq!(count(&app, delivered).await, 2);
}

#[tokio::test]
async fn online_recipient_gets_no_push() {
    let (app, pushes) = TestApp::with_push().await;
    let (a, b) = pair(&app).await;
    app.subscribe_push(&b, &endpoint("bara"), "cs").await;
    let url = ws::serve(&app.router).await;
    let mut socket = ws::ready_socket(&url, &b).await;

    wave(&app, &a, &b).await;
    assert_eq!(
        ws::next(&mut socket).await.map(|v| v["type"].clone()),
        Ok(json!("wave"))
    );
    app.pushes_settled().await;
    assert!(pushes.sent().is_empty());
}

#[tokio::test]
async fn a_preference_turned_off_silences_that_kind_only() {
    let (app, pushes) = TestApp::with_push().await;
    let (a, b) = pair(&app).await;
    app.subscribe_push(&a, &endpoint("anna"), "cs").await;
    app.subscribe_push(&b, &endpoint("bara"), "cs").await;
    app.patch(
        "/api/me/push/prefs",
        &b.access_token,
        json!({ "waves": false }),
    )
    .await;

    wave(&app, &a, &b).await;
    app.pushes_settled().await;
    assert!(pushes.sent().is_empty());

    // Bara waves back: the match goes to Anna (Bara caused it), whose prefs are all on.
    wave(&app, &b, &a).await;
    app.pushes_settled().await;
    let to_anna = pushes.sent_to(&endpoint("anna"));
    assert_eq!(to_anna.len(), 1);
    assert_eq!(to_anna[0]["type"], "match");
    assert_eq!(to_anna[0]["body"], "Máte novou shodu!");
    let match_id = to_anna[0]["tag"]
        .as_str()
        .and_then(|t| t.strip_prefix("match-"));
    assert_eq!(
        to_anna[0]["url"],
        format!("/matches/{}", match_id.unwrap_or("?"))
    );
    assert!(pushes.sent_to(&endpoint("bara")).is_empty());
}

#[tokio::test]
async fn messages_coalesce_per_match_and_skip_the_sender() {
    let (app, pushes) = TestApp::with_push().await;
    let (a, b) = pair(&app).await;
    let match_id = app.match_up(&a, &b).await;
    app.subscribe_push(&a, &endpoint("anna"), "cs").await;
    app.subscribe_push(&b, &endpoint("bara"), "en").await;

    for text in ["hi", "are you there?", "hello?"] {
        assert_eq!(
            message(&app, &a, &match_id, text).await,
            StatusCode::CREATED
        );
    }
    app.pushes_settled().await;
    let to_bara = pushes.sent_to(&endpoint("bara"));
    assert_eq!(to_bara.len(), 1, "one push per match per minute");
    assert_eq!(
        to_bara[0],
        json!({
            "type": "message", "title": "localdate", "body": "New message",
            "url": format!("/matches/{match_id}"), "tag": format!("message-{match_id}"),
        })
    );
    assert!(
        !to_bara[0].to_string().contains("hello"),
        "no message text in pushes"
    );
    assert!(pushes.sent_to(&endpoint("anna")).is_empty());

    // The window is per recipient: Bara's reply still reaches Anna.
    assert_eq!(
        message(&app, &b, &match_id, "yes").await,
        StatusCode::CREATED
    );
    app.pushes_settled().await;
    assert_eq!(pushes.sent_to(&endpoint("anna")).len(), 1);
}

#[tokio::test]
async fn a_gone_subscription_is_deleted_and_the_others_kept() {
    let (app, pushes) = TestApp::with_push().await;
    let (a, b) = pair(&app).await;
    app.subscribe_push(&b, &endpoint("old"), "cs").await;
    app.subscribe_push(&b, &endpoint("new"), "cs").await;
    pushes.expire(&endpoint("old"));

    wave(&app, &a, &b).await;
    app.pushes_settled().await;
    assert_eq!(pushes.sent().len(), 2);
    let left = "SELECT count(*) FROM push_subscription";
    assert_eq!(count(&app, left).await, 1);
    let kept = format!("{left} WHERE endpoint = '{}'", endpoint("new"));
    assert_eq!(count(&app, &kept).await, 1);
}

#[tokio::test]
async fn nothing_crosses_a_block() {
    let (app, pushes) = TestApp::with_push().await;
    let (a, b) = pair(&app).await;
    let match_id = app.match_up(&a, &b).await;
    app.subscribe_push(&b, &endpoint("bara"), "cs").await;
    app.post_as(
        "/api/blocks",
        &b.access_token,
        json!({ "user_id": a.user_id }),
    )
    .await;

    assert_eq!(
        message(&app, &a, &match_id, "hi").await,
        StatusCode::FORBIDDEN
    );
    app.pushes_settled().await;
    assert!(pushes.sent().is_empty());
}
