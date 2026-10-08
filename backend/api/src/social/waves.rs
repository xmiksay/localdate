use axum::Json;
use axum::extract::State;
use chrono::Utc;
use entity::wave;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, EntityTrait, PaginatorTrait, QueryFilter, Set,
    TransactionTrait,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::matches::{find_between, find_or_create, summaries};
use crate::auth::AuthUser;
use crate::auth::extractor::lock_unbanned;
use crate::discovery::nearby::{self, NearbyProfile};
use crate::discovery::window::{self, MAX_WAVES_PER_WINDOW};
use crate::error::{AppError, AppJson};
use crate::state::AppState;
use crate::ws::ServerEvent;

#[derive(Deserialize)]
pub struct WaveBody {
    to_user_id: Uuid,
}

#[derive(Serialize)]
pub struct WaveResult {
    matched: bool,
    match_id: Option<Uuid>,
}

enum Outcome {
    /// Nothing new happened; the state is returned as it already was.
    Existing(WaveResult),
    Waved,
    Matched(entity::matches::Model),
}

pub async fn post_wave(
    State(state): State<AppState>,
    auth: AuthUser,
    AppJson(body): AppJson<WaveBody>,
) -> Result<Json<WaveResult>, AppError> {
    let (me, target) = (auth.id, body.to_user_id);
    let now = Utc::now().fixed_offset();

    let txn = state.db.begin().await?;
    lock_unbanned(&txn, me).await?;
    // The row lock serializes this sender's waves, so the 20-per-window count cannot be raced.
    let win = window::active_locked(&txn, me)
        .await?
        .ok_or(AppError::NoActiveWindow)?;
    if nearby::rows(&txn, me, &[target]).await?.is_empty() {
        return Err(AppError::NotVisible);
    }

    let outcome = if let Some(m) = find_between(&txn, me, target).await? {
        Outcome::Existing(WaveResult {
            matched: true,
            match_id: Some(m.id),
        })
    } else if wave::Entity::find()
        .filter(wave::Column::WindowId.eq(win.id))
        .filter(wave::Column::ToUserId.eq(target))
        .count(&txn)
        .await?
        > 0
    {
        Outcome::Existing(WaveResult {
            matched: false,
            match_id: None,
        })
    } else {
        let sent = wave::Entity::find()
            .filter(wave::Column::WindowId.eq(win.id))
            .count(&txn)
            .await?;
        if sent >= MAX_WAVES_PER_WINDOW {
            return Err(AppError::WaveLimit);
        }
        let reverse = wave::Entity::find()
            .filter(wave::Column::FromUserId.eq(target))
            .filter(wave::Column::ToUserId.eq(me))
            .filter(wave::Column::ExpiresAt.gt(now))
            .count(&txn)
            .await?
            > 0;
        if reverse {
            let m = find_or_create(&txn, me, target).await?;
            wave::Entity::delete_many()
                .filter(
                    Condition::any()
                        .add(
                            Condition::all()
                                .add(wave::Column::FromUserId.eq(me))
                                .add(wave::Column::ToUserId.eq(target)),
                        )
                        .add(
                            Condition::all()
                                .add(wave::Column::FromUserId.eq(target))
                                .add(wave::Column::ToUserId.eq(me)),
                        ),
                )
                .exec(&txn)
                .await?;
            Outcome::Matched(m)
        } else {
            wave::ActiveModel {
                id: Set(Uuid::new_v4()),
                from_user_id: Set(me),
                to_user_id: Set(target),
                window_id: Set(win.id),
                created_at: Set(now),
                expires_at: Set(win.ends_at),
            }
            .insert(&txn)
            .await?;
            Outcome::Waved
        }
    };
    txn.commit().await?;

    Ok(Json(match outcome {
        Outcome::Existing(result) => result,
        Outcome::Waved => {
            state
                .hub
                .send(target, &ServerEvent::Wave { from_user_id: me });
            WaveResult {
                matched: false,
                match_id: None,
            }
        }
        Outcome::Matched(m) => {
            for user in [me, target] {
                if let Some(summary) = summaries(&state.db, user, std::slice::from_ref(&m))
                    .await?
                    .pop()
                {
                    state.hub.send(user, &ServerEvent::Match { summary });
                }
            }
            WaveResult {
                matched: true,
                match_id: Some(m.id),
            }
        }
    }))
}

pub async fn incoming(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<NearbyProfile>>, AppError> {
    window::active(&state.db, auth.id)
        .await?
        .ok_or(AppError::NoActiveWindow)?;
    let senders: Vec<Uuid> = wave::Entity::find()
        .filter(wave::Column::ToUserId.eq(auth.id))
        .filter(wave::Column::ExpiresAt.gt(Utc::now().fixed_offset()))
        .all(&state.db)
        .await?
        .into_iter()
        .map(|w| w.from_user_id)
        .collect();
    // An empty id list means "everyone" to the nearby query, so it must not reach it.
    if senders.is_empty() {
        return Ok(Json(vec![]));
    }
    Ok(Json(nearby::list(&state.db, auth.id, &senders).await?))
}
