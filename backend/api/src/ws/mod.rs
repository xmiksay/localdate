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

use crate::auth::extractor::{Account, account_status, verify_access};
use crate::error::AppError;
use crate::state::AppState;

pub use event::ServerEvent;
pub use hub::{CloseReason, Hub};

use hub::Outbound;

const AUTH_TIMEOUT: Duration = Duration::from_secs(10);

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
async fn authenticate(socket: &mut WebSocket, state: &AppState) -> Result<Uuid, AppError> {
    loop {
        match socket.recv().await {
            Some(Ok(Message::Text(text))) => {
                let token = auth_token(&text).ok_or(AppError::Unauthorized)?;
                return Ok(verify_access(&state.db, &state.config.jwt_secret, &token)
                    .await?
                    .id);
            }
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
            _ => return Err(AppError::Unauthorized),
        }
    }
}

async fn close(mut socket: WebSocket, reason: CloseReason) {
    let frame = CloseFrame {
        code: reason.code(),
        reason: reason.text().into(),
    };
    let _ = socket.send(Message::Close(Some(frame))).await;
}

async fn send_event(socket: &mut WebSocket, event: &ServerEvent) -> Result<(), ()> {
    let json = serde_json::to_string(event).map_err(|_| ())?;
    socket.send(Message::text(json)).await.map_err(|_| ())
}

async fn session(mut socket: WebSocket, state: AppState) {
    let user = match tokio::time::timeout(AUTH_TIMEOUT, authenticate(&mut socket, &state)).await {
        Ok(Ok(user)) => user,
        Ok(Err(AppError::Banned)) => return close(socket, CloseReason::Banned).await,
        Ok(Err(AppError::Internal)) => return close(socket, CloseReason::Internal).await,
        _ => return close(socket, CloseReason::Unauthorized).await,
    };

    let (id, mut events) = state.hub.subscribe(user);
    // A ban committed between the auth check and `subscribe` disconnected nothing; re-check.
    let refused = match account_status(&state.db, user).await {
        Ok(Account::Active { .. }) => None,
        Ok(Account::Banned) => Some(CloseReason::Banned),
        Ok(Account::Missing) => Some(CloseReason::Unauthorized),
        Err(_) => Some(CloseReason::Internal),
    };
    if let Some(reason) = refused {
        state.hub.unsubscribe(user, id);
        return close(socket, reason).await;
    }
    // Subscribed before `ready` so nothing pushed in between is lost; `ready` goes to this socket only.
    if send_event(&mut socket, &ServerEvent::Ready).await.is_err() {
        state.hub.unsubscribe(user, id);
        return;
    }
    loop {
        tokio::select! {
            out = events.recv() => match out {
                Some(Outbound::Event(event)) => {
                    if send_event(&mut socket, &event).await.is_err() {
                        break;
                    }
                }
                Some(Outbound::Close(reason)) => {
                    state.hub.unsubscribe(user, id);
                    return close(socket, reason).await;
                }
                None => break,
            },
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
