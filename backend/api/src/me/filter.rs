use axum::Json;
use axum::extract::State;
use entity::{Gender, Reason, filter};
use sea_orm::sea_query::OnConflict;
use sea_orm::{ConnectionTrait, EntityTrait, Set};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::discovery::duration::check_minutes;
use crate::error::{AppError, AppJson};
use crate::state::AppState;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct FilterDto {
    pub max_distance_m: i32,
    pub genders: Vec<Gender>,
    pub age_min: i16,
    pub age_max: i16,
    pub reasons: Vec<Reason>,
    pub default_window_minutes: i16,
}

impl Default for FilterDto {
    fn default() -> Self {
        Self {
            max_distance_m: 2000,
            genders: vec![],
            age_min: 18,
            age_max: 99,
            reasons: vec![Reason::Date, Reason::Meet],
            default_window_minutes: 60,
        }
    }
}

impl From<filter::Model> for FilterDto {
    fn from(m: filter::Model) -> Self {
        Self {
            max_distance_m: m.max_distance_m,
            genders: m.genders,
            age_min: m.age_min,
            age_max: m.age_max,
            reasons: m.reasons,
            default_window_minutes: m.default_window_minutes,
        }
    }
}

fn dedup<T: PartialEq>(items: Vec<T>) -> Vec<T> {
    let mut out: Vec<T> = Vec::with_capacity(items.len());
    for item in items {
        if !out.contains(&item) {
            out.push(item);
        }
    }
    out
}

/// Checks ranges and returns the filter with genders/reasons de-duplicated.
fn validate(f: FilterDto) -> Result<FilterDto, AppError> {
    if !(200..=10_000).contains(&f.max_distance_m) {
        return Err(AppError::validation("max_distance_m must be 200-10000"));
    }
    if !(18..=99).contains(&f.age_min) || !(18..=99).contains(&f.age_max) || f.age_min > f.age_max {
        return Err(AppError::validation(
            "ages must be within 18-99 and age_min <= age_max",
        ));
    }
    if f.reasons.is_empty() {
        return Err(AppError::validation("reasons must not be empty"));
    }
    check_minutes("default_window_minutes", f.default_window_minutes.into())?;
    Ok(FilterDto {
        genders: dedup(f.genders),
        reasons: dedup(f.reasons),
        ..f
    })
}

pub async fn load(
    db: &impl ConnectionTrait,
    user_id: Uuid,
) -> Result<Option<filter::Model>, AppError> {
    Ok(filter::Entity::find_by_id(user_id).one(db).await?)
}

pub async fn get_filter(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<FilterDto>, AppError> {
    let dto = load(&state.db, auth.id)
        .await?
        .map_or_else(FilterDto::default, Into::into);
    Ok(Json(dto))
}

pub async fn put_filter(
    State(state): State<AppState>,
    auth: AuthUser,
    AppJson(body): AppJson<FilterDto>,
) -> Result<Json<FilterDto>, AppError> {
    let f = validate(body)?;
    let row = filter::ActiveModel {
        user_id: Set(auth.id),
        max_distance_m: Set(f.max_distance_m),
        genders: Set(f.genders.clone()),
        age_min: Set(f.age_min),
        age_max: Set(f.age_max),
        reasons: Set(f.reasons.clone()),
        default_window_minutes: Set(f.default_window_minutes),
    };
    filter::Entity::insert(row)
        .on_conflict(
            OnConflict::column(filter::Column::UserId)
                .update_columns([
                    filter::Column::MaxDistanceM,
                    filter::Column::Genders,
                    filter::Column::AgeMin,
                    filter::Column::AgeMax,
                    filter::Column::Reasons,
                    filter::Column::DefaultWindowMinutes,
                ])
                .to_owned(),
        )
        .exec(&state.db)
        .await?;
    Ok(Json(f))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(edit: impl FnOnce(&mut FilterDto)) -> FilterDto {
        let mut f = FilterDto::default();
        edit(&mut f);
        f
    }

    #[test]
    fn defaults_are_valid() {
        assert!(validate(FilterDto::default()).is_ok());
    }

    #[test]
    fn rejects_out_of_range_values() {
        let bad = [
            with(|f| f.max_distance_m = 199),
            with(|f| f.max_distance_m = 10_001),
            with(|f| f.age_min = 17),
            with(|f| f.age_max = 100),
            with(|f| {
                f.age_min = 40;
                f.age_max = 30;
            }),
            with(|f| f.reasons = vec![]),
            with(|f| f.default_window_minutes = 45),
        ];
        for f in bad {
            assert!(matches!(validate(f), Err(AppError::Validation(_))));
        }
    }

    #[test]
    fn accepts_boundaries_and_dedups() {
        let f = validate(with(|f| {
            f.max_distance_m = 200;
            f.age_min = 99;
            f.age_max = 99;
            f.default_window_minutes = 240;
            f.genders = vec![Gender::Male, Gender::Male, Gender::Female];
            f.reasons = vec![Reason::Meet, Reason::Meet];
        }))
        .expect("valid");
        assert_eq!(f.genders, vec![Gender::Male, Gender::Female]);
        assert_eq!(f.reasons, vec![Reason::Meet]);
    }
}
