//! Messages from the bot: Bot API `sendMessage` in production, an in-memory outbox in tests.

use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use serde_json::json;

use crate::auth::email::message::Lang;
use crate::mail::SendFuture;

/// Boxed future instead of `async fn` so the bot can live in `AppState` as `dyn TelegramBot`.
pub trait TelegramBot: Send + Sync {
    /// Sends `text` to the private chat with user `chat_id` (the user allowed it at login/link).
    fn send_message(&self, chat_id: i64, text: String) -> SendFuture<'_>;
}

const API: &str = "https://api.telegram.org";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// The Bot API over HTTPS. Its URL contains the bot token, so this type has no `Debug` and every
/// error is stripped of its URL before it can reach a log line.
pub struct HttpBot {
    client: reqwest::Client,
    send_url: String,
}

impl HttpBot {
    pub fn new(bot_token: &str) -> Result<Self> {
        let client = reqwest::Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(REQUEST_TIMEOUT)
            .build()
            .context("building the Telegram HTTP client")?;
        Ok(Self {
            client,
            send_url: format!("{API}/bot{bot_token}/sendMessage"),
        })
    }
}

impl TelegramBot for HttpBot {
    fn send_message(&self, chat_id: i64, text: String) -> SendFuture<'_> {
        Box::pin(async move {
            let body = json!({
                "chat_id": chat_id,
                "text": text,
                "link_preview_options": { "is_disabled": true },
            });
            let response = self
                .client
                .post(&self.send_url)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.to_string())
                .send()
                .await
                .map_err(|e| anyhow!("Telegram request failed: {}", e.without_url()))?;
            let status = response.status();
            if status.is_success() {
                return Ok(());
            }
            // `description` is Telegram's own error text (e.g. "bot was blocked by the user").
            let description = response
                .bytes()
                .await
                .ok()
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                .and_then(|v| v["description"].as_str().map(str::to_owned))
                .unwrap_or_default();
            Err(anyhow!("Telegram answered {status}: {description}"))
        })
    }
}

/// Captures every message; integration tests read the links out of it.
#[derive(Default)]
pub struct MemoryBot {
    sent: Mutex<Vec<(i64, String)>>,
}

impl MemoryBot {
    pub fn sent(&self) -> Vec<(i64, String)> {
        self.sent.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl TelegramBot for MemoryBot {
    fn send_message(&self, chat_id: i64, text: String) -> SendFuture<'_> {
        self.sent
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((chat_id, text));
        Box::pin(async { Ok(()) })
    }
}

/// The password-reset message; `link` carries the token in its fragment.
pub fn reset_text(lang: Lang, username: &str, link: &str) -> String {
    match lang {
        Lang::Cs => format!(
            "localdate: někdo požádal o nové heslo k účtu {username}. Nastavíš si ho tímto \
             odkazem:\n{link}\n\nOdkaz platí 15 minut a jde použít jen jednou. Pokud jsi o nic \
             nežádal(a), zprávu ignoruj."
        ),
        Lang::En => format!(
            "localdate: someone asked for a new password for the account {username}. Set one \
             with this link:\n{link}\n\nThe link is valid for 15 minutes and works once. If you \
             did not ask for it, ignore this message."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_text_names_the_account_and_carries_the_link() {
        for lang in [Lang::Cs, Lang::En] {
            let text = reset_text(lang, "eva", "https://a.cz/auth/password/reset#token=t");
            assert!(text.contains("eva"));
            assert!(text.contains("https://a.cz/auth/password/reset#token=t"));
        }
    }

    #[tokio::test]
    async fn memory_bot_captures() {
        let bot = MemoryBot::default();
        bot.send_message(7, "hi".into()).await.expect("send");
        assert_eq!(bot.sent(), vec![(7, "hi".to_owned())]);
    }

    #[tokio::test]
    async fn http_errors_never_show_the_token() {
        let bot = HttpBot::new("123:SECRET-token").expect("client");
        // http:// is refused by https_only before any connection is made.
        let bot = HttpBot {
            send_url: bot
                .send_url
                .replace("https://api.telegram.org", "http://127.0.0.1:9"),
            ..bot
        };
        let err = bot.send_message(1, "x".into()).await.expect_err("refused");
        assert!(!format!("{err:?}").contains("SECRET"), "{err:?}");
    }
}
