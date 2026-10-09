mod common;

use axum::http::StatusCode;
use common::TestApp;
use localdate_api::safety::{blocked_with, is_blocked_between};
use serde_json::json;
use std::collections::HashSet;

#[tokio::test]
async fn block_is_idempotent_listed_and_removable() {
    let app = TestApp::new().await;
    let eva = app.register("eva").await;
    let bob = app.register("bob").await;
    let carl = app.register("carl").await;
    app.onboard(&bob, "male", "1990-01-01").await;

    for _ in 0..2 {
        let (status, _) = app
            .post_as(
                "/api/blocks",
                &eva.access_token,
                json!({ "user_id": bob.user_id }),
            )
            .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }
    app.post_as(
        "/api/blocks",
        &eva.access_token,
        json!({ "user_id": carl.user_id }),
    )
    .await;

    let (status, list) = app.get("/api/blocks", Some(&eva.access_token)).await;
    assert_eq!(status, StatusCode::OK);
    let list = list.as_array().expect("array");
    assert_eq!(list.len(), 2);
    // Newest first; carl has no profile.
    assert_eq!(list[0]["user_id"], json!(carl.user_id));
    assert_eq!(list[0]["display_name"], "");
    assert_eq!(list[1]["display_name"], "Tester");
    assert!(list[1]["created_at"].is_string());

    assert!(
        is_blocked_between(&app.db, eva.user_id, bob.user_id)
            .await
            .expect("query")
    );
    assert!(
        is_blocked_between(&app.db, bob.user_id, eva.user_id)
            .await
            .expect("query")
    );
    assert!(
        !is_blocked_between(&app.db, bob.user_id, carl.user_id)
            .await
            .expect("query")
    );
    let set = blocked_with(&app.db, bob.user_id).await.expect("query");
    assert_eq!(set, HashSet::from([eva.user_id]));

    let (status, _) = app
        .delete(&format!("/api/blocks/{}", bob.user_id), &eva.access_token)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(
        !is_blocked_between(&app.db, eva.user_id, bob.user_id)
            .await
            .expect("query")
    );
    // Unblocking something not blocked is still 204.
    let (status, _) = app
        .delete(&format!("/api/blocks/{}", bob.user_id), &eva.access_token)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn block_validates_target() {
    let app = TestApp::new().await;
    let eva = app.register("eva").await;
    let (status, body) = app
        .post_as(
            "/api/blocks",
            &eva.access_token,
            json!({ "user_id": eva.user_id }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "validation");
    let (status, _) = app
        .post_as(
            "/api/blocks",
            &eva.access_token,
            json!({ "user_id": uuid::Uuid::new_v4() }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = app.get("/api/blocks", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn report_stores_and_blocks() {
    let app = TestApp::new().await;
    let eva = app.register("eva").await;
    let bob = app.register("bob").await;
    let (status, _) = app
        .post_as(
            "/api/reports",
            &eva.access_token,
            json!({ "user_id": bob.user_id, "reason": "spam", "note": "  buy now " }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(
        is_blocked_between(&app.db, eva.user_id, bob.user_id)
            .await
            .expect("query")
    );

    use entity::report;
    use sea_orm::EntityTrait;
    let rows = report::Entity::find().all(&app.db).await.expect("reports");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].note.as_deref(), Some("buy now"));

    // Reporting someone already blocked still works.
    let (status, _) = app
        .post_as(
            "/api/reports",
            &eva.access_token,
            json!({ "user_id": bob.user_id, "reason": "fake" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn report_validation() {
    let app = TestApp::new().await;
    let eva = app.register("eva").await;
    let bob = app.register("bob").await;
    for body in [
        json!({ "user_id": bob.user_id, "reason": "rude" }),
        json!({ "user_id": bob.user_id, "reason": "other", "note": "x".repeat(1001) }),
        json!({ "user_id": eva.user_id, "reason": "other" }),
    ] {
        let (status, err) = app
            .post_as("/api/reports", &eva.access_token, body.clone())
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(err["error"]["code"], "validation");
    }
    assert!(
        !is_blocked_between(&app.db, eva.user_id, bob.user_id)
            .await
            .expect("query")
    );
    let (status, _) = app
        .post_as(
            "/api/reports",
            &eva.access_token,
            json!({ "user_id": uuid::Uuid::new_v4(), "reason": "spam" }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_me_removes_rows_and_photo_files() {
    let app = TestApp::new().await;
    let eva = app.register("eva").await;
    let bob = app.register("bob").await;
    app.onboard(&eva, "female", "1990-01-01").await;
    app.upload_photo(&eva, 8, 8).await;
    app.onboard(&bob, "male", "1990-01-01").await;
    app.post_as(
        "/api/blocks",
        &bob.access_token,
        json!({ "user_id": eva.user_id }),
    )
    .await;
    assert_eq!(app.photo_files(), 3);

    let (status, _) = app.delete("/api/me", &eva.access_token).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Only bob's photo remains; eva's rows are gone via cascade.
    assert_eq!(app.photo_files(), 1);
    use entity::{block, photo, profile, user};
    use sea_orm::{EntityTrait, PaginatorTrait};
    assert_eq!(user::Entity::find().count(&app.db).await.expect("count"), 1);
    assert_eq!(
        profile::Entity::find().count(&app.db).await.expect("count"),
        1
    );
    assert_eq!(
        photo::Entity::find().count(&app.db).await.expect("count"),
        1
    );
    assert_eq!(
        block::Entity::find().count(&app.db).await.expect("count"),
        0
    );

    let (status, _) = app
        .post(
            "/api/auth/login",
            json!({ "username": "eva", "password": common::PASSWORD }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
