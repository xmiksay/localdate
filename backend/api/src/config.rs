use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::auth::oauth::{self, OidcConfig};

const MIN_JWT_SECRET_BYTES: usize = 32;
const DEFAULT_CLEANUP_INTERVAL_SECS: u64 = 300;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub photo_dir: PathBuf,
    pub bind_addr: SocketAddr,
    /// Per-IP limiter on register/login. Always on in `from_env`; the test harness turns it off
    /// (every test client would share one bucket) except in the rate-limit tests.
    pub rate_limit: bool,
    /// Period of the background cleanup job (`CLEANUP_INTERVAL_SECS`).
    pub cleanup_interval: Duration,
    /// Rate-limit on `X-Forwarded-For` instead of the peer (`TRUST_PROXY_HEADERS`); only safe
    /// behind a proxy that overwrites the header.
    pub trust_proxy_headers: bool,
    /// How often an open WebSocket re-reads its account; a ban or deletion missed by the
    /// cross-replica bridge still closes it. Fixed in `from_env`; tests shorten it.
    pub ws_account_recheck: Duration,
    /// Magic-link email; `None` disables every email feature (`503 email_disabled`).
    pub email: Option<EmailConfig>,
    /// How often the server pings each WebSocket, and how long a socket may stay silent (no pong,
    /// no frame at all) before it is closed. A half-open socket would otherwise keep its user
    /// "online" and so suppress Web Push. Fixed in `from_env`; tests shorten them.
    pub ws_ping_every: Duration,
    pub ws_idle_timeout: Duration,
    /// Web Push identity; `None` (no VAPID keys set) turns push off.
    pub vapid: Option<VapidConfig>,
    /// Configured OAuth providers (Google, Telegram); one without client credentials is absent = disabled.
    pub oauth: Vec<OidcConfig>,
    /// `APP_BASE_URL`: the PWA origin, no trailing slash. Email and OAuth redirect URIs derive from
    /// it; its scheme decides whether cookies get `Secure`.
    pub app_base_url: Option<String>,
    /// Bot that sends password-reset links by Telegram; `None` (no token) = no such messages.
    pub telegram_bot: Option<TelegramBotConfig>,
}

/// Raw `VAPID_*` values; `push::vapid::Vapid::from_config` checks that they fit together.
#[derive(Clone)]
pub struct VapidConfig {
    pub public_key: String,
    pub private_key: String,
    pub subject: String,
}

/// `Config` is `Debug`; the private key must never reach a log line through it.
impl std::fmt::Debug for VapidConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VapidConfig")
            .field("public_key", &self.public_key)
            .field("private_key", &"<redacted>")
            .field("subject", &self.subject)
            .finish()
    }
}

pub const WS_ACCOUNT_RECHECK: Duration = Duration::from_secs(60);
pub const WS_PING_EVERY: Duration = Duration::from_secs(25);
pub const WS_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailConfig {
    /// Origin of the PWA (`APP_BASE_URL`), no trailing slash; links point at its routes.
    pub base_url: String,
    /// `From:` header (`EMAIL_FROM`).
    pub from: String,
    pub transport: EmailTransport,
}

#[derive(Clone, PartialEq, Eq)]
pub enum EmailTransport {
    /// lettre SMTP URL with credentials (`SMTP_URL`).
    Smtp(String),
    /// `EMAIL_DEV_LOG=true`: log the mail (with its link) at INFO instead of sending. Debug builds only.
    DevLog,
}

// Hand-written so the SMTP password never ends up in a `{:?}` of the config.
impl std::fmt::Debug for EmailTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Smtp(_) => "Smtp(..)",
            Self::DevLog => "DevLog",
        })
    }
}

mod telegram;

pub use telegram::TelegramBotConfig;

const DEV_LOG_FROM: &str = "localdate <noreply@localhost>";

