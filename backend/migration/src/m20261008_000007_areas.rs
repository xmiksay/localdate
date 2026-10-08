use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    // The CHECK compares against 'timed' only: a value added by ALTER TYPE cannot be used in the
    // same transaction. The FK has no delete action (not RESTRICT, whose SQLSTATE 23001 sea-orm does
    // not classify): an area with window rows cannot be deleted, admins deactivate it instead.
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            r#"
CREATE TYPE area_kind AS ENUM ('city_centre', 'train_station', 'venue', 'other');
CREATE TABLE area (
    id uuid PRIMARY KEY,
    name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 80),
    kind area_kind NOT NULL,
    lat double precision NOT NULL CHECK (lat BETWEEN -90 AND 90),
    lon double precision NOT NULL CHECK (lon BETWEEN -180 AND 180),
    radius_m integer NOT NULL CHECK (radius_m BETWEEN 50 AND 5000),
    active boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT now()
);

ALTER TYPE window_kind ADD VALUE 'area';
ALTER TABLE visibility_window
    ADD COLUMN area_id uuid REFERENCES area(id),
    ADD CONSTRAINT visibility_window_area_kind CHECK ((kind = 'timed') = (area_id IS NULL));
CREATE INDEX visibility_window_area_id_idx ON visibility_window (area_id) WHERE area_id IS NOT NULL;
"#,
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        crate::run_sql(
            manager,
            r#"
DELETE FROM visibility_window WHERE area_id IS NOT NULL;
ALTER TABLE visibility_window DROP COLUMN area_id;
ALTER TYPE window_kind RENAME TO window_kind_old;
CREATE TYPE window_kind AS ENUM ('timed');
ALTER TABLE visibility_window
    ALTER COLUMN kind DROP DEFAULT,
    ALTER COLUMN kind TYPE window_kind USING kind::text::window_kind,
    ALTER COLUMN kind SET DEFAULT 'timed';
DROP TYPE window_kind_old;
DROP TABLE area;
DROP TYPE area_kind;
"#,
        )
        .await
    }
}
