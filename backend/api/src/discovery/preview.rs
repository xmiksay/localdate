//! `GET /preview`: the guest preview for visitors without an account. Anyone can register a fake
//! account to look around, so the login wall protects nobody; a basic preview takes away the
//! reason to. It shows less than a registered window (no `user_id`, bio, gallery or interests),
//! at most [`CAP`] cards, and the guest's point is used for the query only — never stored.

use std::cmp::Reverse;
use std::collections::HashMap;

use axum::Json;
use axum::extract::{Query, State};
use entity::Gender;
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{ActiveEnum, DbBackend, FromQueryResult, Statement};
use serde::Serialize;
use uuid::Uuid;

use super::geo::{DistanceBand, distance_band, ensure_valid};
use super::rules::AREA_STALE_SECS;
use crate::areas::{AreaRef, coord};
use crate::error::AppError;
use crate::me::media_url;
use crate::rate_limit::ClientIp;
use crate::state::AppState;

/// Cards per response; `total` tells how many more registering would show.
pub const CAP: usize = 10;
/// A guest sees timed windows at most this far away, and never past the user's own max distance.
pub const MAX_DISTANCE_M: i32 = 5000;

/// $1/$2 = guest lat/lon, $3 = guest gender, $4 = genders the guest wants (empty = any),
/// $5 = `AREA_STALE_SECS`, $6 = `MAX_DISTANCE_M`. Like `nearby::SQL` minus everything a guest
/// lacks (window, age, reasons, blocks): only the windowed user's gender filter applies to the
/// guest. A timed window counts within min(its max distance, $6); a fresh area window counts when
/// the guest's point lies inside its area's circle.
const SQL: &str = r#"
SELECT ow.user_id, ow.starts_at, dist.d AS distance_m, op.display_name,
       op.gender::text AS gender, ages.age, ar.id AS area_id, ar.name AS area_name, ph.file_name
FROM visibility_window ow
JOIN filter ofl ON ofl.user_id = ow.user_id
JOIN profile op ON op.user_id = ow.user_id
JOIN "user" ou ON ou.id = ow.user_id AND ou.banned_at IS NULL
LEFT JOIN area ar ON ar.id = ow.area_id
CROSS JOIN LATERAL (SELECT file_name FROM photo
    WHERE photo.user_id = ow.user_id ORDER BY position LIMIT 1) ph
CROSS JOIN LATERAL (SELECT
    $6::float8 / 110000.0 AS dlat,
    $6::float8 / 110000.0 / GREATEST(cos(radians($1::float8)), 1e-6) AS dlon) bb
CROSS JOIN LATERAL (SELECT
    2 * 6371000 * asin(sqrt(least(1.0,
        sin(radians(ow.lat - $1::float8) / 2) ^ 2
        + cos(radians($1::float8)) * cos(radians(ow.lat)) * sin(radians(ow.lon - $2::float8) / 2) ^ 2
    ))) AS d) dist
CROSS JOIN LATERAL (SELECT
    2 * 6371000 * asin(sqrt(least(1.0,
        sin(radians(ar.lat - $1::float8) / 2) ^ 2
        + cos(radians($1::float8)) * cos(radians(ar.lat)) * sin(radians(ar.lon - $2::float8) / 2) ^ 2
    ))) AS d) area_dist
CROSS JOIN LATERAL (SELECT
    date_part('year', age((now() AT TIME ZONE 'UTC')::date, op.birth_date))::int AS age) ages
WHERE ow.ended_at IS NULL AND ow.ends_at > now()
  AND ((ow.area_id IS NULL
    AND ow.lat BETWEEN $1::float8 - bb.dlat AND $1::float8 + bb.dlat
    AND (abs(ow.lon - $2::float8) <= bb.dlon OR abs(ow.lon - $2::float8) >= 360 - bb.dlon)
    AND dist.d <= LEAST(ofl.max_distance_m, $6::int))
  OR (ow.area_id IS NOT NULL
    AND ow.location_updated_at > now() - make_interval(secs => $5::float8)
    AND area_dist.d <= ar.radius_m))
  AND (cardinality(ofl.genders) = 0 OR $3::text::gender = ANY(ofl.genders))
  AND (cardinality($4::text[]) = 0 OR op.gender = ANY($4::text[]::gender[]))
"#;

#[derive(Debug, FromQueryResult)]
struct PreviewRow {
    user_id: Uuid,
    starts_at: DateTimeWithTimeZone,
    distance_m: f64,
    display_name: String,
    gender: String,
    age: i32,
    area_id: Option<Uuid>,
    area_name: Option<String>,
    file_name: String,
}

