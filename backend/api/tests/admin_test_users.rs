mod common;

use axum::http::StatusCode;
use common::photos::png_bytes;
use common::{TestApp, Tokens, count, now_year_birth};
use localdate_api::admin::seed::{Rng, seed_test_users};
use serde_json::{Value, json};

fn new_test_user(username: &str) -> Value {
    json!({
        "username": username,
        "display_name": "Testovací Eva",
        "gender": "female",
        "birth_date": format!("{}-01-01", now_year_birth(25)),
        "interest_ids": [1, 2, 3],
    })
}

async fn create(app: &TestApp, admin: &Tokens, body: Value) -> (StatusCode, Value) {
    app.post_as("/api/admin/test-users", &admin.access_token, body)
        .await
}

#[tokio::test]
async fn admin_creates_lists_and_deletes_test_users() {
    let app = TestApp::new().await;
    let admin = app.admin("mod").await;
    let real = app.register("eva").await;

    let (status, row) = create(&app, &admin, new_test_user("tester_one")).await;
    assert_eq!(status, StatusCode::CREATED, "{row}");
    assert_eq!(row["username"], "tester_one");
    assert_eq!(row["is_test"], true);
    assert_eq!(row["display_name"], "Testovací Eva");
    assert_eq!(row["gender"], "female");
    assert_eq!(row["age"], 25);
    assert!(
        row["photo_url"]
            .as_str()
            .is_some_and(|u| u.starts_with("/media/")),
        "placeholder avatar: {row}"
    );
    let id = row["id"].as_str().expect("id").to_owned();
    // Onboarded: profile, filter, photo; password-less.
    let onboarded = count(
        &app,
        &format!(
            "SELECT count(*) FROM \"user\" u JOIN profile p ON p.user_id = u.id \
             JOIN filter f ON f.user_id = u.id \
             WHERE u.id = '{id}' AND u.password_hash IS NULL AND f.max_distance_m = 10000 \
             AND (SELECT count(*) FROM photo WHERE user_id = u.id) = 1 \
             AND (SELECT count(*) FROM user_interest WHERE user_id = u.id) = 3"
        ),
    )
    .await;
    assert_eq!(onboarded, 1);

    let mut no_photo = new_test_user("tester_two");
    no_photo["placeholder_photo"] = json!(false);
    let (status, second) = create(&app, &admin, no_photo).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(second["photo_url"], Value::Null);
    let second_id = second["id"].as_str().expect("id").to_owned();
    let (status, photo) = app
        .post_multipart(
            &format!("/api/admin/test-users/{second_id}/photos"),
            &admin.access_token,
            "file",
            &png_bytes(16, 16),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{photo}");

    let (status, list) = app
        .get("/api/admin/test-users", Some(&admin.access_token))
        .await;
    assert_eq!(status, StatusCode::OK);
    let names: Vec<&str> = list
        .as_array()
        .expect("array")
        .iter()
        .map(|r| r["username"].as_str().expect("username"))
        .collect();
    assert_eq!(names, ["tester_two", "tester_one"], "newest first, no eva");

    // The user search marks them too, and finds real users.
    let (status, found) = app
        .get("/api/admin/users?q=TESTER_O", Some(&admin.access_token))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(found.as_array().map(Vec::len), Some(1));
    assert_eq!(found[0]["is_test"], true);
    let (_, found) = app
        .get("/api/admin/users?q=eva", Some(&admin.access_token))
        .await;
    assert!(
        found
            .as_array()
            .expect("array")
            .iter()
            .any(|r| r["username"] == "eva" && r["is_test"] == false),
        "{found}"
    );
    let (_, all) = app.get("/api/admin/users", Some(&admin.access_token)).await;
    assert_eq!(all.as_array().map(Vec::len), Some(4));
    // Free-form names match by key: case-insensitive beyond ASCII, spaces and all.
    let (status, _) = create(&app, &admin, new_test_user("Petr Novák")).await;
    assert_eq!(status, StatusCode::CREATED);
    let (_, found) = app
        .get(
            "/api/admin/users?q=R%20NOV%C3%81K",
            Some(&admin.access_token),
        )
        .await;
    assert_eq!(found[0]["username"], "Petr Novák", "{found}");
    assert_eq!(found.as_array().map(Vec::len), Some(1));

    let files = app.photo_files();
    let (status, _) = app
        .delete(&format!("/api/admin/test-users/{id}"), &admin.access_token)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(app.photo_files(), files - 1, "its photo file is gone");
    let (status, _) = app
        .delete(&format!("/api/admin/test-users/{id}"), &admin.access_token)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Real accounts are out of reach of these endpoints.
    for path in [
        format!("/api/admin/test-users/{}", real.user_id),
        format!("/api/admin/test-users/{}", admin.user_id),
    ] {
        let (status, _) = app.delete(&path, &admin.access_token).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }
    let (status, _) = app
        .post_multipart(
            &format!("/api/admin/test-users/{}/photos", real.user_id),
            &admin.access_token,
            "file",
            &png_bytes(8, 8),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(count(&app, "SELECT count(*) FROM \"user\"").await, 4);
}

#[tokio::test]
async fn test_user_creation_validates_like_registration_and_profile() {
    let app = TestApp::new().await;
    let admin = app.admin("mod").await;
    app.register("eva").await;

    // Unique case-insensitively, as for registration.
    let (status, body) = create(&app, &admin, new_test_user(" EVA ")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "username_taken");
    let (status, _) = create(&app, &admin, new_test_user("   ")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let mut young = new_test_user("young_one");
    young["birth_date"] = json!(format!("{}-12-31", now_year_birth(17)));
    let (status, body) = create(&app, &admin, young).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "underage");
    let mut bad_interest = new_test_user("bad_interest");
    bad_interest["interest_ids"] = json!([999]);
    let (status, _) = create(&app, &admin, bad_interest).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    // A failed profile leaves no half-made account behind.
    assert_eq!(
        count(&app, "SELECT count(*) FROM \"user\" WHERE is_test").await,
        0
    );
    assert_eq!(app.photo_files(), 0, "placeholder files cleaned up too");

    let bob = app.register("bob").await;
    for (method_get, path) in [
        (true, "/api/admin/test-users"),
        (true, "/api/admin/users"),
        (true, "/api/admin/audit"),
        (true, "/api/admin/settings"),
        (false, "/api/admin/test-users"),
    ] {
        let (status, _) = if method_get {
            app.get(path, Some(&bob.access_token)).await
        } else {
            create(&app, &bob, new_test_user("sneaky")).await
        };
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
    }
    let (status, _) = app
        .get(
            &format!("/api/admin/users?q={}", "x".repeat(65)),
            Some(&admin.access_token),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_test_user_promoted_to_admin_is_out_of_reach() {
    let app = TestApp::new().await;
    let admin = app.admin("mod").await;
    let (_, row) = create(&app, &admin, new_test_user("tester_admin")).await;
    let id = row["id"].as_str().expect("id").to_owned();
    localdate_api::admin::set_admin(&app.db, "tester_admin", true)
        .await
        .expect("grant");
    let (status, _) = app
        .post_multipart(
            &format!("/api/admin/test-users/{id}/photos"),
            &admin.access_token,
            "file",
            &png_bytes(8, 8),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = app
        .delete(&format!("/api/admin/test-users/{id}"), &admin.access_token)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        count(
            &app,
            &format!("SELECT count(*) FROM \"user\" WHERE id = '{id}'")
        )
        .await,
        1
    );
}

#[tokio::test]
async fn seed_creates_onboarded_test_users_with_photos() {
    let app = TestApp::new().await;
    let mut rng = Rng::new(2026);
    let created = seed_test_users(&app.db, &app.state.photos, 5, &mut rng)
        .await
        .expect("seeded");
    assert_eq!(created.len(), 5);
    let onboarded = count(
        &app,
        "SELECT count(*) FROM \"user\" u JOIN profile p ON p.user_id = u.id \
         JOIN filter f ON f.user_id = u.id \
         WHERE u.is_test AND u.password_hash IS NULL \
         AND (SELECT count(*) FROM photo WHERE user_id = u.id) = 1 \
         AND (SELECT count(*) FROM user_interest WHERE user_id = u.id) BETWEEN 3 AND 6",
    )
    .await;
    assert_eq!(onboarded, 5);
    assert_eq!(app.photo_files(), 5);

    let again = seed_test_users(&app.db, &app.state.photos, 2, &mut rng)
        .await
        .expect("seeded again");
    assert!(again.iter().all(|n| !created.contains(n)));
    assert_eq!(
        count(&app, "SELECT count(*) FROM \"user\" WHERE is_test").await,
        7
    );

    for bad in [0, 201] {
        assert!(
            seed_test_users(&app.db, &app.state.photos, bad, &mut rng)
                .await
                .is_err()
        );
    }
}
