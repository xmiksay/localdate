use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    // Usernames become free-form display names (the app trims, NFC-normalises and rejects control
    // characters; see `auth::validation`). Uniqueness moves to `username_key`, the case-folded key
    // the app computes: Postgres `lower()` would depend on the database locale. Every existing name
    // matched `^[a-z0-9_]{3,32}$` — ASCII, already lowercase, NFC — so it is its own key.
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            r#"
ALTER TABLE "user" ADD COLUMN username_key text;
UPDATE "user" SET username_key = username;
ALTER TABLE "user" ALTER COLUMN username_key SET NOT NULL,
    DROP CONSTRAINT user_username_check,
    DROP CONSTRAINT user_username_key,
    ADD CONSTRAINT user_username_length_check CHECK (char_length(username) BETWEEN 1 AND 64),
    ADD CONSTRAINT user_username_key_key UNIQUE (username_key);
"#,
        )
        .await
    }

    // The old pattern comes back NOT VALID: free-form names created since stay, new ones must match.
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            r#"
ALTER TABLE "user" DROP COLUMN username_key,
    DROP CONSTRAINT user_username_length_check,
    ADD CONSTRAINT user_username_key UNIQUE (username),
    ADD CONSTRAINT user_username_check CHECK (username ~ '^[a-z0-9_]{3,32}$') NOT VALID;
"#,
        )
        .await
    }
}