impl Config {
    pub fn from_env() -> Result<Self> {
        let var = |name: &str| std::env::var(name).with_context(|| format!("{name} is required"));
        let jwt_secret = var("JWT_SECRET")?;
        if jwt_secret.len() < MIN_JWT_SECRET_BYTES {
            bail!("JWT_SECRET must be at least {MIN_JWT_SECRET_BYTES} bytes");
        }
        let app_base_url = app_base_url(std::env::var("APP_BASE_URL").ok())?;
        let config = Self {
            database_url: var("DATABASE_URL")?,
            jwt_secret,
            photo_dir: var("PHOTO_DIR")?.into(),
            bind_addr: var("BIND_ADDR")?
                .parse()
                .context("BIND_ADDR must be host:port")?,
            rate_limit: true,
            cleanup_interval: cleanup_interval(std::env::var("CLEANUP_INTERVAL_SECS").ok())?,
            trust_proxy_headers: flag(std::env::var("TRUST_PROXY_HEADERS").ok())
                .context("TRUST_PROXY_HEADERS must be true or false")?,
            ws_account_recheck: WS_ACCOUNT_RECHECK,
            email: email_config(
                non_empty(std::env::var("SMTP_URL").ok()),
                flag(std::env::var("EMAIL_DEV_LOG").ok())
                    .context("EMAIL_DEV_LOG must be true or false")?,
                non_empty(std::env::var("EMAIL_FROM").ok()),
                app_base_url.clone(),
                cfg!(debug_assertions),
            )?,
            ws_ping_every: WS_PING_EVERY,
            ws_idle_timeout: WS_IDLE_TIMEOUT,
            vapid: vapid(
                std::env::var("VAPID_PUBLIC_KEY").ok(),
                std::env::var("VAPID_PRIVATE_KEY").ok(),
                std::env::var("VAPID_SUBJECT").ok(),
            )?,
            oauth: oauth::config::from_env(
                app_base_url.as_deref(),
                oauth::config::Credentials {
                    google_id: std::env::var("GOOGLE_CLIENT_ID").ok(),
                    google_secret: std::env::var("GOOGLE_CLIENT_SECRET").ok(),
                    telegram_id: std::env::var("TELEGRAM_CLIENT_ID").ok(),
                    telegram_secret: std::env::var("TELEGRAM_CLIENT_SECRET").ok(),
                },
            )?,
            telegram_bot: telegram::telegram_bot(
                std::env::var("TELEGRAM_BOT_TOKEN").ok(),
                app_base_url.as_deref(),
            )?,
            app_base_url,
        };
        let telegram_login = config
            .oauth
            .iter()
            .find(|o| o.provider == oauth::Provider::Telegram);
        telegram::same_bot(
            telegram_login.map(|o| o.client_id.as_str()),
            config.telegram_bot.as_ref(),
        )?;
        Ok(config)
    }
}

/// `APP_BASE_URL`: http(s) origin of the PWA, trailing slash dropped; unset or empty = `None`.
fn app_base_url(raw: Option<String>) -> Result<Option<String>> {
    let Some(base) = non_empty(raw) else {
        return Ok(None);
    };
    if !(base.starts_with("https://") || base.starts_with("http://")) {
        bail!("APP_BASE_URL must start with http:// or https://");
    }
    Ok(Some(base.trim_end_matches('/').to_owned()))
}

fn cleanup_interval(raw: Option<String>) -> Result<Duration> {
    let secs = match raw {
        None => DEFAULT_CLEANUP_INTERVAL_SECS,
        Some(s) => s
            .trim()
            .parse::<u64>()
            .context("CLEANUP_INTERVAL_SECS must be a whole number of seconds")?,
    };
    if secs == 0 {
        bail!("CLEANUP_INTERVAL_SECS must be greater than 0");
    }
    Ok(Duration::from_secs(secs))
}

