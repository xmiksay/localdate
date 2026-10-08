use std::io::IsTerminal;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use localdate_api::auth::email::EmailService;
use localdate_api::config::Config;
use localdate_api::state::AppState;
use localdate_api::web::Dist;
use migration::{Migrator, MigratorTrait};
use sea_orm::{Database, DatabaseConnection};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    version,
    about = "localdate API server; without a subcommand it serves"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Manage the admin role (needs only DATABASE_URL)
    Admin {
        #[command(subcommand)]
        action: AdminAction,
    },
}

#[derive(Subcommand)]
enum AdminAction {
    /// Make <username> an admin
    Grant { username: String },
    /// Take the admin role away from <username>
    Revoke { username: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        // Colour escapes only for a terminal; piped/container logs (`kubectl logs`) stay plain.
        .with_ansi(std::io::stdout().is_terminal())
        .init();
    match cli.command {
        None => serve().await,
        Some(Command::Admin { action }) => admin(action).await,
    }
}

async fn admin(action: AdminAction) -> Result<()> {
    let url = std::env::var("DATABASE_URL").context("DATABASE_URL is required")?;
    let db = Database::connect(&url)
        .await
        .context("connecting to database")?;
    // Schema changes stay with `make migrate` / server start; the CLI must not apply them.
    let pending = Migrator::get_pending_migrations(&db)
        .await
        .context("checking migrations")?;
    if !pending.is_empty() {
        bail!(
            "{} pending migration(s): run `make migrate` (or start the server) first",
            pending.len()
        );
    }
    let (username, grant) = match &action {
        AdminAction::Grant { username } => (username, true),
        AdminAction::Revoke { username } => (username, false),
    };
    localdate_api::admin::set_admin(&db, username, grant).await?;
    println!(
        "{} admin role {} {username}",
        if grant { "granted" } else { "revoked" },
        if grant { "to" } else { "from" },
    );
    Ok(())
}

/// How long `serve` keeps starting new DB connection attempts; one attempt itself can take up to the
/// pool acquire timeout (30 s) when the host does not answer.
const CONNECT_PATIENCE: Duration = Duration::from_secs(60);

/// On k8s the API and Postgres start together; waiting here beats crash-looping until the DB is up.
async fn connect_with_retry(url: &str) -> Result<DatabaseConnection> {
    let started = Instant::now();
    let mut attempt = 0;
    loop {
        match Database::connect(url).await {
            Ok(db) => return Ok(db),
            Err(e) if started.elapsed() < CONNECT_PATIENCE => {
                let delay = localdate_api::retry::backoff(attempt);
                tracing::warn!(error = %e, retry_in = ?delay, "database not reachable yet");
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
            Err(e) => return Err(e).context("connecting to database"),
        }
    }
}

async fn serve() -> Result<()> {
    if Dist::get("index.html").is_none() {
        tracing::warn!(
            "frontend bundle not found (build it with `make build`); non-API paths will 404"
        );
    }

    let config = Config::from_env()?;
    let db = connect_with_retry(&config.database_url).await?;
    Migrator::up(&db, None)
        .await
        .context("running migrations")?;
    tokio::fs::create_dir_all(&config.photo_dir)
        .await
        .context("creating PHOTO_DIR")?;

    let listener = tokio::net::TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("binding {}", config.bind_addr))?;
    tracing::info!(addr = %config.bind_addr, "listening");

    // Started here rather than in `app()` so integration tests drive `cleanup::run_once` directly.
    tokio::spawn(localdate_api::cleanup::run_forever(
        db.clone(),
        config.cleanup_interval,
    ));
    let email = match &config.email {
        Some(cfg) => Some(EmailService::new(
            localdate_api::mail::from_config(cfg)?,
            cfg.base_url.clone(),
        )),
        None => {
            tracing::info!("email disabled (no SMTP_URL / EMAIL_DEV_LOG)");
            None
        }
    };
    let state = AppState::new(db, config).await?.with_email(email);
    let hub = state.hub.clone();
    let app = localdate_api::app(state);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        shutdown_signal().await;
        // Before draining, so open sockets close (1001) instead of holding the shutdown up.
        hub.shutdown().await;
    })
    .await
    .context("serving")
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!(error = %e, "ctrl-c handler failed");
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(e) => {
                tracing::error!(error = %e, "SIGTERM handler failed");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
    tracing::info!("shutting down");
}
