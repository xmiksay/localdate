//! Telegram fixtures: login through the fake OpenID provider (subject = numeric `id` claim) and
//! the bot's outbox for password-reset messages.

use axum::http::StatusCode;
use localdate_api::auth::oauth::Provider;
use localdate_api::config::TelegramBotConfig;
use serde_json::json;

use super::email::BASE_URL;
use super::oauth::{FakeProvider, IdClaims, Raw, Started};
use super::{TestApp, Tokens, tokens_from};

const TG: Provider = Provider::Telegram;

/// Telegram's ID token for user `id`: `sub` is a different, opaque value, as Telegram sends it.
pub fn claims(started: &Started, id: i64) -> IdClaims {
    IdClaims {
        id: Some(id),
        ..FakeProvider::claims(started, &format!("opaque-sub-{id}"))
    }
}

impl TestApp {
    /// Telegram login pointed at a fake provider, plus the reset bot (`MemoryBot`) unless `bot` is
    /// false; `email` also turns on the in-memory mailer.
    pub async fn with_telegram(email: bool, bot: bool) -> (Self, FakeProvider) {
        Self::with_oauth(TG, email, |c| {
            c.telegram_bot = bot.then(|| TelegramBotConfig {
                bot_token: "424242:test-bot-token".into(),
                base_url: BASE_URL.into(),
            });
        })
        .await
    }

    /// Start → consent as Telegram user `id` → callback.
    pub async fn telegram_return(&self, fake: &FakeProvider, id: i64) -> (Started, Raw) {
        let started = self.oauth_start_as(TG, None).await;
        let claims = claims(&started, id);
        let resp = self.oauth_return_as(TG, fake, &started, claims).await;
        (started, resp)
    }

    /// Logs in (or starts a sign-up) as Telegram user `id`: the `exchange` answer.
    pub async fn telegram_exchange(&self, fake: &FakeProvider, id: i64) -> Raw {
        let (started, resp) = self.telegram_return(fake, id).await;
        let fragment = resp.fragment();
        let code = fragment
            .get("code")
            .unwrap_or_else(|| panic!("no code: {fragment:?}"));
        self.oauth_exchange(Some(&started.cookie), code).await
    }

    /// New account `username` created through Telegram user `id`.
    pub async fn telegram_signup(&self, fake: &FakeProvider, id: i64, username: &str) -> Tokens {
        let resp = self.telegram_exchange(fake, id).await;
        assert_eq!(resp.status, StatusCode::OK, "{}", resp.body);
        assert_eq!(resp.body["signup"]["provider"], "telegram");
        let token = resp.body["signup"]["token"].as_str().expect("signup token");
        let (status, body) = self
            .post(
                "/api/auth/oauth/signup",
                json!({ "token": token, "username": username }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        tokens_from(&body)
    }

    /// Links Telegram user `id` to `t`'s account; the done page's fragment.
    pub async fn telegram_link(
        &self,
        fake: &FakeProvider,
        t: &Tokens,
        id: i64,
    ) -> std::collections::HashMap<String, String> {
        let started = self.oauth_link_as(TG, &t.access_token).await;
        let claims = claims(&started, id);
        self.oauth_return_as(TG, fake, &started, claims)
            .await
            .fragment()
    }

    /// Bot messages to chat `id`, oldest first, once every send has finished.
    pub async fn bot_messages_to(&self, id: i64) -> Vec<String> {
        self.settle_messages().await;
        self.bot
            .sent()
            .into_iter()
            .filter(|(chat, _)| *chat == id)
            .map(|(_, text)| text)
            .collect()
    }
}
