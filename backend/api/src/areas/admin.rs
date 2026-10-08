//! `/admin/areas`: admins curate the areas users can open a window in.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::Utc;
use entity::{AreaKind, area};
use sea_orm::{ActiveModelTrait, EntityTrait, QueryOrder, Set, SqlErr};
use serde::Deserialize;
use uuid::Uuid;

use super::AreaDto;
use crate::auth::AdminUser;
use crate::discovery::geo::ensure_valid;
use crate::error::{AppError, AppJson, parse_id};
use crate::state::AppState;

const NAME_MAX: usize = 80;
const RADIUS: std::ops::RangeInclusive<i32> = 50..=5000;

#[derive(Deserialize)]
pub struct AreaBody {
    name: String,
    kind: AreaKind,
    lat: f64,
    lon: f64,
    radius_m: i32,
    active: Option<bool>,
}

/// The checked fields of an [`AreaBody`]; `name` trimmed.
#[derive(Debug, PartialEq)]
struct Valid {
    name: String,
    kind: AreaKind,
    lat: f64,
    lon: f64,
    radius_m: i32,
}

fn validate(b: &AreaBody) -> Result<Valid, AppError> {
    let name = b.name.trim();
    if name.is_empty() || name.chars().count() > NAME_MAX {
        return Err(AppError::validation("name must be 1–80 characters"));
    }
    if !RADIUS.contains(&b.radius_m) {
        return Err(AppError::validation("radius_m must be within 50..5000"));
    }
    ensure_valid(b.lat, b.lon)?;
    Ok(Valid {
        name: name.to_owned(),
        kind: b.kind,
        lat: b.lat,
        lon: b.lon,
        radius_m: b.radius_m,
    })
}

fn apply(row: &mut area::ActiveModel, v: Valid) {
    row.name = Set(v.name);
    row.kind = Set(v.kind);
    row.lat = Set(v.lat);
    row.lon = Set(v.lon);
    row.radius_m = Set(v.radius_m);
}

pub async fn list(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<Vec<AreaDto>>, AppError> {
    let all = area::Entity::find()
        .order_by_desc(area::Column::Active)
        .order_by_asc(area::Column::Name)
        .order_by_asc(area::Column::Id)
        .all(&state.db)
        .await?;
    Ok(Json(all.into_iter().map(Into::into).collect()))
}

pub async fn create(
    State(state): State<AppState>,
    _admin: AdminUser,
    AppJson(body): AppJson<AreaBody>,
) -> Result<(StatusCode, Json<AreaDto>), AppError> {
    let valid = validate(&body)?;
    let mut row = area::ActiveModel {
        id: Set(Uuid::new_v4()),
        active: Set(body.active.unwrap_or(true)),
        created_at: Set(Utc::now().fixed_offset()),
        ..Default::default()
    };
    apply(&mut row, valid);
    let created = row.insert(&state.db).await?;
    Ok((StatusCode::CREATED, Json(created.into())))
}

pub async fn update(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
    AppJson(body): AppJson<AreaBody>,
) -> Result<Json<AreaDto>, AppError> {
    let id = parse_id(&id)?;
    let valid = validate(&body)?;
    let active = body
        .active
        .ok_or_else(|| AppError::validation("active is required"))?;
    let existing = area::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut row: area::ActiveModel = existing.into();
    apply(&mut row, valid);
    row.active = Set(active);
    Ok(Json(row.update(&state.db).await?.into()))
}

/// The window FK has no delete action, so the database itself refuses while any window row (running or
/// not yet purged) points here — no check-then-delete race.
pub async fn remove(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let result = area::Entity::delete_by_id(parse_id(&id)?)
        .exec(&state.db)
        .await;
    match result {
        Ok(r) if r.rows_affected == 0 => Err(AppError::NotFound),
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(e) if matches!(e.sql_err(), Some(SqlErr::ForeignKeyConstraintViolation(_))) => {
            Err(AppError::AreaInUse)
        }
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(name: &str, radius_m: i32, lat: f64) -> AreaBody {
        AreaBody {
            name: name.into(),
            kind: AreaKind::Venue,
            lat,
            lon: 14.4,
            radius_m,
            active: None,
        }
    }

    #[test]
    fn name_is_trimmed_and_bounded() {
        let v = validate(&body("  Lucerna ", 200, 50.0)).expect("valid");
        assert_eq!(v.name, "Lucerna");
        assert!(validate(&body("   ", 200, 50.0)).is_err());
        assert!(validate(&body(&"ž".repeat(80), 200, 50.0)).is_ok());
        assert!(validate(&body(&"ž".repeat(81), 200, 50.0)).is_err());
    }

    #[test]
    fn radius_and_coordinates_are_checked() {
        assert!(validate(&body("a", 50, 50.0)).is_ok());
        assert!(validate(&body("a", 5000, 50.0)).is_ok());
        assert!(validate(&body("a", 49, 50.0)).is_err());
        assert!(validate(&body("a", 5001, 50.0)).is_err());
        assert!(validate(&body("a", 200, 90.5)).is_err());
        // Centres are kept exactly, not rounded like user positions.
        assert_eq!(
            validate(&body("a", 200, 50.123_456)).map(|v| v.lat).ok(),
            Some(50.123_456)
        );
    }
}
