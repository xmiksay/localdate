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
    /// Per-IP limiter on register/login; tests disable it (oneshot has no ConnectInfo).
    pub rate_limit: bool,
    /// Period of the background cleanup job (`CLEANUP_INTERVAL_SECS`).
    pub cleanup_interval: Duration,
}

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
}
