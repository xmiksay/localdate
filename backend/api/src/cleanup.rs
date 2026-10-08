//! Periodic retention job: wipes coordinates of ended windows and deletes rows nobody can use
//! any more. Matches and messages are not touched — they live until account deletion.

use std::time::Duration as StdDuration;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbBackend, Statement,
    TransactionTrait, Value,
};
use tokio::time::MissedTickBehavior;

use crate::ws::REPLICA_STALE;

/// WebSocket replica rows go once they count as dead (`ws::REPLICA_STALE`).
pub const REPLICA_RETENTION: Duration = Duration::seconds(REPLICA_STALE.as_secs() as i64);

/// How long an ended window row is kept (without coordinates) before deletion.
pub const WINDOW_RETENTION: Duration = Duration::hours(24);
/// Revoked refresh tokens are kept this long so presenting one still revokes its family.
pub const REVOKED_TOKEN_RETENTION: Duration = Duration::days(7);
/// Arbitrary key shared by all replicas; only the holder runs a tick.
pub const LOCK_KEY: i64 = 0x6c64_636c_6e75_7031;

/// Points in time that decide what a tick removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cutoffs {
    /// Open windows that ran out by this are closed (coordinates wiped); expired waves go.
    pub now: DateTime<Utc>,
    /// Windows that ended at or before this are deleted.
    pub windows_ended_before: DateTime<Utc>,
    /// Refresh tokens revoked at or before this are deleted.
    pub tokens_revoked_before: DateTime<Utc>,
    /// WebSocket replicas last seen at or before this are deleted with their presence rows.
    pub replicas_seen_before: DateTime<Utc>,
}