#[derive(Serialize, Debug)]
pub struct PreviewProfile {
    display_name: String,
    age: i32,
    gender: Gender,
    photo_url: String,
    /// `None` for area windows: the shared area is the only place information, as in `/nearby`.
    distance_band: Option<DistanceBand>,
    area: Option<AreaRef>,
}

#[derive(Serialize, Debug)]
pub struct Preview {
    total: usize,
    items: Vec<PreviewProfile>,
}

fn gender(raw: &str, key: &str) -> Result<Gender, AppError> {
    Gender::try_from_value(&raw.to_owned())
        .map_err(|_| AppError::validation(format!("{key} must be male, female or other")))
}

/// `genders` as a comma-separated list; missing or empty = any.
fn genders(params: &HashMap<String, String>) -> Result<Vec<Gender>, AppError> {
    params
        .get("genders")
        .map(|v| v.split(',').filter(|s| !s.is_empty()).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .map(|s| gender(s, "genders"))
        .collect()
}

/// Area windows first (the guest stands in their area), then nearest band, newest window; the
/// hidden `user_id` only makes the order stable.
fn sort_for_display(rows: &mut [PreviewRow]) {
    rows.sort_by_key(|r| {
        (
            r.area_id.is_none().then(|| distance_band(r.distance_m)),
            Reverse(r.starts_at),
            r.user_id,
        )
    });
}

pub async fn get_preview(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Preview>, AppError> {
    // Its own bucket, so a guest browsing the preview never spends the login budget.
    if ip.is_some_and(|ip| !state.preview_limiter.check(ip)) {
        return Err(AppError::RateLimited);
    }
    let (lat, lon) = (coord(&params, "lat")?, coord(&params, "lon")?);
    ensure_valid(lat, lon)?;
    let me = gender(params.get("gender").map_or("", String::as_str), "gender")?;
    let wanted: Vec<String> = genders(&params)?
        .into_iter()
        .map(|g| g.to_value())
        .collect();

    let stmt = Statement::from_sql_and_values(
        DbBackend::Postgres,
        SQL,
        [
            lat.into(),
            lon.into(),
            me.to_value().into(),
            wanted.into(),
            AREA_STALE_SECS.into(),
            MAX_DISTANCE_M.into(),
        ],
    );
    let mut rows = PreviewRow::find_by_statement(stmt).all(&state.db).await?;
    sort_for_display(&mut rows);
    let total = rows.len();
    let items = rows
        .into_iter()
        .take(CAP)
        .map(|r| {
            Ok(PreviewProfile {
                distance_band: r.area_id.is_none().then(|| distance_band(r.distance_m)),
                area: r.area_id.zip(r.area_name).map(AreaRef::from),
                gender: Gender::try_from_value(&r.gender)?,
                photo_url: media_url(&r.file_name),
                display_name: r.display_name,
                age: r.age,
            })
        })
        .collect::<Result<_, AppError>>()?;
    Ok(Json(Preview { total, items }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn row(id: u128, area: bool, distance_m: f64, age_min: i64) -> PreviewRow {
        PreviewRow {
            user_id: Uuid::from_u128(id),
            starts_at: (Utc::now() - chrono::Duration::minutes(age_min)).fixed_offset(),
            distance_m,
            display_name: String::new(),
            gender: "female".into(),
            age: 30,
            area_id: area.then(Uuid::nil),
            area_name: area.then(String::new),
            file_name: String::new(),
        }
    }

    #[test]
    fn order_is_area_then_band_then_newest_then_id() {
        let mut rows = vec![
            row(1, false, 100.0, 0),
            row(2, false, 3000.0, 0),
            row(3, true, 900.0, 20),
            row(4, false, 150.0, 30),
            row(5, true, 50.0, 5),
            row(6, false, 1800.0, 10),
            row(7, false, 1500.0, 10),
        ];
        // Same band and instant: only the id can break the tie.
        rows[6].starts_at = rows[5].starts_at;
        sort_for_display(&mut rows);
        let order: Vec<u128> = rows.iter().map(|r| r.user_id.as_u128()).collect();
        assert_eq!(order, [5, 3, 1, 4, 6, 7, 2]);
    }

    #[test]
    fn genders_parses_a_comma_list_and_refuses_unknown_values() {
        let params = |v: &str| HashMap::from([("genders".to_owned(), v.to_owned())]);
        assert!(genders(&HashMap::new()).expect("missing").is_empty());
        assert!(genders(&params("")).expect("empty").is_empty());
        assert_eq!(
            genders(&params("female,other")).expect("list"),
            [Gender::Female, Gender::Other]
        );
        assert!(genders(&params("female,robot")).is_err());
        assert!(gender("", "gender").is_err());
    }
}
