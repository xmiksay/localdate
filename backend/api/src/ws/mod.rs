//! `/api/ws`: server push only. The first client frame authenticates the socket.

mod bridge;
mod envelope;
mod event;
mod hub;
mod presence;
mod publisher;

use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use serde::Deserialize;

use crate::auth::AuthUser;
use crate::auth::extractor::{Account, account_status, verify_access};
use crate::error::AppError;
use crate::state::AppState;

pub use bridge::Hub;
pub use event::ServerEvent;
pub use hub::CloseReason;
pub use presence::REPLICA_STALE;

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
async fn authenticate(socket: &mut WebSocket, state: &AppState) -> Result<AuthUser, AppError> {
    loop {
        match socket.recv().await {
            Some(Ok(Message::Text(text))) => {
                let token = auth_token(&text).ok_or(AppError::Unauthorized)?;
                return verify_access(&state.db, &state.config.jwt_secret, &token).await;
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

/// Why an authenticated socket must close, given its account's current state.
fn refusal(account: &Account) -> Option<CloseReason> {
    match account {
        Account::Active { .. } => None,
        Account::Banned => Some(CloseReason::Banned),
        Account::Missing | Account::Superseded => Some(CloseReason::Unauthorized),
    }
}

async fn session(mut socket: WebSocket, state: AppState) {
    let (user, issued_at) =
        match tokio::time::timeout(AUTH_TIMEOUT, authenticate(&mut socket, &state)).await {
            Ok(Ok(auth)) => (auth.id, auth.issued_at),
            Ok(Err(AppError::Banned)) => return close(socket, CloseReason::Banned).await,
            Ok(Err(AppError::Internal)) => return close(socket, CloseReason::Internal).await,
            _ => return close(socket, CloseReason::Unauthorized).await,
        };

    let (id, mut events) = state.hub.subscribe(user).await;
    // A ban committed between the auth check and `subscribe` disconnected nothing; re-check.
    let refused = match account_status(&state.db, user, issued_at).await {
        Ok(account) => refusal(&account),
        Err(_) => Some(CloseReason::Internal),
    };
    if let Some(reason) = refused {
        state.hub.unsubscribe(user, id).await;
        return close(socket, reason).await;
    }
    // Subscribed before `ready` so nothing pushed in between is lost; `ready` goes to this socket only.
    if send_event(&mut socket, &ServerEvent::Ready).await.is_err() {
        state.hub.unsubscribe(user, id).await;
        return;
    }
    // Defence in depth: a ban whose Close op never reached this replica still ends the socket.
    let mut recheck = tokio::time::interval_at(
        tokio::time::Instant::now() + state.config.ws_account_recheck,
        state.config.ws_account_recheck,
    );
    let mut ping = tokio::time::interval_at(
        tokio::time::Instant::now() + state.config.ws_ping_every,
        state.config.ws_ping_every,
    );
    let mut last_heard = tokio::time::Instant::now();
    loop {
        tokio::select! {
            _ = ping.tick() => {
                // Silence past the timeout means a dead peer (sleeping phone, dropped network):
                // closing drops the presence row, so the user reads as offline and gets pushes.
                if last_heard.elapsed() > state.config.ws_idle_timeout {
                    tracing::debug!("ws peer silent past idle timeout; closing");
                    break;
                }
                if socket.send(Message::Ping(Default::default())).await.is_err() {
                    break;
                }
            },
            _ = recheck.tick() => match account_status(&state.db, user, issued_at).await {
                Ok(account) => if let Some(reason) = refusal(&account) {
                    state.hub.unsubscribe(user, id).await;
                    return close(socket, reason).await;
                },
                // A DB hiccup is no reason to drop the socket; the next tick checks again.
                Err(_) => tracing::debug!("ws account re-check failed"),
            },
            out = events.recv() => match out {
                Some(Outbound::Event(event)) => {
                    if send_event(&mut socket, &event).await.is_err() {
                        break;
                    }
                }
                Some(Outbound::Close(reason)) => {
                    state.hub.unsubscribe(user, id).await;
                    return close(socket, reason).await;
                }
                None => break,
            },
            frame = socket.recv() => match frame {
                // Pings are answered by the protocol layer; other client frames are ignored.
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => last_heard = tokio::time::Instant::now(),
            },
        }
    }
    state.hub.unsubscribe(user, id).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refusal_follows_the_account_state() {
        assert_eq!(refusal(&Account::Active { is_admin: false }), None);
        assert_eq!(refusal(&Account::Banned), Some(CloseReason::Banned));
        assert_eq!(refusal(&Account::Missing), Some(CloseReason::Unauthorized));
    }

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
