use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    // One row per open WebSocket, so several tabs on one replica need no reference counting.
    // Presence rows hang off their replica: purging a stale replica takes its sockets with it.
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            r#"
CREATE TABLE ws_replica (
    replica_id uuid PRIMARY KEY,
    seen_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX ws_replica_seen_at_idx ON ws_replica (seen_at);
CREATE TABLE ws_presence (
    replica_id uuid NOT NULL REFERENCES ws_replica(replica_id) ON DELETE CASCADE,
    socket_id bigint NOT NULL,
    user_id uuid NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    connected_at timestamptz NOT NULL,
    PRIMARY KEY (replica_id, socket_id)
);
CREATE INDEX ws_presence_user_id_idx ON ws_presence (user_id);
"#,
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(manager, "DROP TABLE ws_presence; DROP TABLE ws_replica;").await
    }
}
