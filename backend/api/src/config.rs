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
}

pub const WS_ACCOUNT_RECHECK: Duration = Duration::from_secs(60);

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
