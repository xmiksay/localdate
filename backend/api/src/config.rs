use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};

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
}

pub const WS_ACCOUNT_RECHECK: Duration = Duration::from_secs(60);

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

const DEV_LOG_FROM: &str = "localdate <noreply@localhost>";

impl Config {
    pub fn from_env() -> Result<Self> {
        let var = |name: &str| std::env::var(name).with_context(|| format!("{name} is required"));
        let jwt_secret = var("JWT_SECRET")?;
        if jwt_secret.len() < MIN_JWT_SECRET_BYTES {
            bail!("JWT_SECRET must be at least {MIN_JWT_SECRET_BYTES} bytes");
        }
        Ok(Self {
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
                non_empty(std::env::var("APP_BASE_URL").ok()),
                cfg!(debug_assertions),
            )?,
        })
    }
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
}
