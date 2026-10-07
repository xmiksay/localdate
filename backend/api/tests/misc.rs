mod common;

use axum::http::StatusCode;
use common::TestApp;
use entity::{Gender, Reason, filter, user};
use sea_orm::{ActiveModelTrait, EntityTrait, Set};

#[tokio::test]
async fn health_is_ok() {
    let app = TestApp::new().await;
    let (status, _) = app.get("/api/health", None).await;
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
