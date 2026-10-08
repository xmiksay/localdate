use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    // `ADD VALUE` may run inside the migration transaction (Postgres ≥ 12) as long as nothing in it
    // uses the new value. `oauth_grant` holds the one-time codes of the OAuth callback and the
    // pending sign-up tokens; `binding` (sha256 of the flow cookie's state) ties a code to the
    // browser that ran the flow, sign-up tokens are already in the SPA's hands and carry none.
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            r#"
ALTER TYPE identity_provider ADD VALUE IF NOT EXISTS 'google';

CREATE TYPE oauth_grant_purpose AS ENUM ('login', 'signup_code', 'signup');
CREATE TABLE oauth_grant (
    id uuid PRIMARY KEY,
    token_hash text NOT NULL UNIQUE,
    purpose oauth_grant_purpose NOT NULL,
    provider identity_provider NOT NULL,
    subject text NOT NULL CHECK (char_length(subject) BETWEEN 1 AND 320),
    user_id uuid REFERENCES "user"(id) ON DELETE CASCADE,
    binding text,
    expires_at timestamptz NOT NULL,
    used_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK ((purpose = 'login') = (user_id IS NOT NULL)),
    CHECK ((purpose = 'signup') = (binding IS NULL))
);
CREATE INDEX oauth_grant_expires_at_idx ON oauth_grant (expires_at);
"#,
        )
        .await
    }

    // Rolling back DELETES every Google identity and the accounts left without any login method:
    // the old code cannot log them in. Restore a backup instead if they matter. The `google` enum
    // value stays (Postgres cannot drop one without rebuilding the type, and an unused value is
    // harmless); `up` adds it with IF NOT EXISTS, so a re-run works.
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            r#"
DROP TABLE oauth_grant;
DROP TYPE oauth_grant_purpose;
DELETE FROM user_identity WHERE provider = 'google';
DELETE FROM "user" u WHERE u.password_hash IS NULL
    AND NOT EXISTS (SELECT 1 FROM user_identity i WHERE i.user_id = u.id);
"#,
        )
        .await
    }
}
