use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    // `ADD VALUE` may run inside the migration transaction (Postgres ≥ 12) as long as nothing in it
    // uses the new value. `email_token.provider` says which identity a token's `email` column names
    // (an address, or a Telegram user id): password-reset links are also sent by Telegram.
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            r#"
ALTER TYPE identity_provider ADD VALUE IF NOT EXISTS 'telegram';
ALTER TABLE email_token ADD COLUMN provider identity_provider NOT NULL DEFAULT 'email';
"#,
        )
        .await
    }

    // Postgres cannot drop an enum value; it stays, unused. Telegram identities go, and with them
    // accounts left without any login method. Restore a backup instead if they matter.
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            r#"
DELETE FROM email_token WHERE provider <> 'email';
DELETE FROM oauth_grant WHERE provider = 'telegram';
ALTER TABLE email_token DROP COLUMN provider;
DELETE FROM user_identity WHERE provider = 'telegram';
DELETE FROM "user" u WHERE u.password_hash IS NULL
    AND NOT EXISTS (SELECT 1 FROM user_identity i WHERE i.user_id = u.id);
"#,
        )
        .await
    }
}
