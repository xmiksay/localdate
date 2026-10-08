//! Cross-replica hub: local delivery through [`LocalHub`], fan-out to the other replicas through
//! Postgres LISTEN/NOTIFY, presence through `ws_presence` (docs/architecture.md "Realtime").

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use entity::{matches, message};
use sea_orm::sqlx::postgres::PgListener;
use sea_orm::{DatabaseConnection, EntityTrait};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;
use uuid::Uuid;

use super::envelope::{self, CHANNEL, Op};
use super::event::ServerEvent;
use super::hub::{CloseReason, LocalHub, Outbound};
use super::presence;
use super::publisher::{self, Outbox};
use crate::social::summaries;

/// How often each replica publishes a `ping` on the shared channel. Every listener receives it,
/// its own included, so a listener always has traffic while its connection works.
const PING_EVERY: Duration = Duration::from_secs(30);
/// Without any notification (not even our own ping) for this long, the listener connection is
/// presumed dead (a silently dropped TCP connection would otherwise never error).
const LISTENER_SILENCE: Duration = Duration::from_secs(75);
/// Upper bound on one heartbeat's presence SQL, so a hung query cannot stall the next beat.
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(10);

pub struct Hub {
    local: Arc<LocalHub>,
    replica_id: Uuid,
    db: DatabaseConnection,
    outbox: Outbox,
    stop_heartbeat: watch::Sender<bool>,
    heartbeat_task: Mutex<Option<JoinHandle<()>>>,
}

impl Hub {
    /// Registers a fresh replica id, starts listening on a connection of its own (outside the app
    /// pool) and spawns the publisher, listener, ping and heartbeat tasks. Needs a Tokio runtime.
    pub async fn start(db: DatabaseConnection, database_url: &str) -> Result<Arc<Self>> {
        let replica_id = Uuid::new_v4();
        let local = Arc::new(LocalHub::default());
        beat(&db, replica_id, &local).await?;
        let listener = listen(database_url).await?;
        let (outbox, queue) = Outbox::new(publisher::QUEUE_CAP);
        tokio::spawn(publisher::run(db.clone(), replica_id, queue));
        tokio::spawn(relay(
            listener,
            database_url.to_owned(),
            db.clone(),
            replica_id,
            local.clone(),
        ));
        tokio::spawn(keepalive(outbox.clone()));
        let (stop_heartbeat, stop) = watch::channel(false);
        let task = tokio::spawn(heartbeat(db.clone(), replica_id, local.clone(), stop));
        tracing::info!(%replica_id, "ws bridge started");
        Ok(Arc::new(Self {
            local,
            replica_id,
            db,
            outbox,
            stop_heartbeat,
            heartbeat_task: Mutex::new(Some(task)),
        }))
    }

    /// Marks this replica alive and reconciles its presence rows with the open sockets. Runs every
    /// [`presence::HEARTBEAT`]; public so tests can force a beat.
    pub async fn heartbeat(&self) -> Result<()> {
        beat(&self.db, self.replica_id, &self.local).await
    }

    /// Opens a local socket slot and records presence; returns the handle for
    /// [`Hub::unsubscribe`] and the socket's event stream.
    pub async fn subscribe(&self, user: Uuid) -> (i64, UnboundedReceiver<Outbound>) {
        let (id, rx) = self.local.subscribe(user);
        if let Err(e) = presence::insert(&self.db, self.replica_id, id, user).await {
            // The next heartbeat restores the row.
            tracing::warn!(error = format!("{e:#}"), "recording ws presence failed");
        }
        (id, rx)
    }

    pub async fn unsubscribe(&self, user: Uuid, id: i64) {
        self.local.unsubscribe(user, id);
        if let Err(e) = presence::remove(&self.db, self.replica_id, id).await {
            // The next heartbeat drops the row.
            tracing::warn!(error = format!("{e:#}"), "clearing ws presence failed");
        }
    }

    /// Delivers to every socket of `user` on every replica.
    pub fn send(&self, user: Uuid, event: &ServerEvent) {
        self.local.send(user, event);
        self.outbox.push(Op::Event {
            user,
            event: event.clone(),
        });
    }

    /// Closes every socket of `user` on every replica.
    pub fn disconnect(&self, user: Uuid, reason: CloseReason) {
        self.local.disconnect(user, reason);
        self.outbox.push(Op::Close { user, reason });
    }

    /// Whether `user` has an open socket on any live replica.
    pub async fn is_online(&self, user: Uuid) -> Result<bool> {
        presence::is_online(&self.db, user).await
    }

    /// Graceful shutdown: closes local sockets (1001), stops the heartbeat so it cannot re-register
    /// this replica, then drops the replica row and with it all its presence.
    pub async fn shutdown(&self) {
        self.local.disconnect_all(CloseReason::GoingAway);
        let _ = self.stop_heartbeat.send(true);
        let task = self
            .heartbeat_task
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(task) = task {
            let _ = task.await;
        }
        if let Err(e) = presence::deregister(&self.db, self.replica_id).await {
            tracing::warn!(error = format!("{e:#}"), "deregistering ws replica failed");
        }
    }
}

