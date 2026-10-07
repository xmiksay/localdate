use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use entity::interest;
use sea_orm::{EntityTrait, QueryOrder};
use serde::Serialize;

use crate::error::AppError;
use crate::state::AppState;

#[derive(Serialize)]
pub struct InterestDto {
    pub id: i32,
    pub key: String,
}

impl From<interest::Model> for InterestDto {
    fn from(m: interest::Model) -> Self {
        Self {
            id: m.id,
            key: m.key,
        }
    }
}

pub fn router() -> Router<AppState> {
    Router::new().route("/interests", get(list))
}

async fn list(State(state): State<AppState>) -> Result<Json<Vec<InterestDto>>, AppError> {
    let rows = interest::Entity::find()
        .order_by_asc(interest::Column::Id)
        .all(&state.db)
        .await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}
