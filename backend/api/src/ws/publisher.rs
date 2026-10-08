//! The NOTIFY side of the bridge: one bounded queue, drained in order by one task.

use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};
use tokio::sync::mpsc::{self, Receiver, Sender, error::TrySendError};
use uuid::Uuid;

use super::envelope::{self, CHANNEL, Op};

/// Ops waiting for NOTIFY; beyond this, new events are dropped (Close ops still get queued).
pub const QUEUE_CAP: usize = 10_000;
/// An event this old when its turn comes is dropped: the recipient has likely refetched anyway.
pub const EVENT_MAX_AGE: Duration = Duration::from_secs(30);
/// How long a Close op (a ban) is retried before giving up; the session's periodic account
/// check still closes the socket.
pub const CLOSE_RETRY_FOR: Duration = Duration::from_secs(300);

pub struct Queued {
    at: Instant,
    op: Op,
}

impl Queued {
    /// Close ops never expire: a ban must reach every replica.
    fn expired(&self, now: Instant) -> bool {
        !self.op.is_close() && now.duration_since(self.at) > EVENT_MAX_AGE
    }
}

#[derive(Clone)]
pub struct Outbox {
    tx: Sender<Queued>,
}

impl Outbox {
    pub fn new(capacity: usize) -> (Self, Receiver<Queued>) {
        let (tx, rx) = mpsc::channel(capacity);
        (Self { tx }, rx)
    }

    pub fn push(&self, op: Op) {
        let item = Queued {
            at: Instant::now(),
            op,
        };
        match self.tx.try_send(item) {
            Ok(()) | Err(TrySendError::Closed(_)) => {}
            Err(TrySendError::Full(item)) if item.op.is_close() => {
                // Waits for room instead; may overtake queued events, which is harmless for a close.
                let tx = self.tx.clone();
                tokio::spawn(async move { tx.send(item).await });
            }
            Err(TrySendError::Full(item)) => {
                tracing::warn!(op = item.op.kind(), "ws publish queue full; op dropped");
            }
        }
    }
}

/// Publishes queued ops in order until every [`Outbox`] is gone.
pub async fn run(db: DatabaseConnection, replica_id: Uuid, mut queue: Receiver<Queued>) {
    let mut dropped = 0u64;
    while let Some(item) = queue.recv().await {
        if item.expired(Instant::now()) {
            dropped += 1;
            if queue.is_empty() {
                tracing::warn!(dropped, "ws publish backlog: stale events dropped");
                dropped = 0;
            }
            continue;
        }
        if dropped > 0 {
            tracing::warn!(dropped, "ws publish backlog: stale events dropped");
            dropped = 0;
        }
        let close = item.op.is_close();
        let payload = match envelope::encode(replica_id, item.op) {
            Ok(payload) => payload,
            Err(e) => {
                tracing::error!(error = format!("{e:#}"), "ws op not publishable, dropped");
                continue;
            }
        };
        if close {
            publish_close(&db, &payload).await;
        } else if let Err(e) = notify(&db, &payload).await {
            tracing::warn!(
                error = format!("{e:#}"),
                "ws NOTIFY failed; other replicas miss this op"
            );
        }
    }
}

async fn publish_close(db: &DatabaseConnection, payload: &str) {
    let started = Instant::now();
    let mut attempt = 0;
    while let Err(e) = notify(db, payload).await {
        if started.elapsed() >= CLOSE_RETRY_FOR {
            tracing::error!(
                error = format!("{e:#}"),
                "ws close op never reached other replicas; their account re-check closes the sockets"
            );
            return;
        }
        let delay = crate::retry::backoff(attempt);
        tracing::warn!(error = format!("{e:#}"), retry_in = ?delay, "ws close NOTIFY failed");
        tokio::time::sleep(delay).await;
        attempt += 1;
    }
}

async fn notify(db: &DatabaseConnection, payload: &str) -> Result<()> {
    let stmt = Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT pg_notify($1, $2)",
        [CHANNEL.into(), payload.into()],
    );
    db.execute(stmt).await.context("pg_notify")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ws::{CloseReason, ServerEvent};

    fn event() -> Op {
        Op::Event {
            user: Uuid::nil(),
            event: ServerEvent::Ready,
        }
    }

    fn close() -> Op {
        Op::Close {
            user: Uuid::nil(),
            reason: CloseReason::Banned,
        }
    }

    #[test]
    fn only_old_events_expire_never_closes() {
        let now = Instant::now();
        let old = now - EVENT_MAX_AGE - Duration::from_secs(1);
        let fresh = Queued {
            at: now,
            op: event(),
        };
        let stale = Queued {
            at: old,
            op: event(),
        };
        let stale_close = Queued {
            at: old,
            op: close(),
        };
        assert!(!fresh.expired(now));
        assert!(stale.expired(now));
        assert!(!stale_close.expired(now));
    }

    #[tokio::test]
    async fn full_queue_drops_events_but_still_takes_closes() {
        let (outbox, mut rx) = Outbox::new(1);
        outbox.push(event());
        outbox.push(event()); // dropped
        outbox.push(close()); // waits for room
        assert!(matches!(
            rx.recv().await.map(|q| q.op),
            Some(Op::Event { .. })
        ));
        assert!(matches!(
            rx.recv().await.map(|q| q.op),
            Some(Op::Close { .. })
        ));
        drop(outbox);
        assert!(rx.recv().await.is_none());
    }
}
