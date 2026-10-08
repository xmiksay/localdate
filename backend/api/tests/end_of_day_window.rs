//! Windows started with `until: 'end_of_day'` instead of `minutes`.

mod common;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use common::TestApp;
use localdate_api::discovery::duration::next_local_midnight;
use serde_json::{Value, json};

const LAT: f64 = 50.0830;
const LON: f64 = 14.4350;

fn ends_at(w: &Value) -> DateTime<Utc> {
    w["ends_at"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .expect("ends_at")
}

fn next_midnight(tz: Tz, now: DateTime<Utc>) -> DateTime<Utc> {
    next_local_midnight(now, tz).expect("a next day")
}

/// A fixed-offset zone whose midnight is 2–11 h away, so the test neither hits the 30 min refusal
/// nor the 12 h cap whatever time it runs at.
fn zone_with_midnight_ahead(now: DateTime<Utc>) -> Tz {
    (-12..=14)
        .map(|h: i32| {
            // POSIX naming: Etc/GMT-2 is UTC+2.
            let name = if h >= 0 {
                format!("Etc/GMT-{h}")
            } else {
                format!("Etc/GMT+{}", -h)
            };
            name.parse::<Tz>().expect("Etc zone")
        })
        .find(|tz| {
            let left = next_midnight(*tz, now) - now;
            left >= Duration::hours(2) && left <= Duration::hours(11)
        })
        .expect("some offset puts midnight 2-11 h ahead")
}

#[tokio::test]
async fn timed_and_area_windows_can_run_until_local_midnight() {
    let app = TestApp::new().await;
    let area = app.area("Náměstí", LAT, LON, 300).await;
    let t = app.register("eva").await;
    app.onboard(&t, "female", "1995-05-05").await;
    let tz = zone_with_midnight_ahead(Utc::now());
    let midnight = next_midnight(tz, Utc::now());

    let (status, w) = app
        .post_as(
            "/api/me/window",
            &t.access_token,
            json!({ "until": "end_of_day", "tz": tz.name(), "lat": LAT, "lon": LON }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{w}");
    assert_eq!(w["kind"], "timed");
    assert_eq!(ends_at(&w), midnight);

    // Extending works as for any window.
    let (status, w) = app
        .patch(
            "/api/me/window",
            &t.access_token,
            json!({ "extend_minutes": 30 }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{w}");
    assert_eq!(ends_at(&w), midnight + Duration::minutes(30));

    let (status, w) = app
        .post_as(
            "/api/me/window",
            &t.access_token,
            json!({ "kind": "area", "area_id": area, "until": "end_of_day", "tz": tz.name(),
                    "lat": LAT, "lon": LON }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{w}");
    assert_eq!(w["kind"], "area");
    assert_eq!(ends_at(&w), midnight);
}

#[tokio::test]
async fn end_of_day_input_is_validated() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;
    app.onboard(&t, "female", "1995-05-05").await;
    for body in [
        json!({ "until": "end_of_day", "tz": "Europe/Atlantis", "lat": LAT, "lon": LON }),
        json!({ "until": "end_of_day", "lat": LAT, "lon": LON }),
        json!({ "until": "end_of_day", "tz": "Europe/Prague", "minutes": 60, "lat": LAT, "lon": LON }),
        json!({ "minutes": 60, "tz": "Europe/Prague", "lat": LAT, "lon": LON }),
        json!({ "until": "end_of_week", "tz": "Europe/Prague", "lat": LAT, "lon": LON }),
        json!({ "lat": LAT, "lon": LON }),
    ] {
        let (status, err) = app
            .post_as("/api/me/window", &t.access_token, body.clone())
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}: {err}");
        assert_eq!(err["error"]["code"], "validation");
    }
    let (_, current) = app.get("/api/me/window", Some(&t.access_token)).await;
    assert!(current.is_null());
}
