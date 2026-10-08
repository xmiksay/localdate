use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    // `lang` lives on the subscription: the subscribing device's UI language is the only place
    // the server learns which language to write the notification in.
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            r#"
CREATE TABLE push_subscription (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    endpoint text NOT NULL UNIQUE,
    p256dh text NOT NULL,
    auth text NOT NULL,
    user_agent text,
    lang text NOT NULL DEFAULT 'cs' CHECK (lang IN ('cs', 'en')),
    created_at timestamptz NOT NULL DEFAULT now(),
    last_success_at timestamptz,
    failure_count integer NOT NULL DEFAULT 0
);
CREATE INDEX push_subscription_user_id_idx ON push_subscription (user_id);
CREATE TABLE push_prefs (
    user_id uuid PRIMARY KEY REFERENCES "user"(id) ON DELETE CASCADE,
    waves boolean NOT NULL DEFAULT true,
    matches boolean NOT NULL DEFAULT true,
    messages boolean NOT NULL DEFAULT true
);
"#,
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            "DROP TABLE push_prefs; DROP TABLE push_subscription;",
        )
        .await
    }
}
