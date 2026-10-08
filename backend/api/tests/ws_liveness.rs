//! Server pings: a silent (half-open) socket is closed, so its user stops counting as online.

mod common;

use std::time::Duration;

use common::{TestApp, ws};
use futures_util::StreamExt;
use localdate_api::config::Config;
use uuid::Uuid;

fn fast_pings(c: &mut Config) {
    c.ws_ping_every = Duration::from_millis(100);
    c.ws_idle_timeout = Duration::from_millis(300);
}

/// Polls presence until it equals `want` or `within` passes; returns the last reading.
async fn online_settles_to(app: &TestApp, user: Uuid, want: bool, within: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + within;
    loop {
        let online = app.state.hub.is_online(user).await.expect("presence query");
        if online == want || tokio::time::Instant::now() >= deadline {
            return online;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn a_socket_that_stops_answering_pings_is_dropped_and_its_user_goes_offline() {
    let app = TestApp::with_config(fast_pings).await;
    let t = app.register("anna").await;
    let url = ws::serve(&app.router).await;
    // Never read again: tungstenite answers pings only while the stream is polled, so this is
    // what a frozen tab or a half-open connection looks like to the server.
    let _silent = ws::ready_socket(&url, &t).await;
    assert!(app.state.hub.is_online(t.user_id).await.expect("presence"));

    let online = online_settles_to(&app, t.user_id, false, Duration::from_secs(3)).await;
    assert!(!online, "silent socket must be dropped");
}

#[tokio::test]
async fn a_socket_that_answers_pings_stays_online() {
    let app = TestApp::with_config(fast_pings).await;
    let t = app.register("anna").await;
    let url = ws::serve(&app.router).await;
    let mut socket = ws::ready_socket(&url, &t).await;
    // Reading lets the client answer every ping with a pong.
    let reader = tokio::spawn(async move { while socket.next().await.is_some() {} });

    tokio::time::sleep(Duration::from_millis(900)).await;
    assert!(app.state.hub.is_online(t.user_id).await.expect("presence"));
    reader.abort();
}
