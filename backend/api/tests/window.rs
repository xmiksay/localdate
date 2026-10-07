mod common;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use common::TestApp;
use serde_json::{Value, json};

fn ends_at(w: &Value) -> DateTime<Utc> {
    w["ends_at"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .expect("ends_at")
}

#[tokio::test]
async fn start_requires_a_complete_profile() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;
    let body = json!({ "minutes": 60, "lat": 50.0, "lon": 14.0 });
    let (status, err) = app
        .post_as("/api/me/window", &t.access_token, body.clone())
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err["error"]["code"], "profile_incomplete");

    app.onboard(&t, "female", "1995-05-05").await;
    app.sql("DELETE FROM photo").await;
    let (status, err) = app.post_as("/api/me/window", &t.access_token, body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{err}");
}

#[tokio::test]
async fn start_validates_input() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;
    app.onboard(&t, "female", "1995-05-05").await;
    for body in [
        json!({ "minutes": 45, "lat": 50.0, "lon": 14.0 }),
        json!({ "minutes": 60, "lat": 91.0, "lon": 14.0 }),
        json!({ "minutes": 60, "lat": 50.0, "lon": -181.0 }),
        json!({ "minutes": 60 }),
    ] {
        let (status, err) = app
            .post_as("/api/me/window", &t.access_token, body.clone())
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(err["error"]["code"], "validation");
    }
    let (status, err) = app
        .patch(
            "/api/me/window",
            &t.access_token,
            json!({ "extend_minutes": 5 }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{err}");
}

#[tokio::test]
async fn lifecycle_start_extend_cap_end() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;
    app.onboard(&t, "female", "1995-05-05").await;

    let (status, none) = app.get("/api/me/window", Some(&t.access_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(none.is_null());

    let started = app.open_window(&t, 50.0870, 14.4210).await;
    assert_eq!(started["kind"], "timed");
    assert_eq!(started["waves_left"], 20);
    let end = ends_at(&started);
    assert!(
        (end - Utc::now() - Duration::minutes(60))
            .num_seconds()
            .abs()
            < 5
    );

    let (status, current) = app.get("/api/me/window", Some(&t.access_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(current["id"], started["id"]);

    let (status, extended) = app
        .patch(
            "/api/me/window",
            &t.access_token,
            json!({ "extend_minutes": 30 }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{extended}");
    assert_eq!(ends_at(&extended) - end, Duration::minutes(30));

    // 11 h left + 4 h would pass the cap of 12 h from now.
    app.sql("UPDATE visibility_window SET ends_at = now() + interval '11 hours'")
        .await;
    let (_, capped) = app
        .patch(
            "/api/me/window",
            &t.access_token,
            json!({ "extend_minutes": 240 }),
        )
        .await;
    let left = ends_at(&capped) - Utc::now();
    assert!(left <= Duration::hours(12) && left > Duration::hours(12) - Duration::seconds(10));

    for _ in 0..2 {
        let (status, _) = app.delete("/api/me/window", &t.access_token).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }
    let (_, gone) = app.get("/api/me/window", Some(&t.access_token)).await;
    assert!(gone.is_null());
    let (status, err) = app
        .patch(
            "/api/me/window",
            &t.access_token,
            json!({ "extend_minutes": 30 }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(err["error"]["code"], "no_active_window");
}

#[tokio::test]
async fn restarting_ends_the_previous_window() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;
    app.onboard(&t, "female", "1995-05-05").await;
    let first = app.open_window(&t, 50.0, 14.0).await;
    let second = app.open_window(&t, 50.0, 14.0).await;
    assert_ne!(first["id"], second["id"]);

    let (_, current) = app.get("/api/me/window", Some(&t.access_token)).await;
    assert_eq!(current["id"], second["id"]);
    let open = common::count(
        &app,
        "SELECT count(*) FROM visibility_window WHERE ended_at IS NULL",
    )
    .await;
    let all = common::count(&app, "SELECT count(*) FROM visibility_window").await;
    assert_eq!((open, all), (1, 2));
}

#[tokio::test]
async fn expired_window_is_closed_lazily() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;
    app.onboard(&t, "female", "1995-05-05").await;
    app.open_window(&t, 50.0, 14.0).await;
    app.sql("UPDATE visibility_window SET ends_at = now() - interval '1 minute'")
        .await;

    let (_, current) = app.get("/api/me/window", Some(&t.access_token)).await;
    assert!(current.is_null());
    let open = common::count(
        &app,
        "SELECT count(*) FROM visibility_window WHERE ended_at IS NULL",
    )
    .await;
    assert_eq!(open, 0);
    // A fresh window can be started afterwards.
    app.open_window(&t, 50.0, 14.0).await;
}

#[tokio::test]
async fn location_needs_a_window_and_is_rounded() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;
    app.onboard(&t, "female", "1995-05-05").await;
    let body = json!({ "lat": 50.123_456, "lon": 14.987_654 });
    let (status, err) = app
        .post_as("/api/me/location", &t.access_token, body.clone())
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(err["error"]["code"], "no_active_window");

    app.open_window(&t, 50.0, 14.0).await;
    let (status, _) = app.post_as("/api/me/location", &t.access_token, body).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let hit = common::count(
        &app,
        "SELECT count(*) FROM visibility_window WHERE lat = 50.123 AND lon = 14.988",
    )
    .await;
    assert_eq!(hit, 1);

    let (status, _) = app
        .post_as(
            "/api/me/location",
            &t.access_token,
            json!({ "lat": 95.0, "lon": 0.0 }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
