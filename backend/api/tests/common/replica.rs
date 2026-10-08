//! A second API replica on the test database, for cross-replica WebSocket tests.

use axum::Router;
use axum::http::{Method, StatusCode};
use localdate_api::state::AppState;
use sea_orm::Database;
use serde_json::Value;

use super::{TestApp, call, database_url};

/// Own connection pool, hub (listener, publisher, heartbeat) and router; shares only the database.
pub struct Replica {
    pub router: Router,
    pub state: AppState,
}

impl TestApp {
    pub async fn replica(&self) -> Replica {
        let db = Database::connect(database_url(&self.admin_url, &self.db_name))
            .await
            .expect("connect replica pool");
        let config = (*self.state.config).clone();
        let state = AppState::new(db, config).await.expect("replica state");
        Replica {
            router: localdate_api::app(state.clone()),
            state,
        }
    }
}

impl Replica {
    /// Authenticated JSON POST served by this replica.
    pub async fn post_as(&self, path: &str, token: &str, body: Value) -> (StatusCode, Value) {
        call(&self.router, Method::POST, path, Some(token), Some(body)).await
    }
}
