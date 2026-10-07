use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(manager, SCHEMA).await?;
        // Single INSERT so serial ids are 1..40 in contract order (docs/api.md).
        crate::run_sql(manager, SEED).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            "DROP TABLE user_interest; DROP TABLE interest; DROP TABLE photo; DROP TABLE profile; DROP TYPE gender;",
        )
        .await
    }
}

const SCHEMA: &str = r#"
CREATE TYPE gender AS ENUM ('male', 'female', 'other');

CREATE TABLE profile (
    user_id uuid PRIMARY KEY REFERENCES "user"(id) ON DELETE CASCADE,
    display_name text NOT NULL CHECK (char_length(display_name) BETWEEN 1 AND 40),
    birth_date date NOT NULL,
    gender gender NOT NULL,
    bio text NOT NULL DEFAULT '' CHECK (char_length(bio) <= 500),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE photo (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    file_name text NOT NULL,
    position smallint NOT NULL CHECK (position >= 0),
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX photo_user_id_idx ON photo (user_id, position);

CREATE TABLE interest (
    id serial PRIMARY KEY,
    key text NOT NULL UNIQUE
);

CREATE TABLE user_interest (
    user_id uuid NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    interest_id integer NOT NULL REFERENCES interest(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, interest_id)
);
"#;

const SEED: &str = r#"
INSERT INTO interest (key) VALUES
    ('hiking'),
    ('running'),
    ('cycling'),
    ('climbing'),
    ('swimming'),
    ('yoga'),
    ('gym'),
    ('football'),
    ('tennis'),
    ('skiing'),
    ('music'),
    ('concerts'),
    ('festivals'),
    ('dancing'),
    ('singing'),
    ('guitar'),
    ('cinema'),
    ('theatre'),
    ('art'),
    ('photography'),
    ('reading'),
    ('writing'),
    ('gaming'),
    ('board_games'),
    ('tech'),
    ('science'),
    ('travel'),
    ('languages'),
    ('cooking'),
    ('coffee'),
    ('wine'),
    ('beer'),
    ('food'),
    ('nature'),
    ('animals'),
    ('volunteering'),
    ('fashion'),
    ('meditation'),
    ('history'),
    ('cars');
"#;