async fn beat(db: &DatabaseConnection, replica_id: Uuid, local: &LocalHub) -> Result<()> {
    let (sockets, taken_at) = local.snapshot();
    tokio::time::timeout(
        HEARTBEAT_TIMEOUT,
        presence::heartbeat(db, replica_id, &sockets, taken_at),
    )
    .await
    .context("ws heartbeat timed out")?
}

async fn heartbeat(
    db: DatabaseConnection,
    replica_id: Uuid,
    local: Arc<LocalHub>,
    mut stop: watch::Receiver<bool>,
) {
    let mut ticker = tokio::time::interval(presence::HEARTBEAT);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = ticker.tick() => {}
            _ = stop.changed() => return,
        }
        if let Err(e) = beat(&db, replica_id, &local).await {
            tracing::warn!(error = format!("{e:#}"), "ws replica heartbeat failed");
        }
    }
}

async fn keepalive(outbox: Outbox) {
    let mut ticker = tokio::time::interval(PING_EVERY);
    loop {
        ticker.tick().await;
        outbox.push(Op::Ping);
    }
}

async fn listen(database_url: &str) -> Result<PgListener> {
    let mut listener = PgListener::connect(database_url)
        .await
        .context("ws listener connect")?;
    // A reconnect inside sqlx would hide the gap; `relay` reconnects itself and resyncs.
    listener.eager_reconnect(false);
    listener
        .listen(CHANNEL)
        .await
        .context("ws listener LISTEN")?;
    Ok(listener)
}

/// Feeds notifications into the local hub, reconnecting with backoff whenever the listener dies.
/// After a gap, local sockets are closed with `Resync` so clients reconnect and refetch.
async fn relay(
    mut listener: PgListener,
    database_url: String,
    db: DatabaseConnection,
    replica_id: Uuid,
    local: Arc<LocalHub>,
) {
    loop {
        let lost = pump(&mut listener, &db, replica_id, &local).await;
        tracing::warn!(
            error = format!("{lost:#}"),
            "ws listener lost; reconnecting"
        );
        let mut attempt = 0;
        listener = loop {
            match listen(&database_url).await {
                Ok(l) => break l,
                Err(e) => {
                    let delay = crate::retry::backoff(attempt);
                    tracing::warn!(error = format!("{e:#}"), retry_in = ?delay, "ws listener reconnect failed");
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                }
            }
        };
        tracing::info!("ws listener reconnected; closing local sockets so clients resync");
        local.disconnect_all(CloseReason::Resync);
    }
}

/// Delivers notifications until the connection is lost; returns why.
async fn pump(
    listener: &mut PgListener,
    db: &DatabaseConnection,
    replica_id: Uuid,
    local: &Arc<LocalHub>,
) -> anyhow::Error {
    loop {
        let notification = match tokio::time::timeout(LISTENER_SILENCE, listener.try_recv()).await {
            Err(_) => return anyhow!("no notification for {LISTENER_SILENCE:?}"),
            Ok(Err(e)) => return anyhow!(e),
            Ok(Ok(None)) => return anyhow!("connection closed"),
            Ok(Ok(Some(n))) => n,
        };
        let env = match envelope::decode(notification.payload()) {
            Ok(env) => env,
            Err(e) => {
                tracing::warn!(error = format!("{e:#}"), "ignoring ws notification");
                continue;
            }
        };
        if env.origin == replica_id {
            continue;
        }
        match env.op {
            Op::Event { user, event } => local.send(user, &event),
            Op::Close { user, reason } => local.disconnect(user, reason),
            // Loading rows must not hold up the notifications behind it.
            op @ (Op::MessageRef { user, .. } | Op::MatchRef { user, .. })
                if local.is_connected(user) =>
            {
                let (db, local) = (db.clone(), local.clone());
                tokio::spawn(async move {
                    if let Err(e) = load_ref(&db, &local, op).await {
                        tracing::warn!(
                            error = format!("{e:#}"),
                            "loading referenced ws event failed"
                        );
                    }
                });
            }
            Op::MessageRef { .. } | Op::MatchRef { .. } | Op::Ping => {}
        }
    }
}

async fn load_ref(db: &DatabaseConnection, local: &LocalHub, op: Op) -> Result<()> {
    match op {
        Op::MessageRef { user, id } => {
            let row = message::Entity::find_by_id(id)
                .one(db)
                .await
                .context("loading referenced message")?;
            // Gone means deleted with its account meanwhile; nothing to show.
            if let Some(row) = row {
                let message = row.into();
                local.send(user, &ServerEvent::Message { message });
            }
        }
        Op::MatchRef { user, id } => {
            let Some(m) = matches::Entity::find_by_id(id)
                .one(db)
                .await
                .context("loading referenced match")?
            else {
                return Ok(());
            };
            let summary = summaries(db, user, std::slice::from_ref(&m))
                .await
                .context("rebuilding referenced match")?
                .pop();
            if let Some(summary) = summary {
                local.send(user, &ServerEvent::Match { summary });
            }
        }
        Op::Event { .. } | Op::Close { .. } | Op::Ping => {}
    }
    Ok(())
}
