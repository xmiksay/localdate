//! Cross-replica presence: `ws_presence` (one row per open socket) and `ws_replica` heartbeats.

use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement, Value};
use uuid::Uuid;

/// How often a replica refreshes `ws_replica.seen_at` and reconciles its presence rows.
pub const HEARTBEAT: Duration = Duration::from_secs(30);
/// A replica not seen for this long is considered dead; its presence rows no longer count.
pub const REPLICA_STALE: Duration = Duration::from_secs(90);

const UPSERT_REPLICA: &str = "INSERT INTO ws_replica (replica_id, seen_at) VALUES ($1, now()) \
     ON CONFLICT (replica_id) DO UPDATE SET seen_at = now()";
// Rows of sockets that are gone (a failed delete, an aborted session). Rows newer than the
// snapshot belong to sockets that subscribed after it and must stay.
const DROP_GONE: &str = "DELETE FROM ws_presence WHERE replica_id = $1 AND connected_at < $2 \
     AND NOT (socket_id = ANY($3))";
// Brings rows back after a cleanup purged this replica as stale (e.g. after a long DB outage).
const RESTORE: &str = "INSERT INTO ws_presence (replica_id, socket_id, user_id, connected_at) \
     SELECT $1, s, u, $2 FROM unnest($3::bigint[], $4::uuid[]) AS t(s, u) \
     WHERE EXISTS (SELECT 1 FROM \"user\" WHERE id = u) ON CONFLICT DO NOTHING";
const INSERT: &str = "INSERT INTO ws_presence (replica_id, socket_id, user_id, connected_at) \
     VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING";
const DELETE: &str = "DELETE FROM ws_presence WHERE replica_id = $1 AND socket_id = $2";
const DELETE_REPLICA: &str = "DELETE FROM ws_replica WHERE replica_id = $1";
const IS_ONLINE: &str = "SELECT EXISTS (SELECT 1 FROM ws_presence p \
     JOIN ws_replica r USING (replica_id) \
     WHERE p.user_id = $1 AND r.seen_at > now() - $2 * interval '1 second') AS online";

/// Marks `replica` alive and makes its presence rows match `sockets` (taken at `taken_at`).
/// A socket that closes between the snapshot and this call may be restored and linger until the
/// next heartbeat — at worst one user reads as online for one extra period.
pub async fn heartbeat(
    db: &DatabaseConnection,
    replica: Uuid,
    sockets: &[(i64, Uuid)],
    taken_at: DateTime<Utc>,
) -> Result<()> {
    exec(db, UPSERT_REPLICA, vec![replica.into()]).await?;
    let ids: Vec<i64> = sockets.iter().map(|(id, _)| *id).collect();
    let users: Vec<Uuid> = sockets.iter().map(|(_, user)| *user).collect();
    exec(
        db,
        DROP_GONE,
        vec![replica.into(), taken_at.into(), ids.clone().into()],
    )
    .await?;
    exec(
        db,
        RESTORE,
        vec![replica.into(), taken_at.into(), ids.into(), users.into()],
    )
    .await
}

pub async fn insert(db: &DatabaseConnection, replica: Uuid, socket: i64, user: Uuid) -> Result<()> {
    let values = vec![
        replica.into(),
        socket.into(),
        user.into(),
        Utc::now().into(),
    ];
    exec(db, INSERT, values).await
}

pub async fn remove(db: &DatabaseConnection, replica: Uuid, socket: i64) -> Result<()> {
    exec(db, DELETE, vec![replica.into(), socket.into()]).await
}

/// Drops the replica and, by cascade, all its presence rows (graceful shutdown).
pub async fn deregister(db: &DatabaseConnection, replica: Uuid) -> Result<()> {
    exec(db, DELETE_REPLICA, vec![replica.into()]).await
}

/// Whether `user` has an open socket on any live replica.
pub async fn is_online(db: &DatabaseConnection, user: Uuid) -> Result<bool> {
    let stmt = Statement::from_sql_and_values(
        DbBackend::Postgres,
        IS_ONLINE,
        [user.into(), REPLICA_STALE.as_secs_f64().into()],
    );
    db.query_one(stmt)
        .await
        .context("querying presence")?
        .context("presence query returned no row")?
        .try_get("", "online")
        .context("decoding presence")
}

async fn exec(db: &DatabaseConnection, sql: &str, values: Vec<Value>) -> Result<()> {
    crate::sql::exec(db, sql, values)
        .await
        .with_context(|| format!("ws presence: {sql}"))?;
    Ok(())
}
