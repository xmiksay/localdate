mod common;

use axum::http::StatusCode;
use chrono::Duration;
use common::{TestApp, Tokens, count};
use localdate_api::cleanup::{CleanupCounts, LOCK_KEY, run_once};
use sea_orm::{ConnectionTrait, TransactionTrait};
use serde_json::json;

const LAT: f64 = 50.0870;
const LON: f64 = 14.4210;

async fn wave(app: &TestApp, from: &Tokens, to: &Tokens) {
    let (status, body) = app
        .post_as(
            "/api/waves",
            &from.access_token,
            json!({ "to_user_id": to.user_id }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "wave failed: {body}");
}

async fn tick(app: &TestApp, offset: Duration) -> CleanupCounts {
    run_once(&app.db, offset)
        .await
        .expect("cleanup runs")
        .expect("lock acquired")
}

async fn windows_with_coords(app: &TestApp) -> i64 {
    count(
        app,
        "SELECT count(*) FROM visibility_window WHERE lat IS NOT NULL OR lon IS NOT NULL",
    )
    .await
}

#[tokio::test]
async fn ending_a_window_clears_its_coordinates() {
    let app = TestApp::new().await;
    let t = app.visible_user("eva", LAT, LON).await;
    assert_eq!(windows_with_coords(&app).await, 1);

    let (status, _) = app.delete("/api/me/window", &t.access_token).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(windows_with_coords(&app).await, 0);
    assert_eq!(
        count(&app, "SELECT count(*) FROM visibility_window").await,
        1
    );
}

#[tokio::test]
async fn expired_window_is_closed_and_deleted_after_a_day() {
    let app = TestApp::new().await;
    let eva = app.visible_user("eva", LAT, LON).await;
    let ida = app.visible_user("ida", LAT, LON).await;
    wave(&app, &eva, &ida).await;
    // eva's window ran out by time an hour ago and was never read again (still open).
    app.sql(
        "UPDATE visibility_window SET starts_at = now() - interval '2 hours', \
         ends_at = now() - interval '1 hour' \
         WHERE user_id = (SELECT id FROM \"user\" WHERE username = 'eva')",
    )
    .await;

    let c = tick(&app, Duration::zero()).await;
    assert_eq!(
        (c.windows_closed, c.waves_deleted, c.windows_deleted),
        (1, 1, 0)
    );
    let eva_closed = count(
        &app,
        "SELECT count(*) FROM visibility_window w JOIN \"user\" u ON u.id = w.user_id \
         WHERE u.username = 'eva' AND w.ended_at = w.ends_at AND w.lat IS NULL AND w.lon IS NULL",
    )
    .await;
    assert_eq!(
        eva_closed, 1,
        "closed like the lazy close, coordinates wiped"
    );
    let ida_active = count(
        &app,
        "SELECT count(*) FROM visibility_window w JOIN \"user\" u ON u.id = w.user_id \
         WHERE u.username = 'ida' AND w.lat IS NOT NULL AND w.lon IS NOT NULL",
    )
    .await;
    assert_eq!(ida_active, 1, "active window untouched");
    assert_eq!(
        count(&app, "SELECT count(*) FROM visibility_window").await,
        2
    );

    // Just under 24 h after eva's end: kept; just past: deleted. ida's (ends in 60 min) is kept.
    let c = tick(&app, Duration::hours(23) - Duration::minutes(2)).await;
    assert_eq!(c.windows_deleted, 0);
    let c = tick(&app, Duration::hours(23) + Duration::minutes(1)).await;
    assert_eq!(c.windows_deleted, 1);
    assert_eq!(
        count(&app, "SELECT count(*) FROM visibility_window").await,
        1
    );
}

#[tokio::test]
async fn early_ended_window_is_retained_from_its_ended_at() {
    let app = TestApp::new().await;
    let t = app.visible_user("eva", LAT, LON).await;
    app.delete("/api/me/window", &t.access_token).await;

    // ends_at is still now + 60 min, but the window really ended now.
    assert_eq!(tick(&app, Duration::hours(23)).await.windows_deleted, 0);
    assert_eq!(
        tick(&app, Duration::hours(24) + Duration::minutes(1))
            .await
            .windows_deleted,
        1
    );
}

#[tokio::test]
async fn matches_and_messages_survive_window_deletion() {
    let app = TestApp::new().await;
    let a = app.visible_user("anna", LAT, LON).await;
    let b = app.visible_user("bara", LAT + 0.0027, LON).await;
    wave(&app, &a, &b).await;
    let (_, res) = app
        .post_as(
            "/api/waves",
            &b.access_token,
            json!({ "to_user_id": a.user_id }),
        )
        .await;
    let id = res["match_id"].as_str().expect("match id").to_owned();
    let (status, _) = app
        .post_as(
            &format!("/api/matches/{id}/messages"),
            &a.access_token,
            json!({ "body": "ahoj" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let c = tick(&app, Duration::hours(26)).await;
    assert_eq!(c.windows_deleted, 2);
    assert_eq!(
        count(&app, "SELECT count(*) FROM visibility_window").await,
        0
    );
    assert_eq!(count(&app, "SELECT count(*) FROM \"match\"").await, 1);
    assert_eq!(count(&app, "SELECT count(*) FROM message").await, 1);
}

#[tokio::test]
async fn unanswered_waves_are_deleted_once_expired() {
    let app = TestApp::new().await;
    let a = app.visible_user("anna", LAT, LON).await;
    let b = app.visible_user("bara", LAT + 0.0027, LON).await;
    wave(&app, &a, &b).await;

    assert_eq!(tick(&app, Duration::zero()).await.waves_deleted, 0);
    assert_eq!(count(&app, "SELECT count(*) FROM wave").await, 1);

    // The wave expires with anna's 60 min window; the window row itself is still kept.
    let c = tick(&app, Duration::minutes(61)).await;
    assert_eq!((c.waves_deleted, c.windows_deleted), (1, 0));
    assert_eq!(count(&app, "SELECT count(*) FROM wave").await, 0);
}

#[tokio::test]
async fn refresh_tokens_expired_or_long_revoked_are_deleted() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;
    // Rotation revokes the registration token; it must stay to keep reuse detection working.
    let (status, rotated) = app
        .post(
            "/api/auth/refresh",
            json!({ "refresh_token": t.refresh_token }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    app.sql(&format!(
        "INSERT INTO refresh_token (id, user_id, token_hash, family_id, expires_at, revoked_at) VALUES \
         (gen_random_uuid(), '{u}', 'old-revoked', gen_random_uuid(), now() + interval '20 days', now() - interval '8 days'), \
         (gen_random_uuid(), '{u}', 'recent-revoked', gen_random_uuid(), now() + interval '20 days', now() - interval '6 days'), \
         (gen_random_uuid(), '{u}', 'expired', gen_random_uuid(), now() - interval '1 minute', NULL)",
        u = t.user_id
    ))
    .await;

    let c = tick(&app, Duration::zero()).await;
    assert_eq!(c.tokens_deleted, 2);
    let left = count(
        &app,
        "SELECT count(*) FROM refresh_token WHERE token_hash IN ('old-revoked', 'expired')",
    )
    .await;
    assert_eq!(left, 0);
    assert_eq!(count(&app, "SELECT count(*) FROM refresh_token").await, 3);

    // Replaying the (recently revoked) first token still revokes the live successor.
    let (status, _) = app
        .post(
            "/api/auth/refresh",
            json!({ "refresh_token": t.refresh_token }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = app
        .post(
            "/api/auth/refresh",
            json!({ "refresh_token": rotated["refresh_token"] }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn tick_is_skipped_while_another_replica_holds_the_lock() {
    let app = TestApp::new().await;
    let other = app.db.begin().await.expect("begin");
    other
        .execute_unprepared(&format!("SELECT pg_advisory_xact_lock({LOCK_KEY})"))
        .await
        .expect("lock");

    let skipped = run_once(&app.db, Duration::zero())
        .await
        .expect("cleanup runs");
    assert_eq!(skipped, None);

    other.rollback().await.expect("rollback");
    assert!(
        run_once(&app.db, Duration::zero())
            .await
            .expect("cleanup runs")
            .is_some()
    );
}

#[tokio::test]
async fn an_open_window_cannot_lose_its_coordinates() {
    let app = TestApp::new().await;
    app.visible_user("eva", LAT, LON).await;
    let res = app
        .db
        .execute_unprepared("UPDATE visibility_window SET lat = NULL, lon = NULL")
        .await;
    assert!(res.is_err(), "CHECK visibility_window_open_has_coords");
}

#[tokio::test]
async fn location_and_extend_never_touch_an_ended_window() {
    let app = TestApp::new().await;
    let t = app.visible_user("eva", LAT, LON).await;
    // Ran out by time but not closed yet: the location write must not land on it.
    app.sql("UPDATE visibility_window SET ends_at = now() - interval '1 minute'")
        .await;
    let loc = json!({ "lat": 50.5, "lon": 14.5 });
    let (status, _) = app
        .post_as("/api/me/location", &t.access_token, loc.clone())
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = app
        .patch(
            "/api/me/window",
            &t.access_token,
            json!({ "extend_minutes": 30 }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        count(
            &app,
            "SELECT count(*) FROM visibility_window WHERE lat = 50.5"
        )
        .await,
        0
    );

    tick(&app, Duration::zero()).await;
    let (status, _) = app.post_as("/api/me/location", &t.access_token, loc).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(windows_with_coords(&app).await, 0);
}

#[tokio::test]
async fn long_revoked_token_is_kept_while_its_family_is_live() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;
    let (_, rotated) = app
        .post(
            "/api/auth/refresh",
            json!({ "refresh_token": t.refresh_token }),
        )
        .await;
    app.sql("UPDATE refresh_token SET revoked_at = now() - interval '8 days' WHERE revoked_at IS NOT NULL")
        .await;

    assert_eq!(tick(&app, Duration::zero()).await.tokens_deleted, 0);
    // The replay still burns the family, including the live successor.
    let (status, _) = app
        .post(
            "/api/auth/refresh",
            json!({ "refresh_token": t.refresh_token }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = app
        .post(
            "/api/auth/refresh",
            json!({ "refresh_token": rotated["refresh_token"] }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
