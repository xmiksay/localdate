use std::io::IsTerminal;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use localdate_api::auth::email::EmailService;
use localdate_api::auth::telegram::TelegramService;
use localdate_api::auth::telegram::bot::HttpBot;
use localdate_api::config::{Config, PhotoStorage};
use localdate_api::media::PhotoStore;
use localdate_api::retry::patiently;
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
    /// Admin role and test users (needs DATABASE_URL)
    Admin {
        #[command(subcommand)]
        action: AdminAction,
    },
    /// Web Push (VAPID) keys
    Vapid {
        #[command(subcommand)]
        action: VapidAction,
    },
}

#[derive(Subcommand)]
enum VapidAction {
    /// Print a fresh key pair as VAPID_PUBLIC_KEY / VAPID_PRIVATE_KEY lines
    Generate,
}

#[derive(Subcommand)]
enum AdminAction {
    /// Make <username> an admin
    Grant { username: String },
    /// Take the admin role away from <username>
    Revoke { username: String },
    /// Create onboarded, password-less test users with placeholder avatars
    /// (also needs the photo storage settings: PHOTO_STORAGE, PHOTO_DIR / S3_*)
    SeedTestUsers {
        #[arg(long, default_value_t = 10)]
        count: u32,
    },
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
        Some(Command::Vapid {
            action: VapidAction::Generate,
        }) => {
            let (public_key, private_key) = localdate_api::push::vapid::generate();
            println!("VAPID_PUBLIC_KEY={public_key}\nVAPID_PRIVATE_KEY={private_key}");
            Ok(())
        }
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
        AdminAction::SeedTestUsers { count } => {
            let store = PhotoStore::new(&PhotoStorage::from_env()?)?;
            let mut rng = localdate_api::admin::seed::Rng::from_entropy();
            let created =
                localdate_api::admin::seed::seed_test_users(&db, &store, *count, &mut rng).await?;
            println!("created {} test user(s):", created.len());
            for name in created {
                println!("  {name}");
            }
            return Ok(());
        }
    };
    localdate_api::admin::set_admin(&db, username, grant).await?;
    println!(
        "{} admin role {} {username}",
        if grant { "granted" } else { "revoked" },
        if grant { "to" } else { "from" },
    );
    Ok(())
}

/// How long shutdown waits for detached work (reset / notice delivery) after the last request.
const DRAIN_DETACHED: Duration = Duration::from_secs(10);

/// How long `serve` keeps starting new attempts to reach the DB and the photo storage; one DB
/// attempt itself can take up to the pool acquire timeout (30 s) when the host does not answer,
/// one storage attempt up to its retry timeout (15 s).
const CONNECT_PATIENCE: Duration = Duration::from_secs(60);

async fn connect_with_retry(url: &str) -> Result<DatabaseConnection> {
    patiently(
        "database",
        CONNECT_PATIENCE,
        |_| true,
        || Database::connect(url),
    )
    .await
    .context("connecting to database")
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
    let telegram = match &config.telegram_bot {
        Some(cfg) => Some(TelegramService::new(
            Arc::new(HttpBot::new(&cfg.bot_token)?),
            cfg.base_url.clone(),
        )),
        None => {
            tracing::info!("no Telegram bot (no TELEGRAM_BOT_TOKEN): reset links go by email only");
            None
        }
    };
    let bind_addr = config.bind_addr;
    let state = AppState::new(db, config)
        .await?
        .with_email(email)
        .with_telegram(telegram);
    // Before binding, so a pod with bad S3 settings never turns ready: the rollout fails instead
    // of the first upload. Refusals (403, missing bucket) fail at once; only outages are waited out.
    patiently(
        "photo storage",
        CONNECT_PATIENCE,
        PhotoStore::is_transient,
        || state.photos.check(),
    )
    .await
    .context("checking photo storage (needs write access to the bucket)")?;

    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .with_context(|| format!("binding {bind_addr}"))?;
    tracing::info!(addr = %bind_addr, "listening");
    let hub = state.hub.clone();
    let detached = state.detached.clone();
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
    .context("serving")?;
    // Reset links and link notices are sent after their response: let them finish.
    let abandoned = detached.drain(DRAIN_DETACHED).await;
    if abandoned > 0 {
        tracing::warn!(
            abandoned,
            "shutdown: background sends still running were abandoned"
        );
    }
    Ok(())
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
