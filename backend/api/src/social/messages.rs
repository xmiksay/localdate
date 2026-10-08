use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use entity::{matches, message};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, Set,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::matches::other_user;
use crate::auth::AuthUser;
use crate::error::{AppError, AppJson, parse_id};
use crate::safety::is_blocked_between;
use crate::state::AppState;
use crate::ws::ServerEvent;

const DEFAULT_LIMIT: u64 = 50;
const MAX_LIMIT: u64 = 100;
const MAX_BODY_CHARS: usize = 2000;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MessageDto {
    pub id: Uuid,
    pub match_id: Uuid,
    pub sender_id: Uuid,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

impl From<message::Model> for MessageDto {
    fn from(m: message::Model) -> Self {
        Self {
            id: m.id,
            match_id: m.match_id,
            sender_id: m.sender_id,
            body: m.body,
            created_at: m.created_at.to_utc(),
        }
    }
}

#[derive(Deserialize)]
pub struct SendBody {
    body: String,
}

/// Unknown or foreign matches are 404 (don't reveal existence); blocked or banned pairs are 403.
async fn participant_match(
    db: &impl ConnectionTrait,
    me: Uuid,
    raw_id: &str,
) -> Result<matches::Model, AppError> {
    let m = matches::Entity::find_by_id(parse_id(raw_id)?)
        .one(db)
        .await?
        .filter(|m| m.user_a == me || m.user_b == me)
        .ok_or(AppError::NotFound)?;
    if is_blocked_between(db, m.user_a, m.user_b).await? {
        return Err(AppError::Forbidden);
    }
    Ok(m)
}

fn parse_limit(raw: Option<&String>) -> Result<u64, AppError> {
    let Some(raw) = raw else {
        return Ok(DEFAULT_LIMIT);
    };
    match raw.parse::<u64>() {
        Ok(n) if (1..=MAX_LIMIT).contains(&n) => Ok(n),
        _ => Err(AppError::validation(format!("limit must be 1-{MAX_LIMIT}"))),
    }
}

fn clean_body(raw: &str) -> Result<String, AppError> {
    let body = raw.trim();
    if body.is_empty() || body.chars().count() > MAX_BODY_CHARS {
        return Err(AppError::validation(format!(
            "body must be 1-{MAX_BODY_CHARS} characters"
        )));
    }
    Ok(body.to_owned())
}

/// Query is read as a raw map so malformed values get the contract's 400 envelope.
pub async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(match_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<MessageDto>>, AppError> {
    let limit = parse_limit(params.get("limit"))?;
    let before = params
        .get("before")
        .map(|b| b.parse::<Uuid>())
        .transpose()
        .map_err(|_| AppError::validation("before must be a message id"))?;
    let m = participant_match(&state.db, auth.id, &match_id).await?;

    let mut query = message::Entity::find().filter(message::Column::MatchId.eq(m.id));
    if let Some(before) = before {
        let cursor = message::Entity::find_by_id(before)
            .filter(message::Column::MatchId.eq(m.id))
            .one(&state.db)
            .await?
            .ok_or(AppError::NotFound)?;
        query = query.filter(
            Condition::any()
                .add(message::Column::CreatedAt.lt(cursor.created_at))
                .add(
                    Condition::all()
                        .add(message::Column::CreatedAt.eq(cursor.created_at))
                        .add(message::Column::Id.lt(cursor.id)),
                ),
        );
    }
    let rows = query
        .order_by_desc(message::Column::CreatedAt)
        .order_by_desc(message::Column::Id)
        .limit(limit)
        .all(&state.db)
        .await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

pub async fn send(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(match_id): Path<String>,
    AppJson(body): AppJson<SendBody>,
) -> Result<(StatusCode, Json<MessageDto>), AppError> {
    let text = clean_body(&body.body)?;
    let m = participant_match(&state.db, auth.id, &match_id).await?;
    let row = message::ActiveModel {
        id: Set(Uuid::new_v4()),
        match_id: Set(m.id),
        sender_id: Set(auth.id),
        body: Set(text),
        created_at: Set(Utc::now().fixed_offset()),
    }
    .insert(&state.db)
    .await?;
    let dto = MessageDto::from(row);
    // The sender gets it too so their other tabs update; clients dedupe by id.
    for user in [auth.id, other_user(&m, auth.id)] {
        state.notify.send(
            auth.id,
            user,
            &ServerEvent::Message {
                message: dto.clone(),
            },
        );
    }
    Ok((StatusCode::CREATED, Json(dto)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_defaults_and_bounds() {
        assert_eq!(parse_limit(None).ok(), Some(50));
        assert_eq!(parse_limit(Some(&"1".into())).ok(), Some(1));
        assert_eq!(parse_limit(Some(&"100".into())).ok(), Some(100));
        for bad in ["0", "101", "-3", "abc", ""] {
            assert!(parse_limit(Some(&bad.to_owned())).is_err(), "{bad}");
        }
    }

    #[test]
    fn body_is_trimmed_and_bounded() {
        assert_eq!(clean_body("  hi ").ok().as_deref(), Some("hi"));
        assert!(clean_body("   ").is_err());
        assert!(clean_body(&"x".repeat(2000)).is_ok());
        assert!(clean_body(&"x".repeat(2001)).is_err());
        assert!(clean_body(&"é".repeat(2000)).is_ok());
    }
}
