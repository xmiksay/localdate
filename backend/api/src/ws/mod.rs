//! `/api/ws`: server push only. The first client frame authenticates the socket.

mod event;
mod hub;

use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::jwt;
use crate::state::AppState;

pub use event::ServerEvent;
pub use hub::Hub;

const AUTH_TIMEOUT: Duration = Duration::from_secs(10);
/// Application close code of docs/api.md for a bad or missing token.
pub const CLOSE_UNAUTHORIZED: u16 = 4401;

pub fn router() -> Router<AppState> {
    Router::new().route("/ws", get(upgrade))
}

#[derive(Deserialize)]
struct ClientFrame {
    #[serde(rename = "type")]
    kind: String,
    token: Option<String>,
}

/// The token of an `{"type":"auth","token":…}` frame; anything else is `None`.
fn auth_token(text: &str) -> Option<String> {
    let frame: ClientFrame = serde_json::from_str(text).ok()?;
    (frame.kind == "auth").then_some(frame.token).flatten()
}

async fn upgrade(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| session(socket, state))
}

/// Waits for the auth frame; protocol pings/pongs before it are skipped by the stack.
async fn authenticate(socket: &mut WebSocket, secret: &str) -> Option<Uuid> {
    loop {
        match socket.recv().await? {
            Ok(Message::Text(text)) => return jwt::verify(secret, &auth_token(&text)?),
            Ok(Message::Ping(_) | Message::Pong(_)) => {}
            _ => return None,
        }
    }
}

async fn reject(mut socket: WebSocket) {
    let frame = CloseFrame {
        code: CLOSE_UNAUTHORIZED,
        reason: "unauthorized".into(),
    };
    let _ = socket.send(Message::Close(Some(frame))).await;
}

async fn send_event(socket: &mut WebSocket, event: &ServerEvent) -> Result<(), ()> {
    let json = serde_json::to_string(event).map_err(|_| ())?;
    socket.send(Message::text(json)).await.map_err(|_| ())
}

async fn session(mut socket: WebSocket, state: AppState) {
    let user = tokio::time::timeout(
        AUTH_TIMEOUT,
        authenticate(&mut socket, &state.config.jwt_secret),
    )
    .await
    .ok()
    .flatten();
    let Some(user) = user else {
        return reject(socket).await;
    };

    let (id, mut events) = state.hub.subscribe(user);
    // Subscribed before `ready` so nothing pushed in between is lost; `ready` goes to this socket only.
    if send_event(&mut socket, &ServerEvent::Ready).await.is_err() {
        state.hub.unsubscribe(user, id);
        return;
    }
    loop {
        tokio::select! {
            event = events.recv() => {
                let Some(event) = event else { break };
                if send_event(&mut socket, &event).await.is_err() {
                    break;
                }
            }
            frame = socket.recv() => match frame {
                // Pings are answered by the protocol layer; other client frames are ignored.
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
        }
    }
    state.hub.unsubscribe(user, id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_auth_frames_with_a_token() {
        assert_eq!(
            auth_token(r#"{"type":"auth","token":"abc"}"#).as_deref(),
            Some("abc")
        );
        assert_eq!(auth_token(r#"{"type":"auth"}"#), None);
        assert_eq!(auth_token(r#"{"type":"ping","token":"abc"}"#), None);
        assert_eq!(auth_token("not json"), None);
    }
}
