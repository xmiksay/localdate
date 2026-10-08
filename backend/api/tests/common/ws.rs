//! WebSocket test client: serve a router on a real port and talk `/api/ws` to it.

use std::time::Duration;

use axum::Router;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use super::Tokens;

pub type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Serves `router` on an ephemeral port; returns its `/api/ws` URL.
pub async fn serve(router: &Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    let router = router.clone();
    tokio::spawn(async move { axum::serve(listener, router).await });
    format!("ws://{addr}/api/ws")
}

pub async fn connect(url: &str) -> Ws {
    connect_async(url).await.expect("connect").0
}

pub async fn auth(ws: &mut Ws, token: &str) {
    let frame = json!({ "type": "auth", "token": token }).to_string();
    ws.send(Message::text(frame)).await.expect("send auth");
}

/// Next text frame as JSON, or the close frame's code.
pub async fn next(ws: &mut Ws) -> Result<Value, u16> {
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

pub async fn ready_socket(url: &str, t: &Tokens) -> Ws {
    let mut ws = connect(url).await;
    auth(&mut ws, &t.access_token).await;
    assert_eq!(next(&mut ws).await, Ok(json!({ "type": "ready" })));
    ws
}
