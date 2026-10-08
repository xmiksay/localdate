//! `TELEGRAM_BOT_TOKEN`: the bot that sends password-reset links (login is `TELEGRAM_CLIENT_*`).

use anyhow::{Context, Result, bail};

#[derive(Clone, PartialEq, Eq)]
pub struct TelegramBotConfig {
    /// From BotFather; authorizes `sendMessage`.
    pub bot_token: String,
    /// `APP_BASE_URL`; reset links sent by the bot point at it.
    pub base_url: String,
}

/// `Config` is `Debug`; the bot token must never reach a log line through it.
impl std::fmt::Debug for TelegramBotConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelegramBotConfig")
            .field("bot_token", &"<redacted>")
            .field("base_url", &self.base_url)
            .finish()
    }
}

/// Unset (or empty) = no Telegram reset messages. The token's shape (`<bot id>:<secret>`) is
/// checked so a pasted client secret or a truncated value fails at startup, not at the first reset.
pub(super) fn telegram_bot(
    token: Option<String>,
    base_url: Option<&str>,
) -> Result<Option<TelegramBotConfig>> {
    let Some(bot_token) = token.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    let well_formed = bot_token.split_once(':').is_some_and(|(id, secret)| {
        !id.is_empty()
            && id.bytes().all(|b| b.is_ascii_digit())
            && !secret.is_empty()
            && secret
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    });
    if !well_formed {
        bail!("TELEGRAM_BOT_TOKEN must look like <digits>:<secret> (as BotFather gives it)");
    }
    let base_url = base_url.context("APP_BASE_URL is required with TELEGRAM_BOT_TOKEN")?;
    Ok(Some(TelegramBotConfig {
        bot_token,
        base_url: base_url.to_owned(),
    }))
}

/// With both Telegram login and the bot configured, they must be the same bot: reset messages go
/// to users who allowed *that* bot (`telegram:bot_access`) to message them. The bot id is the
/// token's numeric prefix, and Telegram's OIDC client id is the bot id.
pub(super) fn same_bot(
    login_client_id: Option<&str>,
    bot: Option<&TelegramBotConfig>,
) -> Result<()> {
    let (Some(client_id), Some(bot)) = (login_client_id, bot) else {
        return Ok(());
    };
    let bot_id = bot.bot_token.split(':').next().unwrap_or_default();
    if bot_id != client_id.trim() {
        bail!(
            "TELEGRAM_BOT_TOKEN belongs to bot {bot_id}, but TELEGRAM_CLIENT_ID is {client_id}: \
             both must be the same bot (its Login Widget client id is the bot id)"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn some(s: &str) -> Option<String> {
        Some(s.to_owned())
    }

    #[test]
    fn login_and_reset_bot_must_be_the_same_bot() {
        let bot = telegram_bot(some("123:abc"), Some("https://a.cz"))
            .expect("valid")
            .expect("on");
        assert!(same_bot(Some("123"), Some(&bot)).is_ok());
        assert!(same_bot(None, Some(&bot)).is_ok(), "bot without login");
        assert!(same_bot(Some("999"), None).is_ok(), "login without bot");
        let err = same_bot(Some("999"), Some(&bot)).expect_err("different bots");
        assert!(err.to_string().contains("same bot"));
        assert!(
            !format!("{err:?}").contains("abc"),
            "the token secret is not in the message"
        );
    }

    #[test]
    fn bot_token_is_optional_but_must_be_well_formed() {
        let base = Some("https://a.cz");
        assert_eq!(telegram_bot(None, base).ok(), Some(None));
        assert_eq!(telegram_bot(some("  "), None).ok(), Some(None));
        let on = telegram_bot(some("123:AbC_-9"), base)
            .expect("valid")
            .expect("enabled");
        assert_eq!(on.base_url, "https://a.cz");
        let shown = format!("{on:?}");
        assert!(!shown.contains("AbC_-9") && shown.contains("<redacted>"));
        for bad in ["abc:123", "123", "123:", "123:a b", ":abc"] {
            assert!(telegram_bot(some(bad), base).is_err(), "{bad}");
        }
        assert!(
            telegram_bot(some("123:abc"), None).is_err(),
            "needs APP_BASE_URL"
        );
    }
}