fn non_empty(raw: Option<String>) -> Option<String> {
    raw.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

/// No SMTP and no dev log → email off. Dev log prints live login links, so a release binary
/// refuses it rather than leaking them into production logs.
fn email_config(
    smtp_url: Option<String>,
    dev_log: bool,
    from: Option<String>,
    base_url: Option<String>,
    debug_build: bool,
) -> Result<Option<EmailConfig>> {
    let transport = match (smtp_url, dev_log) {
        (None, false) => return Ok(None),
        (Some(_), true) => bail!("set either SMTP_URL or EMAIL_DEV_LOG, not both"),
        (Some(url), false) => EmailTransport::Smtp(url),
        (None, true) if !debug_build => bail!("EMAIL_DEV_LOG is refused in release builds"),
        (None, true) => EmailTransport::DevLog,
    };
    let from = match (from, &transport) {
        (Some(from), _) => from,
        (None, EmailTransport::DevLog) => DEV_LOG_FROM.to_owned(),
        (None, EmailTransport::Smtp(_)) => bail!("EMAIL_FROM is required with SMTP_URL"),
    };
    let base_url = base_url.context("APP_BASE_URL is required when email is enabled")?;
    if !(base_url.starts_with("https://") || base_url.starts_with("http://")) {
        bail!("APP_BASE_URL must start with http:// or https://");
    }
    Ok(Some(EmailConfig {
        base_url: base_url.trim_end_matches('/').to_owned(),
        from,
        transport,
    }))
}

/// Both keys or neither (empty counts as unset, as an optional k8s Secret key leaves it); with keys
/// the subject is required.
fn vapid(
    public_key: Option<String>,
    private_key: Option<String>,
    subject: Option<String>,
) -> Result<Option<VapidConfig>> {
    match (non_empty(public_key), non_empty(private_key)) {
        (None, None) => Ok(None),
        (Some(public_key), Some(private_key)) => Ok(Some(VapidConfig {
            public_key,
            private_key,
            subject: non_empty(subject).context("VAPID_SUBJECT is required with VAPID keys")?,
        })),
        _ => bail!("set both VAPID_PUBLIC_KEY and VAPID_PRIVATE_KEY, or neither"),
    }
}

/// Unset means false; anything but a recognised boolean is an error rather than a silent false.
fn flag(raw: Option<String>) -> Result<bool> {
    let Some(raw) = raw else { return Ok(false) };
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        other => bail!("not a boolean: {other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_interval_defaults_parses_and_rejects_bad_values() {
        assert_eq!(cleanup_interval(None).ok(), Some(Duration::from_secs(300)));
        assert_eq!(
            cleanup_interval(Some(" 60 ".into())).ok(),
            Some(Duration::from_secs(60))
        );
        for bad in ["0", "-5", "5m", ""] {
            assert!(cleanup_interval(Some(bad.into())).is_err(), "{bad}");
        }
    }

    fn some(s: &str) -> Option<String> {
        Some(s.to_owned())
    }

    #[test]
    fn email_is_off_without_smtp_or_dev_log() {
        let cfg = email_config(None, false, some("x@y.cz"), some("https://a.cz"), true);
        assert_eq!(cfg.ok(), Some(None));
    }

    #[test]
    fn email_smtp_needs_from_and_base_url() {
        let smtp = || some("smtps://u:p@host:465");
        let ok = email_config(
            smtp(),
            false,
            some("a <a@b.cz>"),
            some("https://a.cz/"),
            false,
        )
        .expect("valid")
        .expect("enabled");
        assert_eq!(ok.base_url, "https://a.cz");
        assert_eq!(
            ok.transport,
            EmailTransport::Smtp("smtps://u:p@host:465".into())
        );
        assert!(
            !format!("{ok:?}").contains("u:p"),
            "password leaked into Debug"
        );
        assert!(email_config(smtp(), false, None, some("https://a.cz"), false).is_err());
        assert!(email_config(smtp(), false, some("a@b.cz"), None, false).is_err());
        assert!(email_config(smtp(), false, some("a@b.cz"), some("a.cz"), false).is_err());
        assert!(email_config(smtp(), true, some("a@b.cz"), some("https://a.cz"), true).is_err());
    }

    #[test]
    fn email_dev_log_only_in_debug_builds() {
        let base = || some("http://localhost:5173");
        let dev = email_config(None, true, None, base(), true)
            .expect("valid")
            .expect("enabled");
        assert_eq!(dev.transport, EmailTransport::DevLog);
        assert_eq!(dev.from, DEV_LOG_FROM);
        assert!(email_config(None, true, None, base(), false).is_err());
    }

    #[test]
    fn app_base_url_is_optional_http_and_loses_its_trailing_slash() {
        assert_eq!(app_base_url(None).ok(), Some(None));
        assert_eq!(app_base_url(some(" ")).ok(), Some(None));
        assert_eq!(
            app_base_url(some("https://a.cz/")).ok(),
            Some(some("https://a.cz"))
        );
        assert!(app_base_url(some("a.cz")).is_err());
    }

    #[test]
    fn flag_defaults_false_and_rejects_garbage() {
        assert_eq!(flag(None).ok(), Some(false));
        for t in ["true", " TRUE ", "1"] {
            assert_eq!(flag(Some(t.into())).ok(), Some(true), "{t}");
        }
        for f in ["false", "False", "0"] {
            assert_eq!(flag(Some(f.into())).ok(), Some(false), "{f}");
        }
        for bad in ["", "yes", "on", "2"] {
            assert!(flag(Some(bad.into())).is_err(), "{bad}");
        }
    }

    #[test]
    fn vapid_needs_both_keys_and_a_subject() {
        let some = |s: &str| Some(s.to_owned());
        assert!(vapid(None, None, None).expect("off").is_none());
        assert!(
            vapid(some(""), some(" "), some("mailto:x@y"))
                .expect("off")
                .is_none()
        );
        let on = vapid(some("pub"), some("priv"), some("mailto:x@y")).expect("on");
        assert_eq!(on.map(|v| v.subject), some("mailto:x@y"));
        assert!(vapid(some("pub"), None, some("mailto:x@y")).is_err());
        assert!(vapid(None, some("priv"), None).is_err());
        assert!(vapid(some("pub"), some("priv"), None).is_err());
    }

    #[test]
    fn vapid_debug_never_shows_the_private_key() {
        let config = VapidConfig {
            public_key: "public-part".into(),
            private_key: "very-secret-part".into(),
            subject: "mailto:x@y".into(),
        };
        let shown = format!("{config:?}");
        assert!(shown.contains("public-part") && shown.contains("<redacted>"));
        assert!(!shown.contains("very-secret-part"));
    }
}
