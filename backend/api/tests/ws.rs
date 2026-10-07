mod common;

use std::time::Duration;

use common::{TestApp, Tokens};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn serve(app: &TestApp) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    let router = app.router.clone();
    tokio::spawn(async move { axum::serve(listener, router).await });
    format!("ws://{addr}/api/ws")
}

async fn connect(url: &str) -> Ws {
    connect_async(url).await.expect("connect").0
}

async fn auth(ws: &mut Ws, token: &str) {
    let frame = json!({ "type": "auth", "token": token }).to_string();
    ws.send(Message::text(frame)).await.expect("send auth");
}

/// Next text frame as JSON, or the close frame's code.
async fn next(ws: &mut Ws) -> Result<Value, u16> {
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("timed out waiting for a frame")
            .expect("stream ended")
            .expect("ws error");
        match frame {
            Message::Text(t) => return Ok(serde_json::from_str(t.as_str()).expect("json")),
            Message::Close(frame) => return Err(frame.map_or(1005, |f| u16::from(f.code))),
            _ => {}
        }
    }
}

async fn ready_socket(url: &str, t: &Tokens) -> Ws {
    let mut ws = connect(url).await;
    auth(&mut ws, &t.access_token).await;
    assert_eq!(next(&mut ws).await, Ok(json!({ "type": "ready" })));
    ws
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
