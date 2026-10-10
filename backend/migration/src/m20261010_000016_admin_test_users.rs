use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    // Audit rows outlive both accounts (`SET NULL`): what an admin did as someone stays readable after
    // a test user is deleted or the admin account goes.
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            r#"
ALTER TABLE "user" ADD COLUMN is_test boolean NOT NULL DEFAULT false;
CREATE INDEX user_is_test_idx ON "user" (created_at DESC) WHERE is_test;

CREATE TABLE admin_audit (
    id uuid PRIMARY KEY,
    admin_id uuid REFERENCES "user" (id) ON DELETE SET NULL,
    target_user_id uuid REFERENCES "user" (id) ON DELETE SET NULL,
    action text NOT NULL CHECK (action IN ('impersonate', 'impersonated_request')),
    meta jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX admin_audit_created_at_idx ON admin_audit (created_at DESC);
"#,
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            r#"
DROP TABLE admin_audit;
ALTER TABLE "user" DROP COLUMN is_test;
"#,
        )
        .await
    }
}
