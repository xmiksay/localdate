mod common;

use axum::http::StatusCode;
use common::{PASSWORD, TestApp, Tokens, count};
use serde_json::{Value, json};

const LAT: f64 = 50.0870;
const LON: f64 = 14.4210;

async fn act(
    app: &TestApp,
    admin: &Tokens,
    action: &str,
    who: impl std::fmt::Display,
) -> (StatusCode, Value) {
    app.post_as(
        &format!("/api/admin/users/{who}/{action}"),
        &admin.access_token,
        json!({}),
    )
    .await
}

async fn login(app: &TestApp, username: &str, password: &str) -> (StatusCode, Value) {
    app.post(
        "/api/auth/login",
        json!({ "username": username, "password": password }),
    )
    .await
}

fn nearby_ids(list: &Value) -> Vec<String> {
    list.as_array()
        .expect("array")
        .iter()
        .map(|p| p["user_id"].as_str().expect("id").to_owned())
        .collect()
}

#[tokio::test]
async fn ban_cuts_every_session_hides_the_user_and_unban_restores_login() {
    let app = TestApp::new().await;
    let admin = app.admin("mod").await;
    let eva = app.visible_user("eva", LAT, LON).await;
    let bob = app.visible_user("bob", LAT + 0.0027, LON).await;
    let carl = app.register("carl").await;
    let match_id = app.match_up(&eva, &bob).await;
    // A second session of bob, to see that every refresh token goes.
    let (_, second) = login(&app, "bob", PASSWORD).await;
    app.post_as(
        "/api/reports",
        &carl.access_token,
        json!({ "user_id": bob.user_id, "reason": "harassment" }),
    )
    .await;

    let (status, _) = act(&app, &admin, "ban", bob.user_id).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // The still-valid access token is refused immediately.
    let (status, body) = app.get("/api/me", Some(&bob.access_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "banned");
    for refresh in [
        &bob.refresh_token,
        second["refresh_token"].as_str().expect("rt"),
    ] {
        let (status, body) = app
            .post("/api/auth/refresh", json!({ "refresh_token": refresh }))
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"]["code"], "banned");
    }
    let (status, body) = login(&app, "bob", PASSWORD).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "banned");
    // A wrong password doesn't learn about the ban.
    let (status, body) = login(&app, "bob", "wrong password!!").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "invalid_credentials");

    let bob_id = bob.user_id;
    assert_eq!(
        count(&app, &format!("SELECT count(*) FROM refresh_token WHERE user_id = '{bob_id}' AND revoked_at IS NULL")).await,
        0
    );
    assert_eq!(
        count(&app, &format!("SELECT count(*) FROM visibility_window WHERE user_id = '{bob_id}' AND (ended_at IS NULL OR lat IS NOT NULL)")).await,
        0
    );
    let (_, resolved) = app
        .get(
            "/api/admin/reports?status=resolved",
            Some(&admin.access_token),
        )
        .await;
    assert_eq!(resolved[0]["resolution"], "banned");
    assert_eq!(resolved[0]["resolved_by"]["id"], json!(admin.user_id));
    assert!(resolved[0]["subject"]["banned_at"].is_string());

    // Others no longer see bob anywhere, nor can they message him.
    let (_, nearby) = app.get("/api/nearby", Some(&eva.access_token)).await;
    assert!(nearby_ids(&nearby).is_empty());
    let (_, matches) = app.get("/api/matches", Some(&eva.access_token)).await;
    assert_eq!(matches, json!([]));
    let (status, _) = app
        .post_as(
            &format!("/api/matches/{match_id}/messages"),
            &eva.access_token,
            json!({ "body": "hello?" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Unban only lifts the ban: bob signs in again and the match is back.
    let (status, _) = act(&app, &admin, "unban", bob.user_id).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = login(&app, "bob", PASSWORD).await;
    assert_eq!(status, StatusCode::OK);
    let (_, matches) = app.get("/api/matches", Some(&eva.access_token)).await;
    assert_eq!(matches[0]["match_id"], json!(match_id));
    let (_, resolved) = app
        .get(
            "/api/admin/reports?status=resolved",
            Some(&admin.access_token),
        )
        .await;
    assert_eq!(resolved[0]["resolution"], "banned");
    assert_eq!(resolved[0]["subject"]["banned_at"], Value::Null);
}

#[tokio::test]
async fn visibility_sql_excludes_banned_users_even_with_an_open_window() {
    let app = TestApp::new().await;
    let eva = app.visible_user("eva", LAT, LON).await;
    let bob = app.visible_user("bob", LAT + 0.0027, LON).await;
    let (_, nearby) = app.get("/api/nearby", Some(&eva.access_token)).await;
    assert_eq!(nearby_ids(&nearby), [bob.user_id.to_string()]);

    // A window opened in a race with the ban must still not show.
    app.sql(&format!(
        "UPDATE \"user\" SET banned_at = now() WHERE id = '{}'",
        bob.user_id
    ))
    .await;
    let (_, nearby) = app.get("/api/nearby", Some(&eva.access_token)).await;
    assert!(nearby_ids(&nearby).is_empty());
    let (status, body) = app
        .post_as(
            "/api/waves",
            &eva.access_token,
            json!({ "to_user_id": bob.user_id }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "not_visible");
}

#[tokio::test]
async fn admins_cannot_be_banned_and_bans_are_idempotent() {
    let app = TestApp::new().await;
    let admin = app.admin("mod").await;
    let other = app.admin("mod2").await;
    let eva = app.register("eva").await;

    for target in [admin.user_id, other.user_id] {
        let (status, body) = act(&app, &admin, "ban", target).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"]["code"], "cannot_ban_admin");
    }
    assert_eq!(
        count(
            &app,
            "SELECT count(*) FROM \"user\" WHERE banned_at IS NOT NULL"
        )
        .await,
        0
    );

    let (status, _) = act(&app, &eva, "ban", other.user_id).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    for action in ["ban", "unban"] {
        let (status, _) = act(&app, &admin, action, uuid::Uuid::new_v4()).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{action}");
        let (status, _) = act(&app, &admin, action, "nope").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{action}");
    }

    let (status, _) = act(&app, &admin, "ban", eva.user_id).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    app.sql("UPDATE \"user\" SET banned_at = '2026-01-01T00:00:00Z' WHERE username = 'eva'")
        .await;
    let (status, _) = act(&app, &admin, "ban", eva.user_id).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        count(
            &app,
            "SELECT count(*) FROM \"user\" WHERE banned_at = '2026-01-01T00:00:00Z'"
        )
        .await,
        1,
        "a repeated ban keeps the original time"
    );
    // Unbanning someone not banned is fine too.
    act(&app, &admin, "unban", eva.user_id).await;
    let (status, _) = act(&app, &admin, "unban", eva.user_id).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

/// Holds `user`'s row `FOR UPDATE` like an in-flight ban, runs `request` meanwhile, then sets
/// `banned_at` and commits. Returns what `request` answered.
async fn during_ban<F>(app: &TestApp, user: uuid::Uuid, request: F) -> (StatusCode, Value)
where
    F: std::future::Future<Output = (StatusCode, Value)>,
{
    use sea_orm::{ConnectionTrait, TransactionTrait};
    let txn = app.db.begin().await.expect("begin");
    txn.execute_unprepared(&format!(
        "SELECT 1 FROM \"user\" WHERE id = '{user}' FOR UPDATE"
    ))
    .await
    .expect("lock");
    let ban = async {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        txn.execute_unprepared(&format!(
            "UPDATE \"user\" SET banned_at = now() WHERE id = '{user}'"
        ))
        .await
        .expect("ban");
        txn.commit().await.expect("commit");
    };
    let (res, ()) = tokio::join!(request, ban);
    res
}

#[tokio::test]
async fn writes_racing_a_ban_wait_for_it_and_are_refused() {
    let app = TestApp::new().await;
    let eva = app.visible_user("eva", LAT, LON).await;
    let bob = app.visible_user("bob", LAT + 0.0027, LON).await;
    let carl = app.visible_user("carl", LAT, LON + 0.001).await;

    // Each request passes the (unlocked) access check before the ban commits.
    let (status, body) = during_ban(
        &app,
        bob.user_id,
        app.post_as(
            "/api/me/window",
            &bob.access_token,
            json!({ "minutes": 60, "lat": LAT, "lon": LON }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "banned");

    let (status, body) = during_ban(
        &app,
        carl.user_id,
        app.post_as(
            "/api/waves",
            &carl.access_token,
            json!({ "to_user_id": eva.user_id }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "banned");
    assert_eq!(count(&app, "SELECT count(*) FROM wave").await, 0);

    let (status, body) = during_ban(
        &app,
        eva.user_id,
        app.post(
            "/api/auth/refresh",
            json!({ "refresh_token": eva.refresh_token }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "banned");
    let eva_id = eva.user_id;
    assert_eq!(
        count(
            &app,
            &format!("SELECT count(*) FROM refresh_token WHERE user_id = '{eva_id}' AND revoked_at IS NULL")
        )
        .await,
        1,
        "the presented token was not rotated"
    );
}
