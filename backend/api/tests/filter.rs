mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::{Value, json};

fn filter(edit: impl FnOnce(&mut Value)) -> Value {
    let mut f = json!({
        "max_distance_m": 5000, "genders": ["female"], "age_min": 25, "age_max": 40,
        "reasons": ["date"], "default_window_minutes": 120,
    });
    edit(&mut f);
    f
}

#[tokio::test]
async fn defaults_when_never_saved() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;
    let (status, body) = app.get("/api/me/filter", Some(&t.access_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({
            "max_distance_m": 2000, "genders": [], "age_min": 18, "age_max": 99,
            "reasons": ["date", "meet"], "default_window_minutes": 60,
        })
    );
    let (_, me) = app.get("/api/me", Some(&t.access_token)).await;
    assert!(me["filter"].is_null());
}

#[tokio::test]
async fn put_upserts_and_dedups() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;
    let body = filter(|f| {
        f["genders"] = json!(["male", "male", "other"]);
        f["reasons"] = json!(["meet", "meet"]);
    });
    let (status, saved) = app.put("/api/me/filter", &t.access_token, body).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["genders"], json!(["male", "other"]));
    assert_eq!(saved["reasons"], json!(["meet"]));

    let (_, second) = app
        .put(
            "/api/me/filter",
            &t.access_token,
            filter(|f| f["age_min"] = json!(30)),
        )
        .await;
    assert_eq!(second["age_min"], 30);
    let (_, got) = app.get("/api/me/filter", Some(&t.access_token)).await;
    assert_eq!(got, second);
    let (_, me) = app.get("/api/me", Some(&t.access_token)).await;
    assert_eq!(me["filter"], second);
}

#[tokio::test]
async fn put_rejects_invalid() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;
    let cases = [
        filter(|f| f["max_distance_m"] = json!(100)),
        filter(|f| f["max_distance_m"] = json!(20000)),
        filter(|f| f["age_min"] = json!(17)),
        filter(|f| f["age_max"] = json!(20)),
        filter(|f| f["reasons"] = json!([])),
        filter(|f| f["default_window_minutes"] = json!(45)),
        filter(|f| f["genders"] = json!(["robot"])),
    ];
    for body in cases {
        let (status, err) = app
            .put("/api/me/filter", &t.access_token, body.clone())
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(err["error"]["code"], "validation");
    }
}
