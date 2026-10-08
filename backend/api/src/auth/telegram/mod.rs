//! The Telegram bot: password-reset links by message. Logging in with Telegram is an OAuth
//! provider (`auth::oauth`); the bot may message a user who granted `telegram:bot_access` there.

pub mod bot;

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;

use self::bot::TelegramBot;
use crate::auth::email::message::Lang;

/// Bot messages in flight at once.
const SEND_CONCURRENCY: usize = 4;
const SLOT_WAIT: Duration = Duration::from_secs(5);
const SEND_TIMEOUT: Duration = Duration::from_secs(30);
/// Where a reset link points in the PWA (the same page as the mailed one).
const RESET_PATH: &str = "/auth/password/reset";

/// Absent from `AppState` when no bot token is configured (no Telegram reset messages).
#[derive(Clone)]
pub struct TelegramService {
    bot: Arc<dyn TelegramBot>,
    /// PWA origin without trailing slash (`APP_BASE_URL`).
    base_url: String,
    permits: Arc<Semaphore>,
}

impl TelegramService {
    pub fn new(bot: Arc<dyn TelegramBot>, base_url: String) -> Self {
        Self {
            bot,
            base_url,
            permits: Arc::new(Semaphore::new(SEND_CONCURRENCY)),
        }
    }

    /// Sends the reset link to `chat_id` in its own task (failures logged); waits only for a slot.
    /// `token` goes into the URL fragment, like the mailed link.
    pub async fn send_reset(&self, chat_id: i64, lang: Lang, token: &str, username: &str) {
        let link = format!("{}{RESET_PATH}#token={token}", self.base_url);
        let text = bot::reset_text(lang, username, &link);
        let permit =
            match tokio::time::timeout(SLOT_WAIT, self.permits.clone().acquire_owned()).await {
                Ok(Ok(permit)) => permit,
                Ok(Err(_)) => return, // semaphore is never closed
                Err(_) => {
                    tracing::warn!("all Telegram slots busy, message dropped");
                    return;
                }
            };
        let bot = self.bot.clone();
        tokio::spawn(async move {
            match tokio::time::timeout(SEND_TIMEOUT, bot.send_message(chat_id, text)).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => tracing::error!(error = %e, "sending Telegram message failed"),
                Err(_) => tracing::error!("sending Telegram message timed out"),
            }
            drop(permit);
        });
    }

    /// No message in flight (tests wait for this before reading the outbox).
    pub fn idle(&self) -> bool {
        self.permits.available_permits() == SEND_CONCURRENCY
    }
}
