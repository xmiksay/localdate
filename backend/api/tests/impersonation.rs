mod common;

use std::time::Duration;

use axum::http::{Method, StatusCode};
use common::ws::{next, ready_socket};
use common::{TestApp, Tokens, count, now_year_birth};
use localdate_api::auth::extractor::authorize;
use localdate_api::auth::jwt;
use serde_json::{Value, json};

const LAT: f64 = 50.0870;
const LON: f64 = 14.4210;

async fn enabled() -> TestApp {
    TestApp::with_config(|c| c.admin_impersonation = true).await
}

async fn test_user(app: &TestApp, admin: &Tokens, username: &str) -> String {
    let (status, row) = app
        .post_as(
            "/api/admin/test-users",
            &admin.access_token,
            json!({
                "username": username, "display_name": "Tester", "gender": "male",
                "birth_date": format!("{}-01-01", now_year_birth(30)), "interest_ids": [],
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{row}");
    row["id"].as_str().expect("id").to_owned()
}

async fn impersonate(app: &TestApp, admin: &Tokens, id: &str) -> (StatusCode, Value) {
    app.post_as(
        &format!("/api/admin/users/{id}/impersonate"),
        &admin.access_token,
        json!({}),
    )
    .await
}

/// The impersonation token for `id`, which must be granted.
async fn act_as(app: &TestApp, admin: &Tokens, id: &str) -> String {
    let (status, body) = impersonate(app, admin, id).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.get("refresh_token").is_none(), "no refresh token");
    body["access_token"].as_str().expect("token").to_owned()
}

#[tokio::test]
async fn off_by_default_hides_the_endpoint() {
    let app = TestApp::new().await;
    let admin = app.admin("mod").await;
    let id = test_user(&app, &admin, "tester").await;
    let (status, body) = app
        .get("/api/admin/settings", Some(&admin.access_token))
        .await;
    assert_eq!(
        (status, body),
        (StatusCode::OK, json!({ "impersonation": false }))
    );
    let (status, _) = impersonate(&app, &admin, &id).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(count(&app, "SELECT count(*) FROM admin_audit").await, 0);

    // A token issued while it was on dies when it is switched off.
    let admin_id = admin.user_id;
    let (token, _) = jwt::issue_impersonation(
        &app.state.config.jwt_secret,
        id.parse().expect("uuid"),
        admin_id,
    )
    .expect("token");
    let (status, _) = app.get("/api/me", Some(&token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let access = jwt::verify(&app.state.config.jwt_secret, &token).expect("valid jwt");
    assert!(authorize(&app.db, true, access).await.is_ok(), "on → fine");
}

#[tokio::test]
async fn admin_acts_as_the_target_and_every_change_is_audited() {
    let app = enabled().await;
    let admin = app.admin("mod").await;
    let eva = app.visible_user("eva", LAT, LON).await;
    let id = test_user(&app, &admin, "tester").await;
    let (_, settings) = app
        .get("/api/admin/settings", Some(&admin.access_token))
        .await;
    assert_eq!(settings["impersonation"], true);

    let (status, body) = impersonate(&app, &admin, &id).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["username"], "tester");
    assert!(body["expires_at"].as_str().is_some());
    let token = body["access_token"].as_str().expect("token").to_owned();

    let (status, me) = app.get("/api/me", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["user"]["username"], "tester");
    assert_eq!(me["is_admin"], false);

    // Position, window, nearby, waves: the test user is a full participant.
    let (status, w) = app
        .post_as(
            "/api/me/window",
            &token,
            json!({ "minutes": 60, "lat": LAT + 0.001, "lon": LON }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{w}");
    let (status, _) = app
        .post_as(
            "/api/me/location",
            &token,
            json!({ "lat": LAT + 0.002, "lon": LON }),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, nearby) = app.get("/api/nearby", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(nearby[0]["user_id"], json!(eva.user_id));
    let (_, seen_by_eva) = app.get("/api/nearby", Some(&eva.access_token)).await;
    assert_eq!(
        seen_by_eva[0]["user_id"],
        json!(id),
        "visible to real users"
    );
    let (status, _) = app
        .post_as("/api/waves", &token, json!({ "to_user_id": eva.user_id }))
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, audit) = app.get("/api/admin/audit", Some(&admin.access_token)).await;
    assert_eq!(status, StatusCode::OK);
    let entries: Vec<(String, Value)> = audit
        .as_array()
        .expect("array")
        .iter()
        .map(|e| {
            (
                e["action"].as_str().expect("action").to_owned(),
                e["meta"].clone(),
            )
        })
        .collect();
    assert_eq!(
        entries,
        [
            (
                "impersonated_request".into(),
                json!({ "method": "POST", "route": "/api/waves" })
            ),
            (
                "impersonated_request".into(),
                json!({ "method": "GET", "route": "/api/nearby" })
            ),
            (
                "impersonated_request".into(),
                json!({ "method": "POST", "route": "/api/me/location" })
            ),
            (
                "impersonated_request".into(),
                json!({ "method": "POST", "route": "/api/me/window" })
            ),
            (
                "impersonated_request".into(),
                json!({ "method": "GET", "route": "/api/me" })
            ),
            ("impersonate".into(), json!({})),
        ],
        "every request, GETs included; newest first"
    );
    assert_eq!(audit[0]["admin"]["username"], "mod");
    assert_eq!(audit[0]["target"]["username"], "tester");
}

#[tokio::test]
async fn credentials_identities_deletion_push_and_admin_are_off_limits() {
    let app = enabled().await;
    let admin = app.admin("mod").await;
    let id = test_user(&app, &admin, "tester").await;
    let token = act_as(&app, &admin, &id).await;

    let cases = [
        (
            Method::PUT,
            "/api/me/password",
            json!({ "new_password": "abcdefghijk" }),
        ),
        (Method::DELETE, "/api/me", Value::Null),
        (
            Method::POST,
            "/api/me/identities/email",
            json!({ "email": "a@b.cz" }),
        ),
        (
            Method::POST,
            "/api/me/identities/email/confirm",
            json!({ "token": "x" }),
        ),
        (
            Method::DELETE,
            &format!("/api/me/identities/{id}"),
            Value::Null,
        ),
        (Method::POST, "/api/auth/oauth/google/link", json!({})),
        (
            Method::POST,
            "/api/me/push/subscriptions",
            json!({ "endpoint": "https://push.example/x", "keys": { "p256dh": "a", "auth": "b" } }),
        ),
        (Method::GET, "/api/admin/reports", Value::Null),
        (
            Method::POST,
            &format!("/api/admin/users/{id}/impersonate"),
            json!({}),
        ),
    ];
    for (method, path, body) in cases {
        let body = (!body.is_null()).then_some(body);
        let (status, res) = app.request(method.clone(), path, Some(&token), body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}: {res}");
        assert_eq!(res["error"]["code"], "impersonation_forbidden", "{path}");
    }
    assert_eq!(
        count(
            &app,
            &format!("SELECT count(*) FROM \"user\" WHERE id = '{id}'")
        )
        .await,
        1,
        "the account survived"
    );
    // Refused attempts are audited too.
    assert_eq!(
        count(
            &app,
            "SELECT count(*) FROM admin_audit WHERE action = 'impersonated_request'"
        )
        .await,
        9
    );
}

#[tokio::test]
async fn an_impersonation_outlives_neither_its_ttl_nor_the_flag() {
    let app = enabled().await;
    let admin = app.admin("mod").await;
    let target = app.register("tester").await;
    let at = |issued_at| jwt::Access {
        user: target.user_id,
        issued_at,
        actor: Some(admin.user_id),
    };
    let now = chrono::Utc::now().timestamp();
    // This is the check the WebSocket re-check runs on the token it was opened with.
    assert!(authorize(&app.db, true, at(now - 60)).await.is_ok());
    for stale in [
        now - jwt::IMPERSONATION_TTL_SECS,
        now - 2 * jwt::IMPERSONATION_TTL_SECS,
    ] {
        let refused = authorize(&app.db, true, at(stale)).await;
        assert!(
            matches!(refused, Err(localdate_api::error::AppError::Unauthorized)),
            "issued {}s ago",
            now - stale
        );
    }
    assert!(
        authorize(&app.db, false, at(now - 60)).await.is_err(),
        "flag off"
    );
    // A plain token is unaffected by either.
    let plain = jwt::Access {
        actor: None,
        ..at(now - 60)
    };
    assert!(authorize(&app.db, false, plain).await.is_ok());
}

#[tokio::test]
async fn admins_banned_unknown_and_non_admin_callers_are_refused() {
    let app = enabled().await;
    let admin = app.admin("mod").await;
    let other_admin = app.admin("mod2").await;
    let banned = app.register("banned").await;
    let eva = app.register("eva").await;
    app.post_as(
        &format!("/api/admin/users/{}/ban", banned.user_id),
        &admin.access_token,
        json!({}),
    )
    .await;
    for target in [admin.user_id, other_admin.user_id, banned.user_id] {
        let (status, body) = impersonate(&app, &admin, &target.to_string()).await;
        assert_eq!(status, StatusCode::CONFLICT, "{target}");
        assert_eq!(body["error"]["code"], "cannot_impersonate");
    }
    let (status, _) = impersonate(&app, &admin, &uuid::Uuid::new_v4().to_string()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = impersonate(&app, &eva, &admin.user_id.to_string()).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // Real (non-test) users can be impersonated too.
    let token = act_as(&app, &admin, &eva.user_id.to_string()).await;
    let (_, me) = app.get("/api/me", Some(&token)).await;
    assert_eq!(me["user"]["username"], "eva");
}

#[tokio::test]
async fn the_session_dies_with_the_actors_admin_role_ban_or_password_change() {
    let app = enabled().await;
    let id = {
        let admin = app.admin("setup").await;
        test_user(&app, &admin, "tester").await
    };

    // Demotion.
    let admin = app.admin("mod").await;
    let token = act_as(&app, &admin, &id).await;
    localdate_api::admin::set_admin(&app.db, "mod", false)
        .await
        .expect("revoke");
    let (status, _) = app.get("/api/me", Some(&token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Ban (set directly: admins cannot be banned through the API).
    let admin = app.admin("mod_b").await;
    let token = act_as(&app, &admin, &id).await;
    app.sql(&format!(
        "UPDATE \"user\" SET banned_at = now() WHERE id = '{}'",
        admin.user_id
    ))
    .await;
    let (status, _) = app.get("/api/me", Some(&token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // The admin's password changes after the token was issued.
    let admin = app.admin("mod_c").await;
    let token = act_as(&app, &admin, &id).await;
    app.sql(&format!(
        "UPDATE \"user\" SET credentials_changed_at = now() + interval '5 seconds' WHERE id = '{}'",
        admin.user_id
    ))
    .await;
    let (status, _) = app.get("/api/me", Some(&token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // The target becomes an admin.
    let admin = app.admin("mod_d").await;
    let token = act_as(&app, &admin, &id).await;
    app.sql(&format!(
        "UPDATE \"user\" SET is_admin = true WHERE id = '{id}'"
    ))
    .await;
    let (status, _) = app.get("/api/me", Some(&token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn websocket_accepts_the_token_and_closes_when_the_actor_is_demoted() {
    let app = TestApp::with_config(|c| {
        c.admin_impersonation = true;
        c.ws_account_recheck = Duration::from_millis(200);
    })
    .await;
    let url = common::ws::serve(&app.router).await;
    let admin = app.admin("mod").await;
    let eva = app.visible_user("eva", LAT, LON).await;
    let tester = app.register("tester").await;
    app.onboard(&tester, "male", "1990-01-01").await;
    app.open_window(&tester, LAT + 0.001, LON).await;
    let token = act_as(&app, &admin, &tester.user_id.to_string()).await;
    let as_tester = Tokens {
        access_token: token,
        refresh_token: String::new(),
        user_id: tester.user_id,
    };
    let mut ws = ready_socket(&url, &as_tester).await;

    app.post_as(
        "/api/waves",
        &eva.access_token,
        json!({ "to_user_id": tester.user_id }),
    )
    .await;
    assert_eq!(
        next(&mut ws).await,
        Ok(json!({ "type": "wave", "from_user_id": eva.user_id })),
        "realtime as the target"
    );

    localdate_api::admin::set_admin(&app.db, "mod", false)
        .await
        .expect("revoke");
    assert_eq!(next(&mut ws).await, Err(4401));

    // The socket's start and end are audited (the end right after the close frame went out).
    let rows = |method: &str| {
        format!(
            "SELECT count(*) FROM admin_audit WHERE action = 'impersonated_request' \
             AND meta = '{{\"method\": \"{method}\", \"route\": \"/api/ws\"}}'::jsonb"
        )
    };
    assert_eq!(count(&app, &rows("WS OPEN")).await, 1);
    let mut closed = 0;
    for _ in 0..50 {
        closed = count(&app, &rows("WS CLOSE")).await;
        if closed == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    assert_eq!(closed, 1);
}
