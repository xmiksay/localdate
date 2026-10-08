mod common;

use axum::http::StatusCode;
use common::{TestApp, Tokens, count};
use localdate_api::admin::set_admin;
use serde_json::{Value, json};

async fn report(app: &TestApp, by: &Tokens, who: &Tokens, reason: &str, note: Option<&str>) {
    let (status, body) = app
        .post_as(
            "/api/reports",
            &by.access_token,
            json!({ "user_id": who.user_id, "reason": reason, "note": note }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "report failed: {body}");
}

async fn queue(app: &TestApp, admin: &Tokens, query: &str) -> Vec<Value> {
    let (status, body) = app
        .get(
            &format!("/api/admin/reports{query}"),
            Some(&admin.access_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "queue failed: {body}");
    body.as_array().expect("array").clone()
}

fn reasons(list: &[Value]) -> Vec<&str> {
    list.iter()
        .map(|r| r["reason"].as_str().expect("reason"))
        .collect()
}

#[tokio::test]
async fn admin_role_is_granted_and_revoked_and_guards_the_api() {
    let app = TestApp::new().await;
    let eva = app.register("eva").await;

    let (status, body) = app.get("/api/admin/reports", Some(&eva.access_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "forbidden");
    let (status, _) = app.get("/api/admin/reports", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (_, me) = app.get("/api/me", Some(&eva.access_token)).await;
    assert_eq!(me["is_admin"], false);

    // The CLI normalizes like login does.
    set_admin(&app.db, "  EVA ", true).await.expect("grant");
    let (_, me) = app.get("/api/me", Some(&eva.access_token)).await;
    assert_eq!(me["is_admin"], true);
    assert_eq!(queue(&app, &eva, "").await, Vec::<Value>::new());

    set_admin(&app.db, "eva", false).await.expect("revoke");
    let (status, _) = app.get("/api/admin/reports", Some(&eva.access_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let err = set_admin(&app.db, "nobody", true)
        .await
        .expect_err("unknown user");
    assert!(err.to_string().contains("no user named 'nobody'"), "{err}");
}

#[tokio::test]
async fn queue_lists_open_first_with_subject_details_and_filters() {
    let app = TestApp::new().await;
    let admin = app.admin("mod").await;
    let eva = app.register("eva").await;
    let carl = app.register("carl").await;
    let bob = app.register("bob").await;
    app.onboard(&bob, "male", "1990-01-01").await;

    report(&app, &eva, &bob, "spam", Some(" ads ")).await;
    report(&app, &carl, &bob, "fake", None).await;
    report(&app, &eva, &carl, "harassment", None).await;
    // Make insertion order unambiguous: spam oldest, harassment newest.
    app.sql("UPDATE report SET created_at = now() - interval '3 hours' WHERE reason = 'spam'")
        .await;
    app.sql("UPDATE report SET created_at = now() - interval '2 hours' WHERE reason = 'fake'")
        .await;

    let all = queue(&app, &admin, "").await;
    assert_eq!(reasons(&all), ["spam", "fake", "harassment"]);
    let spam = &all[0];
    assert_eq!(spam["note"], "ads");
    assert_eq!(
        spam["reporter"],
        json!({ "id": eva.user_id, "username": "eva" })
    );
    assert_eq!(spam["resolution"], Value::Null);
    assert_eq!(spam["resolved_by"], Value::Null);
    let subject = &spam["subject"];
    assert_eq!(subject["id"], json!(bob.user_id));
    assert_eq!(subject["username"], "bob");
    assert_eq!(subject["display_name"], "Tester");
    assert!(
        subject["photo_url"]
            .as_str()
            .expect("photo")
            .starts_with("/media/")
    );
    assert_eq!(subject["banned_at"], Value::Null);
    assert_eq!(subject["is_admin"], false);
    assert_eq!(subject["open_reports"], 2);
    // carl has no profile or photo.
    assert_eq!(all[2]["subject"]["display_name"], Value::Null);
    assert_eq!(all[2]["subject"]["photo_url"], Value::Null);
    // Admins see no location or birth date either.
    let text = serde_json::to_string(&all).expect("json");
    for leaked in ["birth", "lat", "lon", "1990"] {
        assert!(!text.contains(leaked), "{leaked} leaked: {text}");
    }

    // Dismissing the newest open report moves it to the resolved tail.
    let fake_id = all[1]["id"].as_str().expect("id").to_owned();
    let (status, _) = app
        .post_as(
            &format!("/api/admin/reports/{fake_id}/dismiss"),
            &admin.access_token,
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let all = queue(&app, &admin, "").await;
    assert_eq!(reasons(&all), ["spam", "harassment", "fake"]);
    assert_eq!(all[0]["subject"]["open_reports"], 1);
    let fake = &all[2];
    assert_eq!(fake["resolution"], "dismissed");
    assert!(fake["resolved_at"].is_string());
    assert_eq!(
        fake["resolved_by"],
        json!({ "id": admin.user_id, "username": "mod" })
    );

    assert_eq!(
        reasons(&queue(&app, &admin, "?status=open").await),
        ["spam", "harassment"]
    );
    assert_eq!(
        reasons(&queue(&app, &admin, "?status=resolved").await),
        ["fake"]
    );
    let (status, body) = app
        .get("/api/admin/reports?status=all", Some(&admin.access_token))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "validation");
}

#[tokio::test]
async fn dismiss_is_single_shot_and_404s_unknown_reports() {
    let app = TestApp::new().await;
    let admin = app.admin("mod").await;
    let eva = app.register("eva").await;
    let bob = app.register("bob").await;
    report(&app, &eva, &bob, "spam", None).await;
    let id = queue(&app, &admin, "").await[0]["id"]
        .as_str()
        .expect("id")
        .to_owned();
    let path = format!("/api/admin/reports/{id}/dismiss");

    let (status, _) = app.post_as(&path, &eva.access_token, json!({})).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = app.post_as(&path, &admin.access_token, json!({})).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, body) = app.post_as(&path, &admin.access_token, json!({})).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "already_resolved");

    for bad in [uuid::Uuid::new_v4().to_string(), "nope".into()] {
        let (status, _) = app
            .post_as(
                &format!("/api/admin/reports/{bad}/dismiss"),
                &admin.access_token,
                json!({}),
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{bad}");
    }
}

#[tokio::test]
async fn deleted_accounts_keep_the_queue_consistent() {
    let app = TestApp::new().await;
    let admin = app.admin("mod").await;
    let eva = app.register("eva").await;
    let bob = app.register("bob").await;
    let carl = app.register("carl").await;
    report(&app, &eva, &bob, "spam", None).await;
    report(&app, &bob, &carl, "fake", None).await;
    let dismissed = queue(&app, &admin, "").await[0]["id"]
        .as_str()
        .expect("id")
        .to_owned();
    app.post_as(
        &format!("/api/admin/reports/{dismissed}/dismiss"),
        &admin.access_token,
        json!({}),
    )
    .await;

    // A reporter leaving keeps their report as evidence, anonymized.
    let (status, _) = app.delete("/api/me", &eva.access_token).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let all = queue(&app, &admin, "").await;
    assert_eq!(reasons(&all), ["fake", "spam"]);
    assert_eq!(all[1]["reporter"], Value::Null);

    // A deleted subject takes the reports about them along.
    let (status, _) = app.delete("/api/me", &carl.access_token).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(reasons(&queue(&app, &admin, "").await), ["spam"]);

    // The resolving admin leaving only blanks `resolved_by`.
    let (status, _) = app.delete("/api/me", &admin.access_token).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        count(
            &app,
            "SELECT count(*) FROM report WHERE resolved_by IS NULL AND resolution = 'dismissed'"
        )
        .await,
        1
    );
}
