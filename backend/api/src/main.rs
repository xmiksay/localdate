use std::net::SocketAddr;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use localdate_api::config::Config;
use localdate_api::state::AppState;
use localdate_api::web::Dist;
use migration::{Migrator, MigratorTrait};
use sea_orm::Database;
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

async fn serve() -> Result<()> {
    if Dist::get("index.html").is_none() {
        tracing::warn!(
            "frontend bundle not found (build it with `make build`); non-API paths will 404"
        );
    }

    let config = Config::from_env()?;
    let db = Database::connect(&config.database_url)
        .await
        .context("connecting to database")?;
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
    let app = localdate_api::app(AppState::new(db, config));
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
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
