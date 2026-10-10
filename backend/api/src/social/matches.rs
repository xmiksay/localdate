use std::collections::HashMap;

use axum::Json;
use axum::extract::State;
use chrono::{DateTime, Utc};
use entity::{matches, message, photo, profile};
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, DbBackend, EntityTrait, QueryFilter, QueryOrder, Set,
    Statement,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::messages::MessageDto;
use crate::auth::ActingUser;
use crate::error::AppError;
use crate::me::media_url;
use crate::safety::blocked_with;
use crate::state::AppState;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OtherDto {
    pub user_id: Uuid,
    pub display_name: String,
    pub photo_url: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MatchSummaryDto {
    pub match_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub other: OtherDto,
    pub last_message: Option<MessageDto>,
}

pub fn other_user(m: &matches::Model, me: Uuid) -> Uuid {
    if m.user_a == me { m.user_b } else { m.user_a }
}

/// The match of this pair, created if missing (idempotent on the pair).
pub async fn find_or_create(
    db: &impl ConnectionTrait,
    x: Uuid,
    y: Uuid,
) -> Result<matches::Model, AppError> {
    let (user_a, user_b) = if x < y { (x, y) } else { (y, x) };
    let row = matches::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_a: Set(user_a),
        user_b: Set(user_b),
        created_at: Set(Utc::now().fixed_offset()),
    };
    matches::Entity::insert(row)
        .on_conflict(
            OnConflict::columns([matches::Column::UserA, matches::Column::UserB])
                .do_nothing()
                .to_owned(),
        )
        .do_nothing()
        .exec(db)
        .await?;
    matches::Entity::find()
        .filter(matches::Column::UserA.eq(user_a))
        .filter(matches::Column::UserB.eq(user_b))
        .one(db)
        .await?
        .ok_or(AppError::Internal)
}

pub async fn find_between(
    db: &impl ConnectionTrait,
    x: Uuid,
    y: Uuid,
) -> Result<Option<matches::Model>, AppError> {
    let (user_a, user_b) = if x < y { (x, y) } else { (y, x) };
    Ok(matches::Entity::find()
        .filter(matches::Column::UserA.eq(user_a))
        .filter(matches::Column::UserB.eq(user_b))
        .one(db)
        .await?)
}

/// Summaries from `me`'s point of view, in the order of `models`; three queries regardless of size.
pub async fn summaries(
    db: &impl ConnectionTrait,
    me: Uuid,
    models: &[matches::Model],
) -> Result<Vec<MatchSummaryDto>, AppError> {
    if models.is_empty() {
        return Ok(vec![]);
    }
    let match_ids: Vec<Uuid> = models.iter().map(|m| m.id).collect();
    let other_ids: Vec<Uuid> = models.iter().map(|m| other_user(m, me)).collect();

    let names: HashMap<Uuid, String> = profile::Entity::find()
        .filter(profile::Column::UserId.is_in(other_ids.clone()))
        .all(db)
        .await?
        .into_iter()
        .map(|p| (p.user_id, p.display_name))
        .collect();
    let mut photos: HashMap<Uuid, String> = HashMap::new();
    for p in photo::Entity::find()
        .filter(photo::Column::UserId.is_in(other_ids))
        .order_by_desc(photo::Column::Position)
        .all(db)
        .await?
    {
        // Descending position, so the primary photo (0) overwrites last.
        photos.insert(p.user_id, media_url(&p.file_name));
    }
    let mut last: HashMap<Uuid, message::Model> = message::Entity::find()
        .from_raw_sql(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT DISTINCT ON (match_id) id, match_id, sender_id, body, created_at \
             FROM message WHERE match_id = ANY($1) ORDER BY match_id, created_at DESC, id DESC",
            [match_ids.into()],
        ))
        .all(db)
        .await?
        .into_iter()
        .map(|m| (m.match_id, m))
        .collect();

    Ok(models
        .iter()
        .map(|m| {
            let other = other_user(m, me);
            MatchSummaryDto {
                match_id: m.id,
                created_at: m.created_at.to_utc(),
                other: OtherDto {
                    user_id: other,
                    display_name: names.get(&other).cloned().unwrap_or_default(),
                    photo_url: photos.get(&other).cloned(),
                },
                last_message: last.remove(&m.id).map(Into::into),
            }
        })
        .collect())
}

pub async fn list(
    State(state): State<AppState>,
    auth: ActingUser,
) -> Result<Json<Vec<MatchSummaryDto>>, AppError> {
    let blocked = blocked_with(&state.db, auth.id).await?;
    let models: Vec<matches::Model> = matches::Entity::find()
        .filter(
            Condition::any()
                .add(matches::Column::UserA.eq(auth.id))
                .add(matches::Column::UserB.eq(auth.id)),
        )
        .all(&state.db)
        .await?
        .into_iter()
        .filter(|m| !blocked.contains(&other_user(m, auth.id)))
        .collect();
    let mut out = summaries(&state.db, auth.id, &models).await?;
    out.sort_by_key(|s| {
        std::cmp::Reverse(
            s.last_message
                .as_ref()
                .map_or(s.created_at, |m| m.created_at),
        )
    });
    Ok(Json(out))
}
