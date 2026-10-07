use std::sync::Arc;

use sea_orm::DatabaseConnection;

use crate::config::Config;
use crate::rate_limit::RateLimiter;
use crate::ws::Hub;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Arc<Config>,
    pub limiter: RateLimiter,
    pub hub: Arc<Hub>,
}

impl AppState {
    pub fn new(db: DatabaseConnection, config: Config) -> Self {
        let limiter = RateLimiter::new(config.rate_limit);
        Self {
            db,
            config: Arc::new(config),
            limiter,
            hub: Arc::new(Hub::default()),
        }
    }
}
