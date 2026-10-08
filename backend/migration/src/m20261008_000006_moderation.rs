use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    // reporter_id becomes SET NULL: a reporter deleting their account must not erase the evidence
    // against someone else. Reports about a deleted account still cascade away with it.
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            r#"
ALTER TABLE "user"
    ADD COLUMN is_admin boolean NOT NULL DEFAULT false,
    ADD COLUMN banned_at timestamptz;

CREATE TYPE report_resolution AS ENUM ('dismissed', 'banned');
ALTER TABLE report
    ALTER COLUMN reporter_id DROP NOT NULL,
    DROP CONSTRAINT report_reporter_id_fkey,
    ADD CONSTRAINT report_reporter_id_fkey
        FOREIGN KEY (reporter_id) REFERENCES "user"(id) ON DELETE SET NULL,
    ADD COLUMN resolved_at timestamptz,
    ADD COLUMN resolved_by uuid REFERENCES "user"(id) ON DELETE SET NULL,
    ADD COLUMN resolution report_resolution,
    ADD CONSTRAINT report_resolution_complete
        CHECK ((resolved_at IS NULL) = (resolution IS NULL));
CREATE INDEX report_open_reported_id_idx ON report (reported_id) WHERE resolved_at IS NULL;
"#,
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            r#"
DROP INDEX report_open_reported_id_idx;
DELETE FROM report WHERE reporter_id IS NULL;
ALTER TABLE report
    DROP CONSTRAINT report_resolution_complete,
    DROP COLUMN resolution,
    DROP COLUMN resolved_by,
    DROP COLUMN resolved_at,
    DROP CONSTRAINT report_reporter_id_fkey,
    ADD CONSTRAINT report_reporter_id_fkey
        FOREIGN KEY (reporter_id) REFERENCES "user"(id) ON DELETE CASCADE,
    ALTER COLUMN reporter_id SET NOT NULL;
DROP TYPE report_resolution;
ALTER TABLE "user" DROP COLUMN banned_at, DROP COLUMN is_admin;
"#,
        )
        .await
    }
}
