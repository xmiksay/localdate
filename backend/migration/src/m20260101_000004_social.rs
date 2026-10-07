use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(manager, SCHEMA).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            r#"DROP TABLE report; DROP TABLE block; DROP TABLE message; DROP TABLE "match"; DROP TABLE wave; DROP TYPE report_reason;"#,
        )
        .await
    }
}

const SCHEMA: &str = r#"
CREATE TYPE report_reason AS ENUM ('spam', 'harassment', 'fake', 'underage', 'other');

CREATE TABLE wave (
    id uuid PRIMARY KEY,
    from_user_id uuid NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    to_user_id uuid NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    window_id uuid NOT NULL REFERENCES visibility_window(id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    CHECK (from_user_id <> to_user_id),
    UNIQUE (window_id, to_user_id)
);
CREATE INDEX wave_to_user_id_idx ON wave (to_user_id);
CREATE INDEX wave_from_user_id_idx ON wave (from_user_id);

CREATE TABLE "match" (
    id uuid PRIMARY KEY,
    user_a uuid NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    user_b uuid NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK (user_a < user_b),
    UNIQUE (user_a, user_b)
);
CREATE INDEX match_user_b_idx ON "match" (user_b);

CREATE TABLE message (
    id uuid PRIMARY KEY,
    match_id uuid NOT NULL REFERENCES "match"(id) ON DELETE CASCADE,
    sender_id uuid NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    body text NOT NULL CHECK (char_length(body) BETWEEN 1 AND 2000),
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX message_match_id_created_at_idx ON message (match_id, created_at);

CREATE TABLE block (
    blocker_id uuid NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    blocked_id uuid NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (blocker_id, blocked_id),
    CHECK (blocker_id <> blocked_id)
);
CREATE INDEX block_blocked_id_idx ON block (blocked_id);

CREATE TABLE report (
    id uuid PRIMARY KEY,
    reporter_id uuid NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    reported_id uuid NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    reason report_reason NOT NULL,
    note text CHECK (char_length(note) <= 1000),
    created_at timestamptz NOT NULL DEFAULT now()
);
"#;
