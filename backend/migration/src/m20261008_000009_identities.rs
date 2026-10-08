use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    // Providers and purposes are Postgres enums so later providers (#14, #15, #17) and password
    // reset (#18) are one `ALTER TYPE … ADD VALUE` each.
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            r#"
ALTER TABLE "user" ALTER COLUMN password_hash DROP NOT NULL;

CREATE TYPE identity_provider AS ENUM ('email');
CREATE TABLE user_identity (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    provider identity_provider NOT NULL,
    subject text NOT NULL CHECK (char_length(subject) BETWEEN 1 AND 320),
    verified_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (provider, subject)
);
CREATE INDEX user_identity_user_id_idx ON user_identity (user_id);

CREATE TYPE email_token_purpose AS ENUM ('login', 'link', 'signup');
CREATE TABLE email_token (
    id uuid PRIMARY KEY,
    token_hash text NOT NULL UNIQUE,
    purpose email_token_purpose NOT NULL,
    user_id uuid REFERENCES "user"(id) ON DELETE CASCADE,
    email text NOT NULL,
    expires_at timestamptz NOT NULL,
    used_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK ((purpose = 'signup') = (user_id IS NULL))
);
CREATE INDEX email_token_expires_at_idx ON email_token (expires_at);
"#,
        )
        .await
    }

    // Rolling back DELETES every passwordless (email-only) account: the old schema cannot hold
    // them. Restore a backup instead if those accounts matter.
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            r#"
DROP TABLE email_token;
DROP TYPE email_token_purpose;
DROP TABLE user_identity;
DROP TYPE identity_provider;
DELETE FROM "user" WHERE password_hash IS NULL;
ALTER TABLE "user" ALTER COLUMN password_hash SET NOT NULL;
"#,
        )
        .await
    }
}
