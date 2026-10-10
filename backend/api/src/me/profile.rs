use axum::Json;
use axum::extract::State;
use chrono::{Datelike, NaiveDate, Utc};
use entity::{Gender, interest, profile, user_interest};
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set,
    TransactionTrait,
};
use serde::{Deserialize, Serialize};

use super::photos::{self, PhotoDto};
use crate::auth::ActingUser;
use crate::error::{AppError, AppJson};
use crate::interests::InterestDto;
use crate::state::AppState;

const MAX_INTERESTS: usize = 10;
const MIN_AGE: i32 = 18;

#[derive(Serialize)]
pub struct ProfileDto {
    display_name: String,
    birth_date: NaiveDate,
    age: i32,
    gender: Gender,
    bio: String,
    interests: Vec<InterestDto>,
    photos: Vec<PhotoDto>,
}

#[derive(Deserialize)]
pub struct ProfileBody {
    pub display_name: String,
    pub birth_date: NaiveDate,
    pub gender: Gender,
    pub bio: String,
    pub interest_ids: Vec<i32>,
}

/// Completed years on `today`. A Feb 29 birthday counts from Mar 1 in common years.
pub fn age_on(birth_date: NaiveDate, today: NaiveDate) -> i32 {
    let years = today.year() - birth_date.year();
    if (today.month(), today.day()) < (birth_date.month(), birth_date.day()) {
        years - 1
    } else {
        years
    }
}

/// Field validation (400) then the 18+ rule (422); returns the trimmed display name.
fn validate(body: &ProfileBody, today: NaiveDate) -> Result<String, AppError> {
    let name = body.display_name.trim();
    if !(1..=40).contains(&name.chars().count()) {
        return Err(AppError::validation("display_name must be 1-40 characters"));
    }
    if body.bio.chars().count() > 500 {
        return Err(AppError::validation("bio must be at most 500 characters"));
    }
    if body.interest_ids.len() > MAX_INTERESTS {
        return Err(AppError::validation(format!(
            "at most {MAX_INTERESTS} interests"
        )));
    }
    let mut ids = body.interest_ids.clone();
    ids.sort_unstable();
    ids.dedup();
    if ids.len() != body.interest_ids.len() {
        return Err(AppError::validation("interest_ids must be distinct"));
    }
    if age_on(body.birth_date, today) < MIN_AGE {
        return Err(AppError::Underage);
    }
    Ok(name.to_owned())
}

pub async fn load(
    db: &impl ConnectionTrait,
    user_id: uuid::Uuid,
) -> Result<Option<ProfileDto>, AppError> {
    let Some(p) = profile::Entity::find_by_id(user_id).one(db).await? else {
        return Ok(None);
    };
    let ids: Vec<i32> = user_interest::Entity::find()
        .filter(user_interest::Column::UserId.eq(user_id))
        .all(db)
        .await?
        .into_iter()
        .map(|ui| ui.interest_id)
        .collect();
    let interests = interest::Entity::find()
        .filter(interest::Column::Id.is_in(ids))
        .order_by_asc(interest::Column::Id)
        .all(db)
        .await?;
    Ok(Some(ProfileDto {
        age: age_on(p.birth_date, Utc::now().date_naive()),
        display_name: p.display_name,
        birth_date: p.birth_date,
        gender: p.gender,
        bio: p.bio,
        interests: interests.into_iter().map(Into::into).collect(),
        photos: photos::list(db, user_id).await?,
    }))
}

pub async fn put_profile(
    State(state): State<AppState>,
    auth: ActingUser,
    AppJson(body): AppJson<ProfileBody>,
) -> Result<Json<ProfileDto>, AppError> {
    save(&state.db, auth.id, body).await?;
    let dto = load(&state.db, auth.id).await?.ok_or(AppError::Internal)?;
    Ok(Json(dto))
}

/// Validates and writes the profile with its interests, in one transaction.
pub async fn save(
    db: &impl TransactionTrait,
    user_id: uuid::Uuid,
    body: ProfileBody,
) -> Result<(), AppError> {
    let display_name = validate(&body, Utc::now().date_naive())?;

    let txn = db.begin().await?;
    let found = interest::Entity::find()
        .filter(interest::Column::Id.is_in(body.interest_ids.clone()))
        .count(&txn)
        .await?;
    if found != body.interest_ids.len() as u64 {
        return Err(AppError::validation("unknown interest id"));
    }
    let row = profile::ActiveModel {
        user_id: Set(user_id),
        display_name: Set(display_name),
        birth_date: Set(body.birth_date),
        gender: Set(body.gender),
        bio: Set(body.bio),
        updated_at: Set(Utc::now().fixed_offset()),
    };
    profile::Entity::insert(row)
        .on_conflict(
            OnConflict::column(profile::Column::UserId)
                .update_columns([
                    profile::Column::DisplayName,
                    profile::Column::BirthDate,
                    profile::Column::Gender,
                    profile::Column::Bio,
                    profile::Column::UpdatedAt,
                ])
                .to_owned(),
        )
        .exec(&txn)
        .await?;
    user_interest::Entity::delete_many()
        .filter(user_interest::Column::UserId.eq(user_id))
        .exec(&txn)
        .await?;
    if !body.interest_ids.is_empty() {
        user_interest::Entity::insert_many(body.interest_ids.iter().map(|&interest_id| {
            user_interest::ActiveModel {
                user_id: Set(user_id),
                interest_id: Set(interest_id),
            }
        }))
        .exec(&txn)
        .await?;
    }
    txn.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).expect("valid date")
    }

    fn body(name: &str, bio: &str, interests: Vec<i32>, birth: NaiveDate) -> ProfileBody {
        ProfileBody {
            display_name: name.into(),
            birth_date: birth,
            gender: Gender::Other,
            bio: bio.into(),
            interest_ids: interests,
        }
    }

    #[test]
    fn age_counts_completed_years() {
        assert_eq!(age_on(d(2000, 6, 15), d(2026, 6, 14)), 25);
        assert_eq!(age_on(d(2000, 6, 15), d(2026, 6, 15)), 26);
        assert_eq!(age_on(d(2000, 12, 31), d(2026, 1, 1)), 25);
    }

    #[test]
    fn age_of_leap_day_birthday() {
        let born = d(2008, 2, 29);
        assert_eq!(age_on(born, d(2026, 2, 28)), 17);
        assert_eq!(age_on(born, d(2026, 3, 1)), 18);
        assert_eq!(age_on(born, d(2028, 2, 29)), 20);
    }

    #[test]
    fn eighteenth_birthday_edge() {
        let today = d(2026, 10, 7);
        let ok = body("A", "", vec![], d(2008, 10, 7));
        let day_short = body("A", "", vec![], d(2008, 10, 8));
        assert!(validate(&ok, today).is_ok());
        assert!(matches!(
            validate(&day_short, today),
            Err(AppError::Underage)
        ));
    }

    #[test]
    fn validates_fields() {
        let today = d(2026, 10, 7);
        let birth = d(1990, 1, 1);
        let bad = [
            body("   ", "", vec![], birth),
            body(&"x".repeat(41), "", vec![], birth),
            body("A", &"x".repeat(501), vec![], birth),
            body("A", "", (1..=11).collect(), birth),
            body("A", "", vec![3, 3], birth),
        ];
        for b in bad {
            assert!(matches!(validate(&b, today), Err(AppError::Validation(_))));
        }
        let name = validate(
            &body("  Eva  ", &"x".repeat(500), (1..=10).collect(), birth),
            today,
        );
        assert_eq!(name.expect("valid"), "Eva");
    }
}
