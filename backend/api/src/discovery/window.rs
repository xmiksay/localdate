//! Visibility window lifecycle and location updates.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use entity::{WindowKind, area, filter, photo, profile, visibility_window as vw, wave};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter,
    QuerySelect, Set, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::duration::{Length, MAX_AHEAD, Until, check_minutes};
use super::geo::{ensure_valid, round_coord};
use crate::areas::{self, AreaRef};
use crate::auth::ActingUser;
use crate::auth::extractor::lock_unbanned;
use crate::error::{AppError, AppJson};
use crate::state::AppState;

pub const MAX_WAVES_PER_WINDOW: u64 = 20;

#[derive(Serialize)]
pub struct WindowDto {
    id: Uuid,
    kind: WindowKind,
    area: Option<AreaRef>,
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
        let area = match w.area_id {
            Some(id) => area::Entity::find_by_id(id)
                .one(db)
                .await?
                .map(AreaRef::from),
            None => None,
        };
        Ok(Self {
            id: w.id,
            kind: w.kind,
            area,
            starts_at: w.starts_at.to_utc(),
            ends_at: w.ends_at.to_utc(),
            waves_left: MAX_WAVES_PER_WINDOW.saturating_sub(sent),
        })
    }
}

#[derive(Deserialize)]
pub struct StartBody {
    kind: Option<WindowKind>,
    area_id: Option<Uuid>,
    minutes: Option<i64>,
    until: Option<Until>,
    tz: Option<String>,
    lat: f64,
    lon: f64,
}

#[derive(Deserialize)]
pub struct ExtendBody {
    extend_minutes: i64,
}

/// The window kind and, for an area window, its area; `kind` defaults to timed, which takes no area.
fn requested_kind(
    kind: Option<WindowKind>,
    area_id: Option<Uuid>,
) -> Result<(WindowKind, Option<Uuid>), AppError> {
    match (kind.unwrap_or(WindowKind::Timed), area_id) {
        (WindowKind::Timed, None) => Ok((WindowKind::Timed, None)),
        (WindowKind::Area, Some(id)) => Ok((WindowKind::Area, Some(id))),
        (WindowKind::Timed, Some(_)) => Err(AppError::validation(
            "area_id is only allowed with kind 'area'",
        )),
        (WindowKind::Area, None) => {
            Err(AppError::validation("area_id is required for kind 'area'"))
        }
    }
}

pub(crate) fn check_coords(lat: f64, lon: f64) -> Result<(f64, f64), AppError> {
    ensure_valid(lat, lon)?;
    Ok((round_coord(lat), round_coord(lon)))
}

/// Extension never reaches further than 12 h from `now`.
fn extended_end(ends_at: DateTime<Utc>, now: DateTime<Utc>, extra: Duration) -> DateTime<Utc> {
    (ends_at + extra).min(now + MAX_AHEAD)
}

/// Closes a window, forgets where it was and drops the waves it sent.
pub(crate) async fn end(
    db: &impl ConnectionTrait,
    w: vw::Model,
    now: DateTime<Utc>,
) -> Result<(), AppError> {
    wave::Entity::delete_many()
        .filter(wave::Column::WindowId.eq(w.id))
        .exec(db)
        .await?;
    let mut row: vw::ActiveModel = w.into();
    row.ended_at = Set(Some(now.fixed_offset()));
    row.lat = Set(None);
    row.lon = Set(None);
    row.update(db).await?;
    Ok(())
}

/// The caller's open window row, expired or not.
pub(crate) async fn open_window(
    db: &impl ConnectionTrait,
    user: Uuid,
) -> Result<Option<vw::Model>, AppError> {
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
    auth: ActingUser,
) -> Result<Json<Option<WindowDto>>, AppError> {
    let Some(w) = active(&state.db, auth.id).await? else {
        return Ok(Json(None));
    };
    Ok(Json(Some(WindowDto::build(&state.db, &w).await?)))
}

pub async fn start_window(
    State(state): State<AppState>,
    auth: ActingUser,
    AppJson(body): AppJson<StartBody>,
) -> Result<(StatusCode, Json<WindowDto>), AppError> {
    let length = Length::requested(body.minutes, body.until, body.tz.as_deref())?;
    let (lat, lon) = check_coords(body.lat, body.lon)?;
    let (kind, area_id) = requested_kind(body.kind, body.area_id)?;

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

    let txn = state.db.begin().await?;
    lock_unbanned(&txn, auth.id).await?;
    // After the lock, so a wait on it cannot shorten the window or skew it from `starts_at`.
    let now = Utc::now();
    let ends_at = length.ends_at(now)?;
    if let Some(id) = area_id {
        // Unrounded: rounding could move a point at the edge in or out of the circle.
        let a = areas::lock_active(&txn, id).await?;
        if !areas::circle(&a).contains(body.lat, body.lon) {
            return Err(AppError::OutsideArea);
        }
    }
    if let Some(old) = open_window(&txn, auth.id).await? {
        end(&txn, old, now).await?;
    }
    let row = vw::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(auth.id),
        kind: Set(kind),
        area_id: Set(area_id),
        lat: Set(Some(lat)),
        lon: Set(Some(lon)),
        location_updated_at: Set(now.fixed_offset()),
        starts_at: Set(now.fixed_offset()),
        ends_at: Set(ends_at.fixed_offset()),
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
    auth: ActingUser,
    AppJson(body): AppJson<ExtendBody>,
) -> Result<Json<WindowDto>, AppError> {
    let extra = check_minutes("extend_minutes", body.extend_minutes)?;
    let txn = state.db.begin().await?;
    // Locked so a concurrent close (cleanup tick, new window) cannot interleave with the extend.
    let w = active_locked(&txn, auth.id)
        .await?
        .ok_or(AppError::NoActiveWindow)?;
    let new_end = extended_end(w.ends_at.to_utc(), Utc::now(), extra).fixed_offset();

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
    auth: ActingUser,
) -> Result<StatusCode, AppError> {
    let txn = state.db.begin().await?;
    if let Some(w) = open_window(&txn, auth.id).await? {
        end(&txn, w, Utc::now()).await?;
    }
    txn.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_and_area_id_must_agree() {
        use WindowKind::{Area, Timed};
        let id = Uuid::from_u128(7);
        assert_eq!(requested_kind(None, None).ok(), Some((Timed, None)));
        assert_eq!(requested_kind(Some(Timed), None).ok(), Some((Timed, None)));
        assert_eq!(
            requested_kind(Some(Area), Some(id)).ok(),
            Some((Area, Some(id)))
        );
        for (kind, area) in [(None, Some(id)), (Some(Area), None)] {
            assert!(matches!(
                requested_kind(kind, area),
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
            now + MAX_AHEAD
        );
    }
}
