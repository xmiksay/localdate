mod common;

use std::time::Duration;

use common::TestApp;
use common::ws::{auth, connect, next, ready_socket};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::tungstenite::Message;

async fn serve(app: &TestApp) -> String {
    common::ws::serve(&app.router).await
}

#[tokio::test]
async fn bad_token_or_frame_closes_with_4401() {
    let app = TestApp::new().await;
    let url = serve(&app).await;

    let mut ws = connect(&url).await;
    auth(&mut ws, "not-a-jwt").await;
    assert_eq!(next(&mut ws).await, Err(4401));

    let mut ws = connect(&url).await;
    ws.send(Message::text("hello")).await.expect("send");
    assert_eq!(next(&mut ws).await, Err(4401));
}

#[tokio::test]
async fn pushes_wave_match_and_message_events() {
    let app = TestApp::new().await;
    let url = serve(&app).await;
    let a = app.visible_user("anna", 50.0870, 14.4210).await;
    let b = app.visible_user("bob", 50.0897, 14.4210).await;
    let mut ws_a = ready_socket(&url, &a).await;
    let mut ws_b = ready_socket(&url, &b).await;
    // A second tab of B receives the same events.
    let mut ws_b2 = ready_socket(&url, &b).await;

    app.post_as(
        "/api/waves",
        &a.access_token,
        json!({ "to_user_id": b.user_id }),
    )
    .await;
    for ws in [&mut ws_b, &mut ws_b2] {
        let ev = next(ws).await.expect("wave event");
        assert_eq!(ev, json!({ "type": "wave", "from_user_id": a.user_id }));
    }

    let (_, res) = app
        .post_as(
            "/api/waves",
            &b.access_token,
            json!({ "to_user_id": a.user_id }),
        )
        .await;
    let match_id = res["match_id"].as_str().expect("match id").to_owned();
    for (ws, other) in [(&mut ws_a, &b), (&mut ws_b, &a)] {
        let ev = next(ws).await.expect("match event");
        assert_eq!(ev["type"], "match");
        assert_eq!(ev["match"]["match_id"], json!(match_id));
        assert_eq!(ev["match"]["other"]["user_id"], json!(other.user_id));
    }
    let ev = next(&mut ws_b2).await.expect("match event on second tab");
    assert_eq!(ev["type"], "match");

    app.post_as(
        &format!("/api/matches/{match_id}/messages"),
        &a.access_token,
        json!({ "body": "hi there" }),
    )
    .await;
    // Sender and recipient both get it.
    for ws in [&mut ws_a, &mut ws_b] {
        let ev = next(ws).await.expect("message event");
        assert_eq!(ev["type"], "message");
        assert_eq!(ev["message"]["body"], "hi there");
        assert_eq!(ev["message"]["sender_id"], json!(a.user_id));
    }

    // A closed socket is dropped without disturbing the others.
    drop(ws_b2);
    app.post_as(
        &format!("/api/matches/{match_id}/messages"),
        &a.access_token,
        json!({ "body": "again" }),
    )
    .await;
    assert_eq!(
        next(&mut ws_b).await.expect("event")["message"]["body"],
        "again"
    );
}

#[tokio::test]
async fn ban_closes_open_sockets_and_refuses_new_ones_with_4403() {
    let app = TestApp::new().await;
    let url = serve(&app).await;
    let admin = app.admin("mod").await;
    let bob = app.register("bob").await;
    let eva = app.register("eva").await;
    let mut ws_bob = ready_socket(&url, &bob).await;
    let mut ws_bob2 = ready_socket(&url, &bob).await;
    let mut ws_eva = ready_socket(&url, &eva).await;

    let (status, _) = app
        .post_as(
            &format!("/api/admin/users/{}/ban", bob.user_id),
            &admin.access_token,
            json!({}),
        )
        .await;
    assert_eq!(status, axum::http::StatusCode::NO_CONTENT);
    assert_eq!(next(&mut ws_bob).await, Err(4403));
    assert_eq!(next(&mut ws_bob2).await, Err(4403));

    let mut ws = connect(&url).await;
    auth(&mut ws, &bob.access_token).await;
    assert_eq!(next(&mut ws).await, Err(4403));

    // Others stay connected.
    ws_eva
        .send(Message::Ping(vec![].into()))
        .await
        .expect("ping");
    let frame = tokio::time::timeout(Duration::from_secs(5), ws_eva.next())
        .await
        .expect("pong in time")
        .expect("open")
        .expect("ok");
    assert!(matches!(frame, Message::Pong(_)));
}

#[tokio::test]
async fn periodic_account_check_closes_sockets_a_missed_ban_or_deletion_left_open() {
    let app = TestApp::with_config(|c| c.ws_account_recheck = Duration::from_millis(200)).await;
    let url = serve(&app).await;
    let bob = app.register("bob").await;
    let eva = app.register("eva").await;
    let mut ws_bob = ready_socket(&url, &bob).await;
    let mut ws_eva = ready_socket(&url, &eva).await;

    // Straight in the DB: no hub disconnect, as if the bridge had lost the Close op.
    app.sql(&format!(
        "UPDATE \"user\" SET banned_at = now() WHERE id = '{}'",
        bob.user_id
    ))
    .await;
    assert_eq!(next(&mut ws_bob).await, Err(4403));
    app.sql(&format!(
        "DELETE FROM \"user\" WHERE id = '{}'",
        eva.user_id
    ))
    .await;
    assert_eq!(next(&mut ws_eva).await, Err(4401));
}
