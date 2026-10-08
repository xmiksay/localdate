use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    // `photo` holds a Facebook profile picture, already re-encoded to WebP, for a sign-up that has
    // not picked its username yet: the provider access token that fetched it is never stored, so
    // the picture cannot be fetched again later. Only sign-up grants carry one; the size cap
    // matches `auth::oauth::import::MAX_PENDING_PHOTO_BYTES`.
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            r#"
ALTER TYPE identity_provider ADD VALUE IF NOT EXISTS 'facebook';

ALTER TABLE oauth_grant ADD COLUMN photo bytea,
    ADD CONSTRAINT oauth_grant_photo_check CHECK (
        photo IS NULL
        OR (purpose IN ('signup_code', 'signup') AND octet_length(photo) <= 8388608)
    );
"#,
        )
        .await
    }

    // Rolling back DELETES every Facebook identity and the accounts left without any login method,
    // like 000012 does for Google; the `facebook` enum value stays (harmless, re-run safe).
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            r#"
ALTER TABLE oauth_grant DROP COLUMN photo;
DELETE FROM oauth_grant WHERE provider = 'facebook';
DELETE FROM user_identity WHERE provider = 'facebook';
DELETE FROM "user" u WHERE u.password_hash IS NULL
    AND NOT EXISTS (SELECT 1 FROM user_identity i WHERE i.user_id = u.id);
"#,
        )
        .await
    }
}
