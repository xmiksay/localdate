use std::sync::Arc;

use sea_orm::DatabaseConnection;
use tokio::sync::Semaphore;

use crate::config::Config;
use crate::rate_limit::RateLimiter;
use crate::ws::Hub;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Arc<Config>,
    pub limiter: RateLimiter,
    pub hub: Arc<Hub>,
    /// Concurrent photo decodes (`IMAGE_DECODE_PERMITS`); sized with the pod memory limit.
    pub image_permits: Arc<Semaphore>,
}

/// Peak per decode is ~128 MiB (`image_proc::MAX_INPUT_PIXELS` × 4) plus resize buffers.
const IMAGE_DECODE_PERMITS: usize = 2;

impl AppState {
    pub fn new(db: DatabaseConnection, config: Config) -> Self {
        let limiter = RateLimiter::new(config.rate_limit, config.trust_proxy_headers);
        Self {
            db,
            config: Arc::new(config),
            limiter,
            hub: Arc::new(Hub::default()),
            image_permits: Arc::new(Semaphore::new(IMAGE_DECODE_PERMITS)),
        }
    }
}
