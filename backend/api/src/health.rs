//! Probes plus the build identity baked in by `build.rs`.
//!
//! `/health` (startup + readiness) pings the DB, so an outage takes pods out of rotation;
//! `/live` (liveness) checks nothing, so the same outage never restart-loops them.

use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::state::AppState;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const COMMIT: &str = env!("LOCALDATE_COMMIT");
pub const BUILT_AT: Option<&str> = option_env!("LOCALDATE_BUILT_AT");

/// Below the probes' `timeoutSeconds: 2` (deploy manifests), so a hung pool answers a clean 503
/// instead of a probe timeout.
const DB_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Serialize, Debug, PartialEq)]
struct Health {
    status: &'static str,
    version: &'static str,
    commit: &'static str,
    built_at: Option<&'static str>,
    db: &'static str,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/live", get(|| async { StatusCode::OK }))
}

async fn health(State(state): State<AppState>) -> (StatusCode, Json<Health>) {
    let db_ok = match tokio::time::timeout(DB_TIMEOUT, state.db.ping()).await {
        Ok(Ok(())) => true,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "health: database ping failed");
            false
        }
        Err(_) => {
            tracing::warn!("health: database ping timed out");
            false
        }
    };
    report(db_ok)
}

fn report(db_ok: bool) -> (StatusCode, Json<Health>) {
    let (code, status, db) = if db_ok {
        (StatusCode::OK, "ok", "ok")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "degraded", "unavailable")
    };
    let body = Health {
        status,
        version: VERSION,
        commit: COMMIT,
        built_at: BUILT_AT,
        db,
    };
    (code, Json(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_is_a_short_sha_or_dev() {
        assert!(
            COMMIT == "dev" || (COMMIT.len() == 7 && COMMIT.bytes().all(|b| b.is_ascii_hexdigit())),
            "{COMMIT}"
        );
    }

    #[test]
    fn db_state_maps_to_status() {
        let (code, Json(body)) = report(true);
        assert_eq!((code, body.status, body.db), (StatusCode::OK, "ok", "ok"));
        let (code, Json(body)) = report(false);
        assert_eq!(
            (code, body.status, body.db),
            (StatusCode::SERVICE_UNAVAILABLE, "degraded", "unavailable")
        );
        assert_eq!(body.version, VERSION);
    }
}
