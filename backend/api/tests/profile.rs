mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::json;

fn profile_body(birth: &str, interests: serde_json::Value) -> serde_json::Value {
    json!({
        "display_name": "  Eva ", "birth_date": birth, "gender": "female",
        "bio": "hi", "interest_ids": interests,
    })
}

#[tokio::test]
async fn me_before_and_after_onboarding() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;

    let (status, body) = app.get("/api/me", Some(&t.access_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["username"], "eva");
    assert!(body["profile"].is_null());
    assert!(body["filter"].is_null());

    app.onboard(&t, "female", "1995-05-05").await;
    let (_, body) = app.get("/api/me", Some(&t.access_token)).await;
    assert_eq!(body["profile"]["display_name"], "Tester");
    assert_eq!(body["profile"]["birth_date"], "1995-05-05");
    assert!(body["profile"]["age"].as_i64().is_some_and(|a| a >= 30));
    assert_eq!(body["profile"]["photos"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["filter"]["max_distance_m"], 2000);
}

#[tokio::test]
async fn me_requires_auth() {
    let app = TestApp::new().await;
    let (status, body) = app.get("/api/me", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "unauthorized");
}

#[tokio::test]
async fn profile_upsert_trims_and_replaces_interests() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;
    let (status, body) = app
        .put(
            "/api/me/profile",
            &t.access_token,
            profile_body("1990-01-01", json!([1, 2, 3])),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["display_name"], "Eva");
    assert_eq!(body["interests"].as_array().map(Vec::len), Some(3));
    assert_eq!(body["interests"][0]["key"], "hiking");

    let (status, body) = app
        .put(
            "/api/me/profile",
            &t.access_token,
            profile_body("1990-01-01", json!([40])),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["interests"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["interests"][0]["key"], "cars");
}

#[tokio::test]
async fn profile_validation_errors() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;
    let too_many: Vec<i32> = (1..=11).collect();
    for interests in [json!(too_many), json!([1, 1]), json!([9999])] {
        let (status, body) = app
            .put(
                "/api/me/profile",
                &t.access_token,
                profile_body("1990-01-01", interests),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "validation");
    }
    let mut bad_name = profile_body("1990-01-01", json!([]));
    bad_name["display_name"] = json!("   ");
    let (status, _) = app.put("/api/me/profile", &t.access_token, bad_name).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Nothing was stored by the failed attempts.
    let (_, me) = app.get("/api/me", Some(&t.access_token)).await;
    assert!(me["profile"].is_null());
}

#[tokio::test]
async fn underage_is_422() {
    let app = TestApp::new().await;
    let t = app.register("kid").await;
    let birth = (chrono::Utc::now().date_naive() - chrono::Days::new(365 * 17)).to_string();
    let (status, body) = app
        .put(
            "/api/me/profile",
            &t.access_token,
            profile_body(&birth, json!([])),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "underage");
}
