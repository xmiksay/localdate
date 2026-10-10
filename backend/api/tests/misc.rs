mod common;

use axum::http::StatusCode;
use common::TestApp;
use entity::{Gender, Reason, filter, user};
use sea_orm::{ActiveModelTrait, EntityTrait, Set};

#[tokio::test]
async fn health_reports_version_commit_and_db() {
    let app = TestApp::new().await;
    let (status, body) = app.get("/api/health", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["db"], "ok");
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    let commit = body["commit"].as_str().expect("commit string");
    assert!(commit == "dev" || commit.len() == 7, "{commit}");
    assert!(body["built_at"].is_null() || body["built_at"].is_string());
    assert_eq!(
        body.as_object().expect("object").len(),
        5,
        "no extra fields: {body}"
    );

    let (status, _) = app.get("/api/live", None).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn health_is_503_without_the_db_but_live_stays_200() {
    let app = TestApp::new().await;
    // Closing a clone closes the shared pool, as an unreachable database would leave it unusable.
    let _ = app.db.clone().close().await;

    let (status, body) = app.get("/api/health", None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["status"], "degraded");
    assert_eq!(body["db"], "unavailable");
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(
        body.as_object().expect("object").len(),
        5,
        "no error details: {body}"
    );

    let (status, _) = app.get("/api/live", None).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn interests_are_seeded_in_contract_order() {
    let app = TestApp::new().await;
    let (status, body) = app.get("/api/interests", None).await;
    assert_eq!(status, StatusCode::OK);
    let list = body.as_array().expect("array");
    assert_eq!(list.len(), 40);
    assert_eq!(list[0]["id"], 1);
    assert_eq!(list[0]["key"], "hiking");
    assert_eq!(list[39]["id"], 40);
    assert_eq!(list[39]["key"], "cars");
}

#[tokio::test]
async fn filter_enum_arrays_roundtrip() {
    let app = TestApp::new().await;
    let t = app.register("alice").await;
    filter::ActiveModel {
        user_id: Set(t.user_id),
        max_distance_m: Set(2000),
        genders: Set(vec![Gender::Female, Gender::Other]),
        age_min: Set(18),
        age_max: Set(99),
        reasons: Set(vec![Reason::Date, Reason::Meet]),
        default_window_minutes: Set(60),
    }
    .insert(&app.db)
    .await
    .expect("insert filter");
    let row = filter::Entity::find_by_id(t.user_id)
        .one(&app.db)
        .await
        .expect("query")
        .expect("row");
    assert_eq!(row.genders, vec![Gender::Female, Gender::Other]);
    assert_eq!(row.reasons, vec![Reason::Date, Reason::Meet]);
}

#[tokio::test]
async fn deleting_a_user_cascades() {
    let app = TestApp::new().await;
    let t = app.register("alice").await;
    user::Entity::delete_by_id(t.user_id)
        .exec(&app.db)
        .await
        .expect("delete");
    let left = entity::refresh_token::Entity::find()
        .all(&app.db)
        .await
        .expect("query");
    assert!(left.is_empty());
}
