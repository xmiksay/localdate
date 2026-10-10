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

use crate::admin::audit;
use crate::auth::extractor::{authorize, verify_access};
use crate::auth::{ActingUser, jwt};
use crate::error::AppError;
use crate::state::AppState;

pub use bridge::Hub;
pub use event::ServerEvent;
pub use hub::CloseReason;
pub use presence::REPLICA_STALE;

use hub::Outbound;

const AUTH_TIMEOUT: Duration = Duration::from_secs(10);
/// How impersonated sockets appear in the audit log.
const WS_ROUTE: &str = "/api/ws";

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
async fn authenticate(socket: &mut WebSocket, state: &AppState) -> Result<ActingUser, AppError> {
    loop {
        match socket.recv().await {
            Some(Ok(Message::Text(text))) => {
                let token = auth_token(&text).ok_or(AppError::Unauthorized)?;
                return verify_access(&state.db, &state.config, &token).await;
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

/// Why an authenticated socket must close, given why its token is refused now; any other error
/// is a failed check, not a refusal.
fn refusal(err: AppError) -> Result<CloseReason, AppError> {
    match err {
        AppError::Banned => Ok(CloseReason::Banned),
        AppError::Unauthorized => Ok(CloseReason::Unauthorized),
        other => Err(other),
    }
}

/// Re-reads the account(s) behind an open socket's token (the acting admin's too).
async fn recheck(state: &AppState, access: jwt::Access) -> Result<Option<CloseReason>, AppError> {
    match authorize(&state.db, state.config.admin_impersonation, access).await {
        Ok(_) => Ok(None),
        Err(e) => refusal(e).map(Some),
    }
}

async fn session(mut socket: WebSocket, state: AppState) {
    let access = match tokio::time::timeout(AUTH_TIMEOUT, authenticate(&mut socket, &state)).await {
        Ok(Ok(auth)) => auth.access(),
        Ok(Err(AppError::Banned)) => return close(socket, CloseReason::Banned).await,
        Ok(Err(AppError::Internal)) => return close(socket, CloseReason::Internal).await,
        _ => return close(socket, CloseReason::Unauthorized).await,
    };
    let Some(admin) = access.actor else {
        return connected(socket, state, access).await;
    };
    // An admin watching the target's realtime is audited like any request, start and end.
    let record =
        |event: &'static str| audit::record_request(&state.db, admin, access.user, event, WS_ROUTE);
    if record(audit::WS_OPEN).await.is_err() {
        return close(socket, CloseReason::Internal).await;
    }
    connected(socket, state.clone(), access).await;
    if let Err(e) = record(audit::WS_CLOSE).await {
        tracing::error!(error = %e, "auditing the end of an impersonated socket failed");
    }
}

/// An authenticated socket, from subscription to close.
async fn connected(mut socket: WebSocket, state: AppState, access: jwt::Access) {
    let user = access.user;

    let (id, mut events) = state.hub.subscribe(user).await;
    // A ban committed between the auth check and `subscribe` disconnected nothing; re-check.
    let refused = recheck(&state, access)
        .await
        .unwrap_or(Some(CloseReason::Internal));
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
    let mut recheck_every = tokio::time::interval_at(
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
            _ = recheck_every.tick() => match recheck(&state, access).await {
                Ok(refused) => if let Some(reason) = refused {
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
        assert_eq!(refusal(AppError::Banned).ok(), Some(CloseReason::Banned));
        assert_eq!(
            refusal(AppError::Unauthorized).ok(),
            Some(CloseReason::Unauthorized)
        );
        assert!(refusal(AppError::Internal).is_err(), "a failed check");
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
