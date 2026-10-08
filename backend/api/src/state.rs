use std::sync::Arc;

use anyhow::Result;
use sea_orm::DatabaseConnection;
use tokio::sync::Semaphore;

use crate::auth::email::{EmailLimiter, EmailService, ResetLimiter};
use crate::auth::oauth::OAuthService;
use crate::auth::telegram::TelegramService;
use crate::config::Config;
use crate::detached::Detached;
use crate::push::Notifier;
use crate::rate_limit::RateLimiter;
use crate::ws::Hub;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Arc<Config>,
    pub limiter: RateLimiter,
    pub hub: Arc<Hub>,
    /// Wave / match / message delivery: WebSocket, plus Web Push for offline recipients.
    pub notify: Arc<Notifier>,
    /// Concurrent photo decodes (`IMAGE_DECODE_PERMITS`); sized with the pod memory limit.
    pub image_permits: Arc<Semaphore>,
    /// `None` = email disabled; set by `with_email` (main builds it from `Config::email`).
    pub email: Option<EmailService>,
    pub email_limiter: EmailLimiter,
    pub reset_limiter: ResetLimiter,
    /// OAuth providers (Google, Telegram); each one is disabled unless configured.
    pub oauth: Arc<OAuthService>,
    /// `None` = no Telegram bot (no reset messages); set by `with_telegram`.
    pub telegram: Option<TelegramService>,
    /// Work finished after the response (password-reset lookups and delivery).
    pub detached: Detached,
}

/// Peak per decode is ~128 MiB (`image_proc::MAX_INPUT_PIXELS` × 4) plus resize buffers.
const IMAGE_DECODE_PERMITS: usize = 2;

impl AppState {
    /// Also starts this replica's WebSocket bridge (listener, publisher, heartbeat tasks).
    pub async fn new(db: DatabaseConnection, config: Config) -> Result<Self> {
        let limiter = RateLimiter::new(config.rate_limit, config.trust_proxy_headers);
        let hub = Hub::start(db.clone(), &config.database_url).await?;
        let notify = Arc::new(Notifier::new(
            db.clone(),
            hub.clone(),
            config.vapid.as_ref(),
        )?);
        let oauth = Arc::new(OAuthService::new(&config)?);
        Ok(Self {
            db,
            config: Arc::new(config),
            limiter,
            hub,
            notify,
            image_permits: Arc::new(Semaphore::new(IMAGE_DECODE_PERMITS)),
            email: None,
            email_limiter: EmailLimiter::default(),
            reset_limiter: ResetLimiter::default(),
            oauth,
            telegram: None,
            detached: Detached::default(),
        })
    }

    pub fn with_email(mut self, email: Option<EmailService>) -> Self {
        self.email = email;
        self
    }

    pub fn with_telegram(mut self, telegram: Option<TelegramService>) -> Self {
        self.telegram = telegram;
        self
    }
}
