//! Waves, matches and chat.

mod matches;
mod messages;
mod waves;

use axum::Router;
use axum::routing::{get, post};

use crate::state::AppState;

pub use matches::{MatchSummaryDto, OtherDto, summaries};
pub use messages::MessageDto;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/waves", post(waves::post_wave))
        .route("/waves/incoming", get(waves::incoming))
        .route("/matches", get(matches::list))
        .route(
            "/matches/{id}/messages",
            get(messages::list).post(messages::send),
        )
}
