mod common;

use std::collections::BTreeSet;
use std::net::SocketAddr;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Method, Request, StatusCode, header};
use common::{TestApp, Tokens};
use serde_json::{Value, json};
use tower::ServiceExt;

const LAT: f64 = 50.0870;
const LON: f64 = 14.4210;
/// 0.0027 degrees of latitude is about 300 m.
const D300: f64 = 0.0027;
/// About 1 km of latitude.
const KM: f64 = 0.009;

/// `GET /preview` as a guest (no token) at LAT/LON plus `dlat`.
async fn preview(app: &TestApp, dlat: f64, query: &str) -> Value {
    let (status, body) = app
        .get(
            &format!("/api/preview?lat={}&lon={LON}&{query}", LAT + dlat),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

fn total(body: &Value) -> i64 {
    body["total"].as_i64().expect("total")
}

fn set_filter(app_user: &Tokens, assignments: &str) -> String {
    format!(
        "UPDATE filter SET {assignments} WHERE user_id = '{}'",
        app_user.user_id
    )
}

/// Onboarded with `gender` and an open timed window at LAT/LON plus `dlat`.
async fn windowed(app: &TestApp, name: &str, gender: &str, dlat: f64) -> Tokens {
    let t = app.register(name).await;
    app.onboard(&t, gender, "1995-05-05").await;
    app.open_window(&t, LAT + dlat, LON).await;
    t
}

#[tokio::test]
async fn guest_sees_a_reduced_card_without_logging_in() {
    let app = TestApp::new().await;
    app.visible_user("anna", LAT, LON).await;
    // Without a window nobody shows up, onboarded or not.
    let idle = app.register("idle").await;
    app.onboard(&idle, "female", "1995-05-05").await;

    let body = preview(&app, D300, "gender=male").await;
    assert_eq!(total(&body), 1);
    let item = &body["items"][0];
    let keys: BTreeSet<&str> = item
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        BTreeSet::from([
            "display_name",
            "age",
            "gender",
            "photo_url",
            "distance_band",
            "area"
        ])
    );
    assert_eq!(item["display_name"], "Tester");
    assert_eq!(item["gender"], "female");
    assert_eq!(item["distance_band"], "lt_500m");
    assert_eq!(item["area"], Value::Null);
    assert!(
        item["photo_url"]
            .as_str()
            .expect("url")
            .starts_with("/media/")
    );
    assert!(item["age"].as_i64().expect("age") >= 18);
}

#[tokio::test]
async fn distance_is_capped_by_their_max_and_five_km() {
    let app = TestApp::new().await;
    let far = windowed(&app, "far", "female", 3.0 * KM).await;
    // Default max distance is 2 km.
    assert_eq!(total(&preview(&app, 0.0, "gender=male").await), 0);
    app.sql(&set_filter(&far, "max_distance_m = 10000")).await;
    assert_eq!(total(&preview(&app, 0.0, "gender=male").await), 1);
    // 6 km from the guest: past the guest cap even though their own max allows it.
    assert_eq!(total(&preview(&app, -3.0 * KM, "gender=male").await), 0);
}

#[tokio::test]
async fn gender_filters_apply_both_ways() {
    let app = TestApp::new().await;
    let anna = app.visible_user("anna", LAT, LON).await;
    app.sql(&set_filter(&anna, "genders = '{male}'")).await;

    assert_eq!(total(&preview(&app, D300, "gender=male").await), 1);
    assert_eq!(total(&preview(&app, D300, "gender=female").await), 0);
    // The guest's own choice of who to see.
    assert_eq!(
        total(&preview(&app, D300, "gender=male&genders=male").await),
        0
    );
    assert_eq!(
        total(&preview(&app, D300, "gender=male&genders=female,other").await),
        1
    );
    assert_eq!(total(&preview(&app, D300, "gender=male&genders=").await), 1);
}

#[tokio::test]
async fn area_windows_count_only_inside_the_circle_while_fresh() {
    let app = TestApp::new().await;
    let area = app.area("Hlavni nadrazi", LAT, LON, 500).await;
    let eva = app.area_user("eva", area, LAT, LON).await;

    let inside = preview(&app, D300, "gender=male").await;
    assert_eq!(total(&inside), 1);
    assert_eq!(inside["items"][0]["distance_band"], Value::Null);
    assert_eq!(inside["items"][0]["area"]["name"], "Hlavni nadrazi");
    // 900 m from the centre: outside the circle, though within eva's 2 km.
    assert_eq!(total(&preview(&app, KM, "gender=male").await), 0);

    app.sql(&format!(
        "UPDATE visibility_window SET location_updated_at = now() - interval '11 minutes' \
         WHERE user_id = '{}'",
        eva.user_id
    ))
    .await;
    assert_eq!(total(&preview(&app, D300, "gender=male").await), 0);
}

#[tokio::test]
async fn banned_photo_less_and_ended_windows_are_left_out() {
    let app = TestApp::new().await;
    let banned = app.visible_user("banned", LAT, LON).await;
    let bare = app.visible_user("bare", LAT, LON).await;
    let ended = app.visible_user("ended", LAT, LON).await;
    assert_eq!(total(&preview(&app, 0.0, "gender=male").await), 3);

    app.sql(&format!(
        "UPDATE \"user\" SET banned_at = now() WHERE id = '{}'",
        banned.user_id
    ))
    .await;
    app.sql(&format!(
        "DELETE FROM photo WHERE user_id = '{}'",
        bare.user_id
    ))
    .await;
    let (status, _) = app.delete("/api/me/window", &ended.access_token).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(total(&preview(&app, 0.0, "gender=male").await), 0);
}

#[tokio::test]
async fn at_most_ten_cards_nearest_first_with_the_full_total() {
    let app = TestApp::new().await;
    for i in 0..11 {
        windowed(&app, &format!("u{i}"), "female", 0.0).await;
    }
    windowed(&app, "farther", "male", 1.5 * KM).await;

    let body = preview(&app, 0.0, "gender=male").await;
    assert_eq!(total(&body), 12);
    let items = body["items"].as_array().expect("items");
    assert_eq!(items.len(), 10);
    assert!(items.iter().all(|p| p["distance_band"] == "lt_200m"));
}

#[tokio::test]
async fn bad_parameters_are_400() {
    let app = TestApp::new().await;
    for query in [
        format!("lat={LAT}&lon={LON}"),
        format!("lat={LAT}&lon={LON}&gender=robot"),
        format!("lat={LAT}&lon={LON}&gender=male&genders=female,robot"),
        format!("lat=91&lon={LON}&gender=male"),
        format!("lon={LON}&gender=male"),
    ] {
        let (status, body) = app.get(&format!("/api/preview?{query}"), None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}: {body}");
        assert_eq!(body["error"]["code"], "validation", "{query}");
    }
}

/// A request from one client address, as `into_make_service_with_connect_info` would see it.
async fn from_peer(app: &TestApp, method: Method, uri: &str, body: Option<Value>) -> StatusCode {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
        .expect("build request");
    let peer: SocketAddr = "10.0.0.7:40000".parse().expect("addr");
    req.extensions_mut().insert(ConnectInfo(peer));
    app.router
        .clone()
        .oneshot(req)
        .await
        .expect("infallible")
        .status()
}

#[tokio::test]
async fn preview_is_rate_limited_apart_from_login() {
    let app = TestApp::with_config(|c| c.rate_limit = true).await;
    let uri = format!("/api/preview?lat={LAT}&lon={LON}&gender=male");
    let mut statuses = vec![];
    for _ in 0..6 {
        statuses.push(from_peer(&app, Method::GET, &uri, None).await);
    }
    assert_eq!(statuses[..5], [StatusCode::OK; 5]);
    assert_eq!(statuses[5], StatusCode::TOO_MANY_REQUESTS);
    // The same client still has its whole login budget.
    let login = from_peer(
        &app,
        Method::POST,
        "/api/auth/login",
        Some(json!({ "username": "nobody", "password": "wrong password" })),
    )
    .await;
    assert_eq!(login, StatusCode::UNAUTHORIZED);
}
