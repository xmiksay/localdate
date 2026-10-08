//! Two API replicas on one database: WebSocket fan-out and presence across them.
mod common;

use std::time::Duration;

use axum::http::StatusCode;
use common::ws::{next, ready_socket, serve};
use common::{TestApp, count};
use localdate_api::cleanup::run_once;
use localdate_api::social::{MatchSummaryDto, MessageDto, OtherDto};
use localdate_api::ws::ServerEvent;
use serde_json::json;
use uuid::Uuid;

/// Polls `hub.is_online(user)` until it equals `want` (sessions end asynchronously).
async fn wait_online(hub: &localdate_api::ws::Hub, user: Uuid, want: bool) {
    for _ in 0..100 {
        if hub.is_online(user).await.expect("presence query") == want {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("is_online({user}) never became {want}");
}

#[tokio::test]
async fn events_published_on_one_replica_reach_sockets_on_the_other() {
    let app = TestApp::new().await;
    let other = app.replica().await;
    let (url_a, url_b) = (serve(&app.router).await, serve(&other.router).await);
    let anna = app.visible_user("anna", 50.0870, 14.4210).await;
    let bob = app.visible_user("bob", 50.0897, 14.4210).await;
    let mut ws_anna = ready_socket(&url_a, &anna).await;
    let mut ws_bob = ready_socket(&url_b, &bob).await;

    let wave = json!({ "to_user_id": anna.user_id });
    let (status, _) = other.post_as("/api/waves", &bob.access_token, wave).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        next(&mut ws_anna).await,
        Ok(json!({ "type": "wave", "from_user_id": bob.user_id }))
    );

    // Matched on A: bob's socket on B gets his match event through the bridge.
    let wave = json!({ "to_user_id": bob.user_id });
    let (_, res) = app.post_as("/api/waves", &anna.access_token, wave).await;
    let match_id = res["match_id"].as_str().expect("match id").to_owned();
    for (ws, other_user) in [(&mut ws_anna, bob.user_id), (&mut ws_bob, anna.user_id)] {
        let ev = next(ws).await.expect("match event");
        assert_eq!(ev["type"], "match");
        assert_eq!(ev["match"]["match_id"], json!(match_id));
        assert_eq!(ev["match"]["other"]["user_id"], json!(other_user));
    }

    // The longest body the API takes is over the NOTIFY limit; A loads it by reference.
    let path = format!("/api/matches/{match_id}/messages");
    let long = "😀".repeat(2000);
    let (status, _) = other
        .post_as(&path, &bob.access_token, json!({ "body": long }))
        .await;
    assert_eq!(status, StatusCode::CREATED);
    for ws in [&mut ws_anna, &mut ws_bob] {
        let ev = next(ws).await.expect("message event");
        assert_eq!(ev["type"], "message");
        assert_eq!(ev["message"]["body"], json!(long));
    }
    // Exactly once per socket: the next event is the next message, not a duplicate.
    other
        .post_as(&path, &bob.access_token, json!({ "body": "short" }))
        .await;
    for ws in [&mut ws_anna, &mut ws_bob] {
        assert_eq!(next(ws).await.expect("event")["message"]["body"], "short");
    }
}

#[tokio::test]
async fn ban_on_one_replica_closes_sockets_on_the_other_with_4403() {
    let app = TestApp::new().await;
    let other = app.replica().await;
    let url_a = serve(&app.router).await;
    let admin = app.admin("mod").await;
    let bob = app.register("bob").await;
    let mut ws_bob = ready_socket(&url_a, &bob).await;

    let path = format!("/api/admin/users/{}/ban", bob.user_id);
    let (status, _) = other.post_as(&path, &admin.access_token, json!({})).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(next(&mut ws_bob).await, Err(4403));
}

#[tokio::test]
async fn presence_spans_replicas_and_ends_with_the_socket_or_a_stale_replica() {
    let app = TestApp::new().await;
    let other = app.replica().await;
    let url_a = serve(&app.router).await;
    let anna = app.register("anna").await;
    let bob = app.register("bob").await;
    let hub_b = &other.state.hub;

    let mut ws = ready_socket(&url_a, &anna).await;
    assert!(hub_b.is_online(anna.user_id).await.expect("presence"));
    assert!(!hub_b.is_online(bob.user_id).await.expect("presence"));
    ws.close(None).await.expect("close");
    wait_online(hub_b, anna.user_id, false).await;
    assert_eq!(count(&app, "SELECT count(*) FROM ws_presence").await, 0);

    let _ws = ready_socket(&url_a, &anna).await;
    let _ws2 = ready_socket(&url_a, &anna).await;
    assert_eq!(count(&app, "SELECT count(*) FROM ws_presence").await, 2);
    assert!(hub_b.is_online(anna.user_id).await.expect("presence"));
    // A replica that stopped heartbeating no longer vouches for its sockets.
    app.sql("UPDATE ws_replica SET seen_at = now() - interval '91 seconds'")
        .await;
    assert!(!hub_b.is_online(anna.user_id).await.expect("presence"));
    // ...and the cleanup job reclaims its rows.
    let counts = run_once(&app.db, chrono::Duration::zero())
        .await
        .expect("cleanup")
        .expect("lock");
    assert_eq!(counts.replicas_deleted, 2);
    assert_eq!(count(&app, "SELECT count(*) FROM ws_presence").await, 0);

    // The replica was only slow, not dead: its next heartbeat brings both sockets back.
    app.state.hub.heartbeat().await.expect("heartbeat");
    assert_eq!(count(&app, "SELECT count(*) FROM ws_presence").await, 2);
    assert!(hub_b.is_online(anna.user_id).await.expect("presence"));
}

#[tokio::test]
async fn heartbeat_drops_rows_of_sockets_that_are_gone() {
    let app = TestApp::new().await;
    let url = serve(&app.router).await;
    let anna = app.register("anna").await;
    let _ws = ready_socket(&url, &anna).await;
    // A row whose delete never happened (DB error, aborted session).
    app.sql(&format!(
        "INSERT INTO ws_presence (replica_id, socket_id, user_id, connected_at) \
         SELECT replica_id, 999999, '{}', now() - interval '1 minute' FROM ws_replica",
        anna.user_id
    ))
    .await;
    assert_eq!(count(&app, "SELECT count(*) FROM ws_presence").await, 2);
    app.state.hub.heartbeat().await.expect("heartbeat");
    assert_eq!(
        count(
            &app,
            "SELECT count(*) FROM ws_presence WHERE socket_id = 999999"
        )
        .await,
        0
    );
    assert_eq!(count(&app, "SELECT count(*) FROM ws_presence").await, 1);
}

#[tokio::test]
async fn graceful_shutdown_drops_the_replica_presence() {
    let app = TestApp::new().await;
    let other = app.replica().await;
    let url_b = serve(&other.router).await;
    let anna = app.register("anna").await;
    let mut ws = ready_socket(&url_b, &anna).await;
    assert!(
        app.state
            .hub
            .is_online(anna.user_id)
            .await
            .expect("presence")
    );

    other.state.hub.shutdown().await;
    // Open sockets are told to go elsewhere before the replica deregisters.
    assert_eq!(next(&mut ws).await, Err(1001));
    assert!(
        !app.state
            .hub
            .is_online(anna.user_id)
            .await
            .expect("presence")
    );
    assert_eq!(count(&app, "SELECT count(*) FROM ws_replica").await, 1);
}

#[tokio::test]
async fn lost_listener_reconnects_and_makes_clients_resync() {
    let app = TestApp::new().await;
    let other = app.replica().await;
    let url_a = serve(&app.router).await;
    let anna = app.register("anna").await;
    let bob = app.register("bob").await;
    let mut ws = ready_socket(&url_a, &anna).await;

    app.sql(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity \
         WHERE datname = current_database() AND query LIKE 'LISTEN%'",
    )
    .await;
    // Events may have been missed while the listener was down: the client must reconnect.
    assert_eq!(next(&mut ws).await, Err(1012));

    // The 1012 is sent only once A listens again, so B's next event reaches the new socket.
    let mut ws = ready_socket(&url_a, &anna).await;
    other.state.hub.send(
        anna.user_id,
        &ServerEvent::Wave {
            from_user_id: bob.user_id,
        },
    );
    assert_eq!(
        next(&mut ws).await,
        Ok(json!({ "type": "wave", "from_user_id": bob.user_id }))
    );
}

#[tokio::test]
async fn oversized_match_event_is_rebuilt_by_the_receiving_replica() {
    let app = TestApp::new().await;
    let other = app.replica().await;
    let url_a = serve(&app.router).await;
    let anna = app.visible_user("anna", 50.0870, 14.4210).await;
    let bob = app.visible_user("bob", 50.0897, 14.4210).await;
    let match_id: Uuid = app.match_up(&anna, &bob).await.parse().expect("uuid");
    let path = format!("/api/matches/{match_id}/messages");
    let long = "😀".repeat(2000);
    app.post_as(&path, &bob.access_token, json!({ "body": long }))
        .await;
    let mut ws = ready_socket(&url_a, &anna).await;

    // What B would push if a match summary carried a long last message.
    let summary = MatchSummaryDto {
        match_id,
        created_at: chrono::Utc::now(),
        other: OtherDto {
            user_id: bob.user_id,
            display_name: "Tester".into(),
            photo_url: None,
        },
        last_message: Some(MessageDto {
            id: Uuid::new_v4(),
            match_id,
            sender_id: bob.user_id,
            body: long.clone(),
            created_at: chrono::Utc::now(),
        }),
    };
    other
        .state
        .hub
        .send(anna.user_id, &ServerEvent::Match { summary });
    let ev = next(&mut ws).await.expect("match event");
    assert_eq!(ev["type"], "match");
    assert_eq!(ev["match"]["match_id"], json!(match_id));
    assert_eq!(ev["match"]["other"]["user_id"], json!(bob.user_id));
    assert_eq!(ev["match"]["last_message"]["body"], json!(long));
}
