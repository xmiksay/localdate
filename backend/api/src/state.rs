use std::sync::Arc;

use anyhow::Result;
use sea_orm::DatabaseConnection;
use tokio::sync::Semaphore;

use crate::auth::email::{EmailLimiter, EmailService};
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
    /// `None` = email disabled; set by `with_email` (main builds it from `Config::email`).
    pub email: Option<EmailService>,
    pub email_limiter: EmailLimiter,
}

/// Peak per decode is ~128 MiB (`image_proc::MAX_INPUT_PIXELS` × 4) plus resize buffers.
const IMAGE_DECODE_PERMITS: usize = 2;

impl AppState {
    /// Also starts this replica's WebSocket bridge (listener, publisher, heartbeat tasks).
    pub async fn new(db: DatabaseConnection, config: Config) -> Result<Self> {
        let limiter = RateLimiter::new(config.rate_limit, config.trust_proxy_headers);
        let hub = Hub::start(db.clone(), &config.database_url).await?;
        Ok(Self {
            db,
            config: Arc::new(config),
            limiter,
            hub,
            image_permits: Arc::new(Semaphore::new(IMAGE_DECODE_PERMITS)),
            email: None,
            email_limiter: EmailLimiter::default(),
        })
    }

    pub fn with_email(mut self, email: Option<EmailService>) -> Self {
        self.email = email;
        self
    }
}
