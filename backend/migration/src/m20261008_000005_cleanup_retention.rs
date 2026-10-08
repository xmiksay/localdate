use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    // Coordinates are wiped as soon as a window ends; the row itself stays for 24 h.
    // Invariant: an open window (ended_at IS NULL) always has coordinates.
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            r#"
ALTER TABLE visibility_window ALTER COLUMN lat DROP NOT NULL, ALTER COLUMN lon DROP NOT NULL;
UPDATE visibility_window SET ended_at = ends_at WHERE ended_at IS NULL AND ends_at <= now();
DELETE FROM wave WHERE window_id IN (SELECT id FROM visibility_window WHERE ended_at IS NOT NULL);
UPDATE visibility_window SET lat = NULL, lon = NULL WHERE ended_at IS NOT NULL;
ALTER TABLE visibility_window ADD CONSTRAINT visibility_window_open_has_coords
    CHECK (ended_at IS NOT NULL OR (lat IS NOT NULL AND lon IS NOT NULL));
CREATE INDEX visibility_window_ends_at_idx ON visibility_window (ends_at);
CREATE INDEX wave_expires_at_idx ON wave (expires_at);
CREATE INDEX refresh_token_expires_at_idx ON refresh_token (expires_at);
CREATE INDEX refresh_token_revoked_at_idx ON refresh_token (revoked_at);
"#,
        )
        .await
    }

    // Ended windows without coordinates are dead rows, so dropping them loses nothing live.
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            r#"
DROP INDEX refresh_token_revoked_at_idx, refresh_token_expires_at_idx, wave_expires_at_idx,
    visibility_window_ends_at_idx;
ALTER TABLE visibility_window DROP CONSTRAINT visibility_window_open_has_coords;
DELETE FROM visibility_window
WHERE (lat IS NULL OR lon IS NULL) AND (ended_at IS NOT NULL OR ends_at <= now());
ALTER TABLE visibility_window ALTER COLUMN lat SET NOT NULL, ALTER COLUMN lon SET NOT NULL;
"#,
        )
        .await
    }
}
