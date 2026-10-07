//! The caller's own account: `/me`, profile, photos and filter.

mod filter;
mod image_proc;
mod photos;
mod profile;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use entity::{photo, user};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::Serialize;

use crate::auth::{AuthUser, UserDto};
use crate::error::AppError;
use crate::state::AppState;

pub use filter::FilterDto;
pub use photos::PhotoDto;
pub use profile::ProfileDto;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me", get(get_me).delete(delete_me))
        .route("/me/profile", put(profile::put_profile))
        .route(
            "/me/filter",
            get(filter::get_filter).put(filter::put_filter),
        )
        .merge(photos::router())
}

#[derive(Serialize)]
struct MeDto {
    user: UserDto,
    profile: Option<ProfileDto>,
    filter: Option<FilterDto>,
}

async fn get_me(State(state): State<AppState>, auth: AuthUser) -> Result<Json<MeDto>, AppError> {
    let user = user::Entity::find_by_id(auth.id)
        .one(&state.db)
        .await?
        .ok_or(AppError::Unauthorized)?;
    Ok(Json(MeDto {
        user: (&user).into(),
        profile: profile::load(&state.db, auth.id).await?,
        filter: filter::load(&state.db, auth.id).await?.map(Into::into),
    }))
}

async fn delete_me(State(state): State<AppState>, auth: AuthUser) -> Result<StatusCode, AppError> {
    // Names must be read first: the cascade removes the rows that know them.
    let files: Vec<String> = photo::Entity::find()
        .filter(photo::Column::UserId.eq(auth.id))
        .all(&state.db)
        .await?
        .into_iter()
        .map(|p| p.file_name)
        .collect();
    user::Entity::delete_by_id(auth.id).exec(&state.db).await?;
    photos::remove_files(&state.config.photo_dir, &files).await;
    Ok(StatusCode::NO_CONTENT)
}
