mod common;

use axum::http::StatusCode;
use common::{TestApp, Tokens};
use serde_json::{Value, json};

const LAT: f64 = 50.0870;
const LON: f64 = 14.4210;
/// About 100 m and 300 m of latitude, i.e. bands `lt_200m` and `lt_500m`.
const D100: f64 = 0.0009;
const D300: f64 = 0.0027;

async fn nearby(app: &TestApp, t: &Tokens) -> Vec<Value> {
    let (status, body) = app.get("/api/nearby", Some(&t.access_token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body.as_array().expect("array").clone()
}

async fn set_interests(app: &TestApp, t: &Tokens, ids: &[i32]) {
    for id in ids {
        app.sql(&format!(
            "INSERT INTO user_interest (user_id, interest_id) VALUES ('{}', {id})",
            t.user_id
        ))
        .await;
    }
}

fn shared_of<'a>(list: &'a [Value], t: &Tokens) -> &'a Value {
    let p = list
        .iter()
        .find(|p| p["user_id"] == t.user_id.to_string())
        .expect("user listed");
    &p["shared_interests"]
}

fn order(list: &[Value]) -> Vec<String> {
    list.iter()
        .map(|p| p["user_id"].as_str().expect("id").to_owned())
        .collect()
}

#[tokio::test]
async fn sorted_by_shared_count_then_band() {
    let app = TestApp::new().await;
    let me = app.visible_user("anna", LAT, LON).await;
    let far_two = app.visible_user("fartwo", LAT + D300, LON).await;
    let near_one = app.visible_user("nearone", LAT - D100, LON).await;
    let far_one = app.visible_user("farone", LAT - D300, LON).await;
    let near_none = app.visible_user("nearnone", LAT + D100, LON).await;
    let far_empty = app.visible_user("farempty", LAT, LON + 0.004).await;
    set_interests(&app, &me, &[3, 1, 2]).await;
    set_interests(&app, &far_two, &[2, 1, 9]).await;
    set_interests(&app, &near_one, &[3, 7]).await;
    set_interests(&app, &far_one, &[1, 8]).await;
    set_interests(&app, &near_none, &[9]).await;
    // far_one opened later than near_one: the band, not the window age, must break the tie.
    app.sql(&format!(
        "UPDATE visibility_window SET starts_at = now() - interval '10 minutes' WHERE user_id = '{}'",
        near_one.user_id
    ))
    .await;

    let list = nearby(&app, &me).await;
    let expected: Vec<String> = [&far_two, &near_one, &far_one, &near_none, &far_empty]
        .iter()
        .map(|t| t.user_id.to_string())
        .collect();
    assert_eq!(order(&list), expected);

    assert_eq!(shared_of(&list, &far_two), &json!([1, 2]));
    assert_eq!(shared_of(&list, &near_one), &json!([3]));
    assert_eq!(shared_of(&list, &far_one), &json!([1]));
    assert_eq!(shared_of(&list, &near_none), &json!([]));
    assert_eq!(shared_of(&list, &far_empty), &json!([]));
    // The full interest list is still returned, not just the overlap.
    assert_eq!(list[0]["interests"].as_array().map(Vec::len), Some(3));
    assert_eq!(list[0]["distance_band"], "lt_500m");
    assert_eq!(list[1]["distance_band"], "lt_200m");

    // Overlap is symmetric from the other side.
    let seen_by_far_two = nearby(&app, &far_two).await;
    assert_eq!(shared_of(&seen_by_far_two, &me), &json!([1, 2]));
}

#[tokio::test]
async fn viewer_without_interests_falls_back_to_band_order() {
    let app = TestApp::new().await;
    let me = app.visible_user("anna", LAT, LON).await;
    let far = app.visible_user("far", LAT + D300, LON).await;
    let near = app.visible_user("near", LAT + D100, LON).await;
    set_interests(&app, &far, &[1, 2, 3]).await;
    set_interests(&app, &near, &[4]).await;

    let list = nearby(&app, &me).await;
    assert_eq!(
        order(&list),
        [near.user_id.to_string(), far.user_id.to_string()]
    );
    assert!(list.iter().all(|p| p["shared_interests"] == json!([])));
}

#[tokio::test]
async fn incoming_waves_carry_shared_interests() {
    let app = TestApp::new().await;
    let me = app.visible_user("anna", LAT, LON).await;
    let bob = app.visible_user("bob", LAT + D100, LON).await;
    set_interests(&app, &me, &[5, 6]).await;
    set_interests(&app, &bob, &[6]).await;
    let (status, body) = app
        .post_as(
            "/api/waves",
            &bob.access_token,
            json!({ "to_user_id": me.user_id }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = app.get("/api/waves/incoming", Some(&me.access_token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let list = body.as_array().expect("array");
    assert_eq!(shared_of(list, &bob), &json!([6]));
}
