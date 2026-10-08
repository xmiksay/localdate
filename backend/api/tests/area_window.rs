//! Area windows: start inside/outside, same-area visibility, auto-end on leaving, deactivation.

mod common;

use axum::http::StatusCode;
use common::{TestApp, Tokens, count};
use localdate_api::discovery::rules::AREA_STALE_SECS;
use serde_json::{Value, json};
use uuid::Uuid;

const LAT: f64 = 50.0830;
const LON: f64 = 14.4350;
/// One metre of latitude in degrees (haversine on the 6371 km sphere).
const M: f64 = 1.0 / 111_194.93;

async fn nearby_ids(app: &TestApp, t: &Tokens) -> Vec<String> {
    let (status, body) = app.get("/api/nearby", Some(&t.access_token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body.as_array()
        .expect("array")
        .iter()
        .map(|p| p["user_id"].as_str().expect("id").to_owned())
        .collect()
}

async fn move_to(app: &TestApp, t: &Tokens, lat: f64) -> (StatusCode, Value) {
    app.post_as(
        "/api/me/location",
        &t.access_token,
        json!({ "lat": lat, "lon": LON }),
    )
    .await
}

async fn window(app: &TestApp, t: &Tokens) -> Value {
    app.get("/api/me/window", Some(&t.access_token)).await.1
}

#[tokio::test]
async fn start_checks_kind_area_and_position() {
    let app = TestApp::new().await;
    let area = app.area("Nádraží", LAT, LON, 300).await;
    let t = app.register("anna").await;
    app.onboard(&t, "female", "1995-05-05").await;

    let (status, err) = app.start_area_window(&t, area, LAT + 301.0 * M, LON).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(err["error"]["code"], "outside_area");
    let (status, _) = app.start_area_window(&t, Uuid::new_v4(), LAT, LON).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    for body in [
        json!({ "kind": "area", "minutes": 60, "lat": LAT, "lon": LON }),
        json!({ "kind": "timed", "area_id": area, "minutes": 60, "lat": LAT, "lon": LON }),
        json!({ "area_id": area, "minutes": 60, "lat": LAT, "lon": LON }),
        json!({ "kind": "nowhere", "minutes": 60, "lat": LAT, "lon": LON }),
        json!({ "kind": "area", "area_id": area, "minutes": 45, "lat": LAT, "lon": LON }),
    ] {
        let (status, err) = app.post_as("/api/me/window", &t.access_token, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{err}");
    }
    assert_eq!(
        count(&app, "SELECT count(*) FROM visibility_window").await,
        0
    );

    let (status, w) = app.start_area_window(&t, area, LAT + 299.0 * M, LON).await;
    assert_eq!(status, StatusCode::CREATED, "{w}");
    assert_eq!(w["kind"], "area");
    assert_eq!(w["area"], json!({ "id": area, "name": "Nádraží" }));
    assert_eq!(window(&app, &t).await["area"]["name"], "Nádraží");
    let (status, extended) = app
        .patch(
            "/api/me/window",
            &t.access_token,
            json!({ "extend_minutes": 30 }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(extended["area"]["id"], json!(area));

    // The old body still opens a timed window, replacing the area one.
    let w = app.open_window(&t, LAT, LON).await;
    assert_eq!(w["kind"], "timed");
    assert!(w["area"].is_null());
}

#[tokio::test]
async fn area_windows_see_only_the_same_area() {
    let app = TestApp::new().await;
    let centre = app.area("Centrum", LAT, LON, 1000).await;
    let station = app.area("Nádraží", LAT + 100.0 * M, LON, 1000).await;
    let anna = app.area_user("anna", centre, LAT, LON).await;
    // ~800 m away with a 200 m filter: inside one area that does not matter.
    let bob = app.area_user("bob", centre, LAT + 800.0 * M, LON).await;
    app.sql(&format!(
        "UPDATE filter SET max_distance_m = 200 WHERE user_id = '{}'",
        bob.user_id
    ))
    .await;
    let carl = app.area_user("carl", station, LAT + 50.0 * M, LON).await;
    let dave = app.visible_user("dave", LAT + 20.0 * M, LON).await;

    assert_eq!(nearby_ids(&app, &anna).await, [bob.user_id.to_string()]);
    assert_eq!(nearby_ids(&app, &bob).await, [anna.user_id.to_string()]);
    assert!(nearby_ids(&app, &carl).await.is_empty());
    assert!(nearby_ids(&app, &dave).await.is_empty());

    let (_, list) = app.get("/api/nearby", Some(&anna.access_token)).await;
    assert_eq!(list[0]["area"], json!({ "id": centre, "name": "Centrum" }));
    assert!(list[0]["distance_band"].is_null());
    assert!(list[0].get("lat").is_none() && list[0].get("lon").is_none());

    // Waving follows the same rule.
    let (status, _) = app
        .post_as(
            "/api/waves",
            &anna.access_token,
            json!({ "to_user_id": carl.user_id }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = app
        .post_as(
            "/api/waves",
            &anna.access_token,
            json!({ "to_user_id": bob.user_id }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Timed windows keep the distance rule among themselves.
    let eva = app.visible_user("eva", LAT + 300.0 * M, LON).await;
    assert_eq!(nearby_ids(&app, &dave).await, [eva.user_id.to_string()]);
    let (_, list) = app.get("/api/nearby", Some(&dave.access_token)).await;
    assert!(list[0]["area"].is_null());
    assert_eq!(list[0]["distance_band"], "lt_500m");
}

#[tokio::test]
async fn leaving_the_area_ends_the_window_after_the_margin() {
    let app = TestApp::new().await;
    let area = app.area("Nádraží", LAT, LON, 300).await;
    let anna = app.area_user("anna", area, LAT, LON).await;
    let bob = app.area_user("bob", area, LAT, LON).await;
    app.post_as(
        "/api/waves",
        &anna.access_token,
        json!({ "to_user_id": bob.user_id }),
    )
    .await;

    // Outside the circle but within the 100 m margin: the window stays.
    for metres in [350.0, 399.0] {
        let (status, body) = move_to(&app, &anna, LAT + metres * M).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{metres} m: {body}");
    }
    assert_eq!(window(&app, &anna).await["kind"], "area");
    // A new window cannot start there, though.
    let (status, _) = app
        .start_area_window(&bob, area, LAT + 350.0 * M, LON)
        .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, err) = move_to(&app, &anna, LAT + 401.0 * M).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(err["error"]["code"], "left_area");
    assert!(window(&app, &anna).await.is_null());
    let ended = format!(
        "SELECT count(*) FROM visibility_window WHERE user_id = '{}' \
         AND ended_at IS NOT NULL AND lat IS NULL AND lon IS NULL",
        anna.user_id
    );
    assert_eq!(count(&app, &ended).await, 1);
    assert_eq!(count(&app, "SELECT count(*) FROM wave").await, 0);
    assert!(nearby_ids(&app, &bob).await.is_empty());

    let (status, err) = move_to(&app, &anna, LAT).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(err["error"]["code"], "no_active_window");
}

#[tokio::test]
async fn deactivated_area_keeps_running_windows_but_refuses_new_ones() {
    let app = TestApp::new().await;
    let admin = app.admin("mod").await;
    let area = app.area("Nádraží", LAT, LON, 300).await;
    let anna = app.area_user("anna", area, LAT, LON).await;
    let bob = app.area_user("bob", area, LAT + 100.0 * M, LON).await;
    let carl = app.register("carl").await;
    app.onboard(&carl, "female", "1995-05-05").await;

    let path = format!("/api/admin/areas/{area}");
    let mut body = json!({
        "name": "Nádraží", "kind": "train_station", "lat": LAT, "lon": LON,
        "radius_m": 300, "active": false,
    });
    let (status, _) = app.put(&path, &admin.access_token, body.clone()).await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(nearby_ids(&app, &anna).await, [bob.user_id.to_string()]);
    let (status, _) = move_to(&app, &anna, LAT + 50.0 * M).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = app.start_area_window(&carl, area, LAT, LON).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, found) = app
        .get(
            &format!("/api/areas?lat={LAT}&lon={LON}"),
            Some(&carl.access_token),
        )
        .await;
    assert_eq!(found, json!([]));

    // Shrinking the area applies to running windows at the next location update.
    body["radius_m"] = json!(50);
    app.put(&path, &admin.access_token, body).await;
    let (status, err) = move_to(&app, &bob, LAT + 160.0 * M).await;
    assert_eq!(status, StatusCode::CONFLICT, "{err}");
    assert_eq!(err["error"]["code"], "left_area");
}

async fn move_with_accuracy(
    app: &TestApp,
    t: &Tokens,
    lat: f64,
    accuracy: f64,
) -> (StatusCode, Value) {
    app.post_as(
        "/api/me/location",
        &t.access_token,
        json!({ "lat": lat, "lon": LON, "accuracy": accuracy }),
    )
    .await
}

#[tokio::test]
async fn a_coarse_fix_widens_the_leave_margin_up_to_the_cap() {
    let app = TestApp::new().await;
    let area = app.area("Nádraží", LAT, LON, 300).await;
    let anna = app.area_user("anna", area, LAT, LON).await;

    // 450 m out is past radius + 100 m, but within the fix's 200 m uncertainty: stays.
    let (status, body) = move_with_accuracy(&app, &anna, LAT + 450.0 * M, 200.0).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    // An absurd accuracy is capped at 500 m.
    let (status, _) = move_with_accuracy(&app, &anna, LAT + 790.0 * M, 1e9).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, err) = move_with_accuracy(&app, &anna, LAT + 801.0 * M, 1e9).await;
    assert_eq!(err["error"]["code"], "left_area", "{status}");

    let bob = app.area_user("bob", area, LAT, LON).await;
    for bad in [json!(-1.0), json!("far")] {
        let (status, _) = app
            .post_as(
                "/api/me/location",
                &bob.access_token,
                json!({ "lat": LAT, "lon": LON, "accuracy": bad }),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    // The same 450 m with an accurate fix ends the window.
    let (status, err) = move_with_accuracy(&app, &bob, LAT + 450.0 * M, 10.0).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(err["error"]["code"], "left_area");
}

#[tokio::test]
async fn a_stale_area_window_is_hidden_until_the_next_location_update() {
    let app = TestApp::new().await;
    let area = app.area("Nádraží", LAT, LON, 300).await;
    let anna = app.area_user("anna", area, LAT, LON).await;
    let bob = app.area_user("bob", area, LAT, LON).await;
    // Timed windows have no staleness rule.
    let carl = app.visible_user("carl", LAT, LON).await;
    let dave = app.visible_user("dave", LAT, LON).await;
    app.sql(&format!(
        "UPDATE visibility_window SET location_updated_at = now() - interval '{} seconds' \
         WHERE user_id IN ('{}', '{}')",
        AREA_STALE_SECS + 1,
        anna.user_id,
        carl.user_id
    ))
    .await;

    assert!(nearby_ids(&app, &anna).await.is_empty());
    assert!(nearby_ids(&app, &bob).await.is_empty());
    assert_eq!(nearby_ids(&app, &dave).await, [carl.user_id.to_string()]);

    let (status, _) = move_to(&app, &anna, LAT).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(nearby_ids(&app, &bob).await, [anna.user_id.to_string()]);
}
