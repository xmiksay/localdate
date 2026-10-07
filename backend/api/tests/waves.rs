mod common;

use axum::http::StatusCode;
use common::{TestApp, Tokens, count};
use serde_json::{Value, json};

const LAT: f64 = 50.0870;
const LON: f64 = 14.4210;
const D300: f64 = 0.0027;

async fn wave(app: &TestApp, from: &Tokens, to: &Tokens) -> (StatusCode, Value) {
    app.post_as(
        "/api/waves",
        &from.access_token,
        json!({ "to_user_id": to.user_id }),
    )
    .await
}

async fn pair(app: &TestApp) -> (Tokens, Tokens) {
    let a = app.visible_user("anna", LAT, LON).await;
    let b = app.visible_user("bob", LAT + D300, LON).await;
    (a, b)
}

async fn state_of(app: &TestApp, viewer: &Tokens, other: &Tokens) -> Value {
    let (_, list) = app.get("/api/nearby", Some(&viewer.access_token)).await;
    list.as_array()
        .and_then(|l| l.iter().find(|p| p["user_id"] == json!(other.user_id)))
        .cloned()
        .expect("other is nearby")
}

#[tokio::test]
async fn wave_needs_window_and_visibility() {
    let app = TestApp::new().await;
    let lonely = app.register("lonely").await;
    app.onboard(&lonely, "female", "1995-05-05").await;
    let other = app.visible_user("other", LAT, LON).await;
    let (status, err) = wave(&app, &lonely, &other).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(err["error"]["code"], "no_active_window");

    let a = app.visible_user("anna", LAT, LON).await;
    let far = app.visible_user("far", LAT + 0.5, LON).await;
    for target in [&far, &a] {
        let (status, err) = wave(&app, &a, target).await;
        assert_eq!(status, StatusCode::CONFLICT, "{err}");
        assert_eq!(err["error"]["code"], "not_visible");
    }
    let ghost = json!({ "to_user_id": uuid::Uuid::new_v4() });
    let (status, err) = app.post_as("/api/waves", &a.access_token, ghost).await;
    assert_eq!(err["error"]["code"], "not_visible");
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn wave_is_idempotent_and_shows_in_incoming_and_nearby() {
    let app = TestApp::new().await;
    let (a, b) = pair(&app).await;
    for _ in 0..2 {
        let (status, res) = wave(&app, &a, &b).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(res, json!({ "matched": false, "match_id": null }));
    }
    assert_eq!(count(&app, "SELECT count(*) FROM wave").await, 1);
    let (_, window) = app.get("/api/me/window", Some(&a.access_token)).await;
    assert_eq!(window["waves_left"], 19);

    assert_eq!(state_of(&app, &a, &b).await["wave_state"], "sent");
    assert_eq!(state_of(&app, &b, &a).await["wave_state"], "received");
    let (status, incoming) = app.get("/api/waves/incoming", Some(&b.access_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(incoming[0]["user_id"], json!(a.user_id));
    assert_eq!(incoming[0]["wave_state"], "received");
    let (_, mine) = app.get("/api/waves/incoming", Some(&a.access_token)).await;
    assert_eq!(mine, json!([]));
}

#[tokio::test]
async fn incoming_drops_blocked_and_hidden_senders() {
    let app = TestApp::new().await;
    let (a, b) = pair(&app).await;
    wave(&app, &a, &b).await;
    app.post_as(
        "/api/blocks",
        &b.access_token,
        json!({ "user_id": a.user_id }),
    )
    .await;
    let (_, incoming) = app.get("/api/waves/incoming", Some(&b.access_token)).await;
    assert_eq!(incoming, json!([]));
}

#[tokio::test]
async fn waving_back_creates_one_match_and_deletes_both_waves() {
    let app = TestApp::new().await;
    let (a, b) = pair(&app).await;
    wave(&app, &a, &b).await;
    let (status, res) = wave(&app, &b, &a).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(res["matched"], true);
    let match_id = res["match_id"].as_str().expect("match id").to_owned();

    assert_eq!(count(&app, "SELECT count(*) FROM wave").await, 0);
    assert_eq!(count(&app, r#"SELECT count(*) FROM "match""#).await, 1);
    let state = state_of(&app, &a, &b).await;
    assert_eq!(state["wave_state"], "matched");
    assert_eq!(state["match_id"], json!(match_id));

    // Waving again returns the existing match.
    let (_, again) = wave(&app, &a, &b).await;
    assert_eq!(again, json!({ "matched": true, "match_id": match_id }));
    assert_eq!(count(&app, r#"SELECT count(*) FROM "match""#).await, 1);

    let (_, matches) = app.get("/api/matches", Some(&a.access_token)).await;
    assert_eq!(matches[0]["match_id"], json!(match_id));
    assert_eq!(matches[0]["other"]["user_id"], json!(b.user_id));
}

#[tokio::test]
async fn wave_limit_is_twenty_per_window() {
    let app = TestApp::new().await;
    let (a, b) = pair(&app).await;
    app.sql(&format!(
        r#"
        INSERT INTO "user" (id, username, password_hash)
          SELECT gen_random_uuid(), 'seed' || g, 'x' FROM generate_series(1, 20) g;
        INSERT INTO wave (id, from_user_id, to_user_id, window_id, expires_at)
          SELECT gen_random_uuid(), '{a}', u.id, w.id, w.ends_at
          FROM "user" u, visibility_window w
          WHERE u.username LIKE 'seed%' AND w.user_id = '{a}';
        "#,
        a = a.user_id
    ))
    .await;
    let (_, window) = app.get("/api/me/window", Some(&a.access_token)).await;
    assert_eq!(window["waves_left"], 0);
    let (status, err) = wave(&app, &a, &b).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(err["error"]["code"], "wave_limit");

    // One slot frees up: the wave goes through.
    app.sql(
        "DELETE FROM wave WHERE to_user_id IN (SELECT id FROM \"user\" WHERE username = 'seed1')",
    )
    .await;
    let (status, _) = wave(&app, &a, &b).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn blocked_target_is_not_visible() {
    let app = TestApp::new().await;
    let (a, b) = pair(&app).await;
    app.post_as(
        "/api/blocks",
        &a.access_token,
        json!({ "user_id": b.user_id }),
    )
    .await;
    let (status, err) = wave(&app, &b, &a).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(err["error"]["code"], "not_visible");
}

#[tokio::test]
async fn ending_or_restarting_a_window_drops_its_waves() {
    let app = TestApp::new().await;
    let (a, b) = pair(&app).await;
    wave(&app, &a, &b).await;
    app.delete("/api/me/window", &a.access_token).await;
    assert_eq!(count(&app, "SELECT count(*) FROM wave").await, 0);

    app.open_window(&a, LAT, LON).await;
    wave(&app, &a, &b).await;
    app.open_window(&a, LAT, LON).await;
    assert_eq!(count(&app, "SELECT count(*) FROM wave").await, 0);
    let (_, window) = app.get("/api/me/window", Some(&a.access_token)).await;
    assert_eq!(window["waves_left"], 20);
}

#[tokio::test]
async fn expired_waves_are_ignored() {
    let app = TestApp::new().await;
    let (a, b) = pair(&app).await;
    wave(&app, &a, &b).await;
    app.sql("UPDATE wave SET expires_at = now() - interval '1 second'")
        .await;
    let (_, incoming) = app.get("/api/waves/incoming", Some(&b.access_token)).await;
    assert_eq!(incoming, json!([]));
    // B waving at A now only sends a wave instead of matching.
    let (_, res) = wave(&app, &b, &a).await;
    assert_eq!(res["matched"], false);
}
