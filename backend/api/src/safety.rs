//! Block / report endpoints plus the helpers other modules use to hide blocked users.

use std::collections::{HashMap, HashSet};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use entity::{ReportReason, block, profile, report, user};
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DbErr, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{AppError, AppJson, parse_id};
use crate::state::AppState;

const MAX_NOTE_CHARS: usize = 1000;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/blocks", get(list_blocks).post(create_block))
        .route("/blocks/{user_id}", delete(delete_block))
        .route("/reports", axum::routing::post(create_report))
}

/// True when either user has blocked the other, or either is banned: every block-aware call
/// site hides banned accounts the same way.
pub async fn is_blocked_between(
    db: &impl ConnectionTrait,
    a: Uuid,
    b: Uuid,
) -> Result<bool, DbErr> {
    let n = block::Entity::find()
        .filter(
            Condition::any()
                .add(
                    Condition::all()
                        .add(block::Column::BlockerId.eq(a))
                        .add(block::Column::BlockedId.eq(b)),
                )
                .add(
                    Condition::all()
                        .add(block::Column::BlockerId.eq(b))
                        .add(block::Column::BlockedId.eq(a)),
                ),
        )
        .count(db)
        .await?;
    Ok(n > 0 || !banned_among(db, [a, b]).await?.is_empty())
}

/// Everyone `me` has blocked or who has blocked `me`, plus every banned account — filter list
/// queries with this set. Bans are rare, so the banned set stays small.
pub async fn blocked_with(db: &impl ConnectionTrait, me: Uuid) -> Result<HashSet<Uuid>, DbErr> {
    let rows = block::Entity::find()
        .filter(
            Condition::any()
                .add(block::Column::BlockerId.eq(me))
                .add(block::Column::BlockedId.eq(me)),
        )
        .all(db)
        .await?;
    let mut hidden: HashSet<Uuid> = rows
        .into_iter()
        .map(|b| {
            if b.blocker_id == me {
                b.blocked_id
            } else {
                b.blocker_id
            }
        })
        .collect();
    let banned: Vec<Uuid> = user::Entity::find()
        .select_only()
        .column(user::Column::Id)
        .filter(user::Column::BannedAt.is_not_null())
        .into_tuple()
        .all(db)
        .await?;
    hidden.extend(banned);
    Ok(hidden)
}

/// The subset of `ids` whose accounts are banned.
async fn banned_among(db: &impl ConnectionTrait, ids: [Uuid; 2]) -> Result<Vec<Uuid>, DbErr> {
    user::Entity::find()
        .select_only()
        .column(user::Column::Id)
        .filter(user::Column::Id.is_in(ids))
        .filter(user::Column::BannedAt.is_not_null())
        .into_tuple()
        .all(db)
        .await
}

async fn insert_block(
    db: &impl ConnectionTrait,
    blocker: Uuid,
    blocked: Uuid,
) -> Result<(), DbErr> {
    let row = block::ActiveModel {
        blocker_id: Set(blocker),
        blocked_id: Set(blocked),
        created_at: Set(Utc::now().fixed_offset()),
    };
    block::Entity::insert(row)
        .on_conflict(
            OnConflict::columns([block::Column::BlockerId, block::Column::BlockedId])
                .do_nothing()
                .to_owned(),
        )
        .do_nothing()
        .exec(db)
        .await?;
    Ok(())
}

/// Self-targeting is a 400, a missing user a 404.
async fn check_target(db: &impl ConnectionTrait, me: Uuid, target: Uuid) -> Result<(), AppError> {
    if me == target {
        return Err(AppError::validation("cannot target yourself"));
    }
    user::Entity::find_by_id(target)
        .one(db)
        .await?
        .map(|_| ())
        .ok_or(AppError::NotFound)
}

#[derive(Serialize)]
struct BlockDto {
    user_id: Uuid,
    display_name: String,
    created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct UserIdBody {
    user_id: Uuid,
}

async fn list_blocks(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<BlockDto>>, AppError> {
    let blocks = block::Entity::find()
        .filter(block::Column::BlockerId.eq(auth.id))
        .order_by_desc(block::Column::CreatedAt)
        .all(&state.db)
        .await?;
    let names: HashMap<Uuid, String> = profile::Entity::find()
        .filter(profile::Column::UserId.is_in(blocks.iter().map(|b| b.blocked_id)))
        .all(&state.db)
        .await?
        .into_iter()
        .map(|p| (p.user_id, p.display_name))
        .collect();
    Ok(Json(
        blocks
            .into_iter()
            .map(|b| BlockDto {
                display_name: names.get(&b.blocked_id).cloned().unwrap_or_default(),
                user_id: b.blocked_id,
                created_at: b.created_at.to_utc(),
            })
            .collect(),
    ))
}

async fn create_block(
    State(state): State<AppState>,
    auth: AuthUser,
    AppJson(body): AppJson<UserIdBody>,
) -> Result<StatusCode, AppError> {
    check_target(&state.db, auth.id, body.user_id).await?;
    insert_block(&state.db, auth.id, body.user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_block(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(user_id): Path<String>,
) -> Result<StatusCode, AppError> {
    let user_id = parse_id(&user_id)?;
    block::Entity::delete_many()
        .filter(block::Column::BlockerId.eq(auth.id))
        .filter(block::Column::BlockedId.eq(user_id))
        .exec(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct ReportBody {
    user_id: Uuid,
    reason: ReportReason,
    note: Option<String>,
}

/// Trimmed note, `None` when blank; over-long is a 400.
fn clean_note(note: Option<String>) -> Result<Option<String>, AppError> {
    let note = note.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty());
    match note {
        Some(n) if n.chars().count() > MAX_NOTE_CHARS => Err(AppError::validation(format!(
            "note must be at most {MAX_NOTE_CHARS} characters"
        ))),
        other => Ok(other),
    }
}

async fn create_report(
    State(state): State<AppState>,
    auth: AuthUser,
    AppJson(body): AppJson<ReportBody>,
) -> Result<StatusCode, AppError> {
    let note = clean_note(body.note)?;
    check_target(&state.db, auth.id, body.user_id).await?;
    let txn = state.db.begin().await?;
    report::ActiveModel {
        id: Set(Uuid::new_v4()),
        reporter_id: Set(Some(auth.id)),
        reported_id: Set(body.user_id),
        reason: Set(body.reason),
        note: Set(note),
        created_at: Set(Utc::now().fixed_offset()),
        resolved_at: Set(None),
        resolved_by: Set(None),
        resolution: Set(None),
    }
    .insert(&txn)
    .await?;
    insert_block(&txn, auth.id, body.user_id).await?;
    txn.commit().await?;
    Ok(StatusCode::CREATED)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_is_trimmed_and_limited() {
        assert_eq!(clean_note(None).expect("ok"), None);
        assert_eq!(clean_note(Some("  ".into())).expect("ok"), None);
        assert_eq!(
            clean_note(Some(" hi ".into())).expect("ok"),
            Some("hi".into())
        );
        assert!(clean_note(Some("x".repeat(1000))).is_ok());
        assert!(clean_note(Some("x".repeat(1001))).is_err());
    }
}