impl Cutoffs {
    pub fn at(now: DateTime<Utc>) -> Self {
        Self {
            now,
            windows_ended_before: now - WINDOW_RETENTION,
            tokens_revoked_before: now - REVOKED_TOKEN_RETENTION,
            replicas_seen_before: now - REPLICA_RETENTION,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CleanupCounts {
    pub windows_closed: u64,
    pub waves_deleted: u64,
    pub windows_deleted: u64,
    pub coords_cleared: u64,
    pub tokens_deleted: u64,
    pub replicas_deleted: u64,
}

// Same effect as the lazy close in `discovery::window`, so an open window always has coords.
const CLOSE_EXPIRED: &str = "UPDATE visibility_window SET ended_at = ends_at, lat = NULL, lon = NULL \
     WHERE ended_at IS NULL AND ends_at <= $1";
const CLEAR_COORDS: &str = "UPDATE visibility_window SET lat = NULL, lon = NULL \
     WHERE ended_at IS NOT NULL AND (lat IS NOT NULL OR lon IS NOT NULL)";
// Waves left in the table are never matched ones: a match deletes the waves that formed it.
// Ended windows keep no waves (that is what closing a window does).
const DELETE_WAVES: &str = "DELETE FROM wave WHERE expires_at <= $1 \
     OR window_id IN (SELECT id FROM visibility_window WHERE ended_at IS NOT NULL)";
// A window ended either early (ended_at) or by running out (ends_at); ended_at may also be a
// lazy close stamped after ends_at, hence LEAST. Matches/messages have no FK to windows.
const DELETE_WINDOWS: &str = "DELETE FROM visibility_window WHERE LEAST(ends_at, ended_at) <= $1";
// A revoked token is only useful for reuse detection while its family still has a live token.
const DELETE_TOKENS: &str = "DELETE FROM refresh_token t WHERE t.expires_at <= $1 \
     OR (t.revoked_at <= $2 AND NOT EXISTS (SELECT 1 FROM refresh_token l \
         WHERE l.family_id = t.family_id AND l.revoked_at IS NULL AND l.expires_at > $1))";

// A replica that stopped heartbeating (crash, kill -9) leaves presence rows behind; they already
// stopped counting as online, this only reclaims them (cascade).
const DELETE_REPLICAS: &str = "DELETE FROM ws_replica WHERE seen_at <= $1";

/// One cleanup pass at the database's `now()` moved by `shift` (zero in production; tests use
/// it to jump ahead). `None` when another replica holds the lock (tick skipped).
pub async fn run_once(db: &DatabaseConnection, shift: Duration) -> Result<Option<CleanupCounts>> {
    let txn = db.begin().await.context("begin cleanup txn")?;
    if !try_lock(&txn).await? {
        txn.rollback().await.context("rollback cleanup txn")?;
        return Ok(None);
    }
    // The DB clock, like the nearby query, so app/DB clock skew cannot shift retention.
    let cut = Cutoffs::at(db_now(&txn).await? + shift);
    let counts = CleanupCounts {
        windows_closed: exec(&txn, CLOSE_EXPIRED, [cut.now.into()]).await?,
        coords_cleared: exec(&txn, CLEAR_COORDS, []).await?,
        waves_deleted: exec(&txn, DELETE_WAVES, [cut.now.into()]).await?,
        windows_deleted: exec(&txn, DELETE_WINDOWS, [cut.windows_ended_before.into()]).await?,
        tokens_deleted: exec(
            &txn,
            DELETE_TOKENS,
            [cut.now.into(), cut.tokens_revoked_before.into()],
        )
        .await?,
        replicas_deleted: exec(&txn, DELETE_REPLICAS, [cut.replicas_seen_before.into()]).await?,
    };
    txn.commit().await.context("commit cleanup txn")?;
    Ok(Some(counts))
}

/// Runs [`run_once`] every `every` forever; failures are logged and the next tick retries.
pub async fn run_forever(db: DatabaseConnection, every: StdDuration) {
    let mut ticker = tokio::time::interval(every);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        let db = db.clone();
        // Own task per tick: a panic surfaces as a JoinError here instead of ending the loop.
        let outcome = match tokio::spawn(async move { run_once(&db, Duration::zero()).await }).await
        {
            Ok(result) => result,
            Err(e) => {
                tracing::error!(error = %e, "cleanup tick panicked");
                continue;
            }
        };
        match outcome {
            Ok(Some(c)) if c != CleanupCounts::default() => tracing::info!(
                closed = c.windows_closed,
                waves = c.waves_deleted,
                windows = c.windows_deleted,
                coords = c.coords_cleared,
                refresh_tokens = c.tokens_deleted,
                ws_replicas = c.replicas_deleted,
                "cleanup removed expired data"
            ),
            Ok(Some(_)) => tracing::debug!("cleanup: nothing to do"),
            Ok(None) => tracing::debug!("cleanup lock held elsewhere, tick skipped"),
            Err(e) => tracing::error!(error = format!("{e:#}"), "cleanup tick failed"),
        }
    }
}

/// Transaction-scoped, so the lock is released on commit/rollback even if the job errors.
async fn try_lock(txn: &DatabaseTransaction) -> Result<bool> {
    let stmt = Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT pg_try_advisory_xact_lock($1) AS locked",
        [LOCK_KEY.into()],
    );
    txn.query_one(stmt)
        .await
        .context("taking cleanup advisory lock")?
        .context("advisory lock query returned no row")?
        .try_get("", "locked")
        .context("reading advisory lock result")
}

async fn db_now(txn: &DatabaseTransaction) -> Result<DateTime<Utc>> {
    let stmt = Statement::from_string(DbBackend::Postgres, "SELECT now() AS now");
    txn.query_one(stmt)
        .await
        .context("reading database time")?
        .context("now() returned no row")?
        .try_get("", "now")
        .context("decoding database time")
}

async fn exec<const N: usize>(
    txn: &DatabaseTransaction,
    sql: &str,
    values: [Value; N],
) -> Result<u64> {
    let stmt = Statement::from_sql_and_values(DbBackend::Postgres, sql, values);
    Ok(txn
        .execute(stmt)
        .await
        .with_context(|| format!("cleanup: {sql}"))?
        .rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cutoffs_keep_windows_a_day_and_revoked_tokens_a_week() {
        let now = DateTime::parse_from_rfc3339("2026-10-08T12:00:00Z")
            .expect("valid timestamp")
            .to_utc();
        let cut = Cutoffs::at(now);
        assert_eq!(cut.now, now);
        assert_eq!(
            cut.windows_ended_before.to_rfc3339(),
            "2026-10-07T12:00:00+00:00"
        );
        assert_eq!(
            cut.tokens_revoked_before.to_rfc3339(),
            "2026-10-01T12:00:00+00:00"
        );
        assert_eq!(
            cut.replicas_seen_before.to_rfc3339(),
            "2026-10-08T11:58:30+00:00"
        );
    }
}
