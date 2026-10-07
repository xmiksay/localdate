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
            "DROP TABLE visibility_window; DROP TABLE filter; DROP TYPE window_kind; DROP TYPE reason;",
        )
        .await
    }
}

// "At most one active window per user" cannot use now() in an index predicate, so the DB
// enforces one *open* window (ended_at IS NULL); the app must set ended_at when it replaces
// a window or notices it expired (ends_at <= now()) before inserting a new one.
const SCHEMA: &str = r#"
CREATE TYPE reason AS ENUM ('date', 'meet');
CREATE TYPE window_kind AS ENUM ('timed');

CREATE TABLE filter (
    user_id uuid PRIMARY KEY REFERENCES "user"(id) ON DELETE CASCADE,
    max_distance_m integer NOT NULL CHECK (max_distance_m BETWEEN 200 AND 10000),
    genders gender[] NOT NULL DEFAULT '{}',
    age_min smallint NOT NULL CHECK (age_min BETWEEN 18 AND 99),
    age_max smallint NOT NULL CHECK (age_max BETWEEN 18 AND 99),
    reasons reason[] NOT NULL CHECK (cardinality(reasons) >= 1),
    default_window_minutes smallint NOT NULL CHECK (default_window_minutes IN (30, 60, 120, 240)),
    CHECK (age_min <= age_max)
);

CREATE TABLE visibility_window (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    kind window_kind NOT NULL DEFAULT 'timed',
    lat double precision NOT NULL CHECK (lat BETWEEN -90 AND 90),
    lon double precision NOT NULL CHECK (lon BETWEEN -180 AND 180),
    location_updated_at timestamptz NOT NULL DEFAULT now(),
    starts_at timestamptz NOT NULL DEFAULT now(),
    ends_at timestamptz NOT NULL,
    ended_at timestamptz
);
CREATE UNIQUE INDEX visibility_window_one_open_per_user ON visibility_window (user_id) WHERE ended_at IS NULL;
CREATE INDEX visibility_window_lat_lon_idx ON visibility_window (lat, lon);
"#;
