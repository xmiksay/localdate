pub mod auth;
pub mod config;
pub mod discovery;
pub mod error;
pub mod interests;
pub mod me;
pub mod rate_limit;
pub mod safety;
pub mod social;
pub mod state;
pub mod web;
pub mod ws;

use axum::Router;
use axum::extract::Extension;
use axum::routing::get;
use rust_embed::RustEmbed;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use state::AppState;

/// The full router, exactly as served; integration tests build it through here.
pub fn app(state: AppState) -> Router {
    app_with_frontend::<web::Dist>(state)
}

/// `app` with the PWA bundle swapped for `F`, so tests can serve a fixture instead of `frontend/dist`.
pub fn app_with_frontend<F: RustEmbed + 'static>(state: AppState) -> Router {
    // Register each domain module's `router()` here.
    let api = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(auth::router())
        .merge(interests::router())
        .merge(me::router())
        .merge(safety::router())
        .merge(discovery::router())
        .merge(social::router())
        .merge(ws::router())
        // Without its own fallback the nested router would inherit the SPA one below.
        .fallback(|| async { error::AppError::NotFound });

    Router::new()
        .nest("/api", api)
        .nest_service("/media", ServeDir::new(&state.config.photo_dir))
        .fallback(web::serve::<F>)
        .layer(Extension(state.limiter.clone()))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
