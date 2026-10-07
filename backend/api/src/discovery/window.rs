//! Visibility window lifecycle and location updates.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use entity::{WindowKind, filter, photo, profile, visibility_window as vw, wave};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter,
    QuerySelect, Set, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::geo::{round_coord, valid_coords};
use crate::auth::AuthUser;
use crate::error::{AppError, AppJson};
use crate::state::AppState;

pub const MAX_WAVES_PER_WINDOW: u64 = 20;
const MINUTES: [i64; 4] = [30, 60, 120, 240];
const MAX_AHEAD: Duration = Duration::hours(12);

#[derive(Serialize)]
pub struct WindowDto {
    id: Uuid,
    kind: WindowKind,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    waves_left: u64,
}

impl WindowDto {
    async fn build(db: &impl ConnectionTrait, w: &vw::Model) -> Result<Self, AppError> {
        let sent = wave::Entity::find()
            .filter(wave::Column::WindowId.eq(w.id))
            .count(db)
            .await?;
        Ok(Self {
            id: w.id,
            kind: w.kind,
            starts_at: w.starts_at.to_utc(),
            ends_at: w.ends_at.to_utc(),
            waves_left: MAX_WAVES_PER_WINDOW.saturating_sub(sent),
        })
    }
}

#[derive(Deserialize)]
pub struct StartBody {
    minutes: i64,
    lat: f64,
    lon: f64,
}

#[derive(Deserialize)]
pub struct ExtendBody {
    extend_minutes: i64,
}

#[derive(Deserialize)]
pub struct LocationBody {
    lat: f64,
    lon: f64,
}

fn check_minutes(field: &str, minutes: i64) -> Result<Duration, AppError> {
    if MINUTES.contains(&minutes) {
        Ok(Duration::minutes(minutes))
    } else {
        Err(AppError::validation(format!(
            "{field} must be 30, 60, 120 or 240"
        )))
    }
}

fn check_coords(lat: f64, lon: f64) -> Result<(f64, f64), AppError> {
    if valid_coords(lat, lon) {
        Ok((round_coord(lat), round_coord(lon)))
    } else {
        Err(AppError::validation(
            "lat must be within -90..90 and lon within -180..180",
        ))
    }
}

/// Extension never reaches further than 12 h from `now`.
fn extended_end(ends_at: DateTime<Utc>, now: DateTime<Utc>, extra: Duration) -> DateTime<Utc> {
    (ends_at + extra).min(now + MAX_AHEAD)
}

/// Closes a window and drops the waves it sent.
async fn end(db: &impl ConnectionTrait, w: vw::Model, now: DateTime<Utc>) -> Result<(), AppError> {
    wave::Entity::delete_many()
        .filter(wave::Column::WindowId.eq(w.id))
        .exec(db)
        .await?;
    let mut row: vw::ActiveModel = w.into();
    row.ended_at = Set(Some(now.fixed_offset()));
    row.update(db).await?;
    Ok(())
}

/// The caller's open window row, expired or not.
async fn open_window(db: &impl ConnectionTrait, user: Uuid) -> Result<Option<vw::Model>, AppError> {
    Ok(vw::Entity::find()
        .filter(vw::Column::UserId.eq(user))
        .filter(vw::Column::EndedAt.is_null())
        .one(db)
        .await?)
}

/// The caller's active window. One that ran out is closed here, lazily.
pub async fn active(db: &impl ConnectionTrait, user: Uuid) -> Result<Option<vw::Model>, AppError> {
    let Some(w) = open_window(db, user).await? else {
        return Ok(None);
    };
    let now = Utc::now();
    if w.ends_at.to_utc() > now {
        return Ok(Some(w));
    }
    end(db, w, now).await?;
    Ok(None)
}

/// Like [`active`] but takes a row lock so concurrent waves serialize on the window.
pub async fn active_locked(
    db: &impl ConnectionTrait,
    user: Uuid,
) -> Result<Option<vw::Model>, AppError> {
    let locked = vw::Entity::find()
        .filter(vw::Column::UserId.eq(user))
        .filter(vw::Column::EndedAt.is_null())
        .lock_exclusive()
        .one(db)
        .await?;
    Ok(locked.filter(|w| w.ends_at.to_utc() > Utc::now()))
}

