pub use sea_orm_migration::prelude::*;

mod m20260101_000001_users_auth;
mod m20260101_000002_profiles;
mod m20260101_000003_filter_window;
mod m20260101_000004_social;
mod m20261008_000005_cleanup_retention;
mod m20261008_000006_moderation;
mod m20261008_000007_areas;
mod m20261008_000008_ws_presence;
mod m20261008_000009_identities;
mod m20261008_000010_push;
mod m20261008_000011_password_reset;
mod m20261008_000012_google_identity;
mod m20261008_000013_telegram_identity;
mod m20261008_000014_facebook_identity;
mod m20261010_000015_free_usernames;

/// Append-only: never edit a shipped migration, add a new one at the end of the list.
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260101_000001_users_auth::Migration),
            Box::new(m20260101_000002_profiles::Migration),
            Box::new(m20260101_000003_filter_window::Migration),
            Box::new(m20260101_000004_social::Migration),
            Box::new(m20261008_000005_cleanup_retention::Migration),
            Box::new(m20261008_000006_moderation::Migration),
            Box::new(m20261008_000007_areas::Migration),
            Box::new(m20261008_000008_ws_presence::Migration),
            Box::new(m20261008_000009_identities::Migration),
            Box::new(m20261008_000010_push::Migration),
            Box::new(m20261008_000011_password_reset::Migration),
            Box::new(m20261008_000012_google_identity::Migration),
            Box::new(m20261008_000013_telegram_identity::Migration),
            Box::new(m20261008_000014_facebook_identity::Migration),
            Box::new(m20261010_000015_free_usernames::Migration),
        ]
    }
}

/// Runs a raw SQL script (several statements allowed) inside the migration transaction.
pub(crate) async fn run_sql(manager: &SchemaManager<'_>, sql: &str) -> Result<(), DbErr> {
    manager.get_connection().execute_unprepared(sql).await?;
    Ok(())
}
