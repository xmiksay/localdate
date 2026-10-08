//! `POST /me/location`: moves the active window, and ends an area window that left its area.

use anyhow::anyhow;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use entity::{area, visibility_window as vw};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set, TransactionTrait,
};
use serde::Deserialize;
use uuid::Uuid;

use super::geo::exit_margin;
use super::window::{self, check_coords};
use crate::areas;
use crate::auth::AuthUser;
use crate::error::{AppError, AppJson};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct LocationBody {
    lat: f64,
    lon: f64,
    /// Radius of the fix's uncertainty in metres, as the browser reports it.
    accuracy: Option<f64>,
}

fn check_accuracy(accuracy: Option<f64>) -> Result<Option<f64>, AppError> {
    match accuracy {
        Some(a) if !a.is_finite() || a < 0.0 => Err(AppError::validation(
            "accuracy must be a non-negative number",
        )),
        a => Ok(a),
    }
}

/// Timed windows: a single conditional write, so coordinates never land on an ended window.
/// Returns whether a timed window was updated.
async fn move_timed(
    db: &impl ConnectionTrait,
    user: Uuid,
    (lat, lon): (f64, f64),
    now: DateTime<Utc>,
) -> Result<bool, AppError> {
    let now = now.fixed_offset();
    let updated = vw::Entity::update_many()
        .col_expr(vw::Column::Lat, Some(lat).into())
        .col_expr(vw::Column::Lon, Some(lon).into())
        .col_expr(vw::Column::LocationUpdatedAt, now.into())
        .filter(vw::Column::UserId.eq(user))
        .filter(vw::Column::EndedAt.is_null())
        .filter(vw::Column::EndsAt.gt(now))
        .filter(vw::Column::AreaId.is_null())
        .exec(db)
        .await?
        .rows_affected;
    Ok(updated > 0)
}

pub async fn update_location(
    State(state): State<AppState>,
    auth: AuthUser,
    AppJson(body): AppJson<LocationBody>,
) -> Result<StatusCode, AppError> {
    let rounded = check_coords(body.lat, body.lon)?;
    let margin = exit_margin(check_accuracy(body.accuracy)?);
    let now = Utc::now();
    if move_timed(&state.db, auth.id, rounded, now).await? {
        return Ok(StatusCode::NO_CONTENT);
    }

    let txn = state.db.begin().await?;
    // Area windows: the row lock keeps the leave check and the write (or end) on the same window.
    let w = window::active_locked(&txn, auth.id)
        .await?
        .ok_or(AppError::NoActiveWindow)?;
    if let Some(area_id) = w.area_id {
        let a = area::Entity::find_by_id(area_id)
            .one(&txn)
            .await?
            .ok_or_else(|| anyhow!("area {area_id} of window {} is missing", w.id))?;
        if areas::circle(&a).left(body.lat, body.lon, margin) {
            window::end(&txn, w, now).await?;
            txn.commit().await?;
            return Err(AppError::LeftArea);
        }
    }
    let mut row: vw::ActiveModel = w.into();
    row.lat = Set(Some(rounded.0));
    row.lon = Set(Some(rounded.1));
    row.location_updated_at = Set(now.fixed_offset());
    row.update(&txn).await?;
    txn.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accuracy_must_be_a_non_negative_number() {
        assert_eq!(check_accuracy(None).ok(), Some(None));
        assert_eq!(check_accuracy(Some(0.0)).ok(), Some(Some(0.0)));
        for bad in [-1.0, f64::NAN, f64::INFINITY] {
            assert!(matches!(
                check_accuracy(Some(bad)),
                Err(AppError::Validation(_))
            ));
        }
    }
}