pub async fn get_window(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Option<WindowDto>>, AppError> {
    let Some(w) = active(&state.db, auth.id).await? else {
        return Ok(Json(None));
    };
    Ok(Json(Some(WindowDto::build(&state.db, &w).await?)))
}

pub async fn start_window(
    State(state): State<AppState>,
    auth: AuthUser,
    AppJson(body): AppJson<StartBody>,
) -> Result<(StatusCode, Json<WindowDto>), AppError> {
    let length = check_minutes("minutes", body.minutes)?;
    let (lat, lon) = check_coords(body.lat, body.lon)?;

    let has_profile = profile::Entity::find_by_id(auth.id)
        .count(&state.db)
        .await?
        > 0;
    let has_filter = filter::Entity::find_by_id(auth.id).count(&state.db).await? > 0;
    let has_photo = photo::Entity::find()
        .filter(photo::Column::UserId.eq(auth.id))
        .count(&state.db)
        .await?
        > 0;
    if !(has_profile && has_filter && has_photo) {
        return Err(AppError::ProfileIncomplete);
    }

    let now = Utc::now();
    let txn = state.db.begin().await?;
    if let Some(old) = open_window(&txn, auth.id).await? {
        end(&txn, old, now).await?;
    }
    let row = vw::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(auth.id),
        kind: Set(WindowKind::Timed),
        lat: Set(lat),
        lon: Set(lon),
        location_updated_at: Set(now.fixed_offset()),
        starts_at: Set(now.fixed_offset()),
        ends_at: Set((now + length).fixed_offset()),
        ended_at: Set(None),
    }
    .insert(&txn)
    .await?;
    txn.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(WindowDto::build(&state.db, &row).await?),
    ))
}

pub async fn extend_window(
    State(state): State<AppState>,
    auth: AuthUser,
    AppJson(body): AppJson<ExtendBody>,
) -> Result<Json<WindowDto>, AppError> {
    let extra = check_minutes("extend_minutes", body.extend_minutes)?;
    let w = active(&state.db, auth.id)
        .await?
        .ok_or(AppError::NoActiveWindow)?;
    let new_end = extended_end(w.ends_at.to_utc(), Utc::now(), extra).fixed_offset();

    let txn = state.db.begin().await?;
    // Waves live as long as the window that sent them.
    wave::Entity::update_many()
        .col_expr(wave::Column::ExpiresAt, new_end.into())
        .filter(wave::Column::WindowId.eq(w.id))
        .exec(&txn)
        .await?;
    let mut row: vw::ActiveModel = w.into();
    row.ends_at = Set(new_end);
    let updated = row.update(&txn).await?;
    txn.commit().await?;
    Ok(Json(WindowDto::build(&state.db, &updated).await?))
}

pub async fn end_window(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<StatusCode, AppError> {
    let txn = state.db.begin().await?;
    if let Some(w) = open_window(&txn, auth.id).await? {
        end(&txn, w, Utc::now()).await?;
    }
    txn.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn update_location(
    State(state): State<AppState>,
    auth: AuthUser,
    AppJson(body): AppJson<LocationBody>,
) -> Result<StatusCode, AppError> {
    let (lat, lon) = check_coords(body.lat, body.lon)?;
    let w = active(&state.db, auth.id)
        .await?
        .ok_or(AppError::NoActiveWindow)?;
    let mut row: vw::ActiveModel = w.into();
    row.lat = Set(lat);
    row.lon = Set(lon);
    row.location_updated_at = Set(Utc::now().fixed_offset());
    row.update(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minutes_are_limited_to_presets() {
        for m in MINUTES {
            assert!(check_minutes("minutes", m).is_ok());
        }
        for m in [0, 45, -30, 480] {
            assert!(matches!(
                check_minutes("minutes", m),
                Err(AppError::Validation(_))
            ));
        }
    }

    #[test]
    fn coords_are_validated_and_rounded() {
        assert_eq!(
            check_coords(50.123_456, 14.987_654).ok(),
            Some((50.123, 14.988))
        );
        assert!(check_coords(91.0, 0.0).is_err());
        assert!(check_coords(0.0, 181.0).is_err());
    }

    #[test]
    fn extension_is_capped_at_twelve_hours_from_now() {
        let now = Utc::now();
        let soon = now + Duration::minutes(10);
        assert_eq!(
            extended_end(soon, now, Duration::minutes(60)),
            soon + Duration::minutes(60)
        );
        let late = now + Duration::hours(11);
        assert_eq!(
            extended_end(late, now, Duration::minutes(240)),
            now + Duration::hours(12)
        );
    }
}
