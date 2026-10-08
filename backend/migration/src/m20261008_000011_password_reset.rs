use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            r#"
ALTER TYPE email_token_purpose ADD VALUE 'password_reset';
ALTER TABLE "user" ADD COLUMN credentials_changed_at timestamptz;
"#,
        )
        .await
    }

    // Postgres cannot drop an enum value; only the tokens go. The value left behind is unused.
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            r#"
ALTER TABLE "user" DROP COLUMN credentials_changed_at;
DELETE FROM email_token WHERE purpose = 'password_reset';
"#,
        )
        .await
    }
}
