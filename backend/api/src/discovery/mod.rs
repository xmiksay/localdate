//! Visibility window, location and the mutual-filter "nearby" list.

pub mod geo;
pub mod nearby;
pub mod rules;
pub mod window;

use axum::Router;
use axum::routing::{get, post};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/me/window",
            get(window::get_window)
                .post(window::start_window)
                .patch(window::extend_window)
                .delete(window::end_window),
        )
        .route("/me/location", post(window::update_location))
        .route("/nearby", get(nearby::get_nearby))
}
