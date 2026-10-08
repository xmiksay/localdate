//! Predefined areas (city centre, train station, venue): `GET /areas` for the window picker and
//! admin CRUD under `/admin/areas`. Area windows themselves live in `discovery::window`.

mod admin;

use std::collections::HashMap;

use axum::Json;
use axum::Router;
use axum::extract::{Query, State};
use axum::routing::{get, put};
use chrono::{DateTime, Utc};
use entity::{AreaKind, area};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QuerySelect};
use serde::Serialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::discovery::geo::{Circle, ensure_valid};
use crate::error::AppError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/areas", get(containing))
        .route("/admin/areas", get(admin::list).post(admin::create))
        .route(
            "/admin/areas/{id}",
            put(admin::update).delete(admin::remove),
        )
}

/// Full area as the contract's `Area`; centres are public places, never a user's position.
#[derive(Serialize, Debug)]
pub struct AreaDto {
    id: Uuid,
    name: String,
    kind: AreaKind,
    lat: f64,
    lon: f64,
    radius_m: i32,
    active: bool,
    created_at: DateTime<Utc>,
}

impl From<area::Model> for AreaDto {
    fn from(a: area::Model) -> Self {
        Self {
            id: a.id,
            name: a.name,
            kind: a.kind,
            lat: a.lat,
            lon: a.lon,
            radius_m: a.radius_m,
            active: a.active,
            created_at: a.created_at.to_utc(),
        }
    }
}

/// The contract's `AreaRef`, carried by windows and nearby items.
#[derive(Serialize, Debug, Clone)]
pub struct AreaRef {
    id: Uuid,
    name: String,
}

impl From<(Uuid, String)> for AreaRef {
    fn from((id, name): (Uuid, String)) -> Self {
        Self { id, name }
    }
}

impl From<area::Model> for AreaRef {
    fn from(a: area::Model) -> Self {
        (a.id, a.name).into()
    }
}

pub fn circle(a: &area::Model) -> Circle {
    Circle {
        lat: a.lat,
        lon: a.lon,
        radius_m: f64::from(a.radius_m),
    }
}

/// An active area by id, read `FOR SHARE` so a concurrent deactivation or move waits for the
/// caller's transaction; unknown and inactive look the same to users.
pub async fn lock_active(db: &impl ConnectionTrait, id: Uuid) -> Result<area::Model, AppError> {
    area::Entity::find_by_id(id)
        .filter(area::Column::Active.eq(true))
        .lock_shared()
        .one(db)
        .await?
        .ok_or(AppError::NotFound)
}

fn coord(params: &HashMap<String, String>, key: &str) -> Result<f64, AppError> {
    params
        .get(key)
        .and_then(|v| v.parse::<f64>().ok())
        .ok_or_else(|| AppError::validation(format!("{key} must be a number")))
}

/// Active areas containing the point, nearest centre first. The table is small and
/// admin-curated, so filtering in Rust with the shared haversine beats a second SQL formula.
async fn containing(
    State(state): State<AppState>,
    _auth: AuthUser,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<AreaDto>>, AppError> {
    let (lat, lon) = (coord(&params, "lat")?, coord(&params, "lon")?);
    ensure_valid(lat, lon)?;
    let mut found: Vec<(f64, area::Model)> = area::Entity::find()
        .filter(area::Column::Active.eq(true))
        .all(&state.db)
        .await?
        .into_iter()
        .filter_map(|a| {
            let c = circle(&a);
            c.contains(lat, lon).then(|| (c.distance_m(lat, lon), a))
        })
        .collect();
    found.sort_by(|(x, _), (y, _)| x.total_cmp(y));
    Ok(Json(found.into_iter().map(|(_, a)| a.into()).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coord_params_must_be_numbers() {
        let params = HashMap::from([
            ("lat".to_owned(), "50.08".to_owned()),
            ("lon".to_owned(), "abc".to_owned()),
        ]);
        assert_eq!(coord(&params, "lat").ok(), Some(50.08));
        assert!(matches!(
            coord(&params, "lon"),
            Err(AppError::Validation(_))
        ));
        assert!(matches!(coord(&params, "x"), Err(AppError::Validation(_))));
    }
}
