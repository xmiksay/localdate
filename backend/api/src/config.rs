use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};

const MIN_JWT_SECRET_BYTES: usize = 32;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub photo_dir: PathBuf,
    pub bind_addr: SocketAddr,
    /// Per-IP limiter on register/login; tests disable it (oneshot has no ConnectInfo).
    pub rate_limit: bool,
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
        })
    }
}
