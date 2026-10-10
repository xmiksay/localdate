//! The caller's own account: `/me`, profile, photos, filter, password and linked identities.

pub mod filter;
mod identities;
pub(crate) mod image_proc;
mod password;
pub mod photos;
pub mod profile;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use entity::{photo, user};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
use serde::Serialize;
use uuid::Uuid;

use crate::auth::{ActingUser, AuthUser, UserDto};
use crate::error::AppError;
use crate::media::PhotoStore;
use crate::state::AppState;

pub use filter::FilterDto;
pub use photos::{PhotoDto, media_url};
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
        .merge(identities::router())
        .merge(password::router())
}

#[derive(Serialize)]
struct MeDto {
    user: UserDto,
    profile: Option<ProfileDto>,
    filter: Option<FilterDto>,
    is_admin: bool,
}

async fn get_me(State(state): State<AppState>, auth: ActingUser) -> Result<Json<MeDto>, AppError> {
    let user = user::Entity::find_by_id(auth.id)
        .one(&state.db)
        .await?
        .ok_or(AppError::Unauthorized)?;
    Ok(Json(MeDto {
        user: (&user).into(),
        profile: profile::load(&state.db, auth.id).await?,
        filter: filter::load(&state.db, auth.id).await?.map(Into::into),
        is_admin: user.is_admin,
    }))
}

async fn delete_me(State(state): State<AppState>, auth: AuthUser) -> Result<StatusCode, AppError> {
    delete_account(&state, auth.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Deletes the account (rows cascade) and then its photo files.
pub async fn delete_account(state: &AppState, user_id: Uuid) -> Result<(), AppError> {
    delete_account_in(&state.db, &state.photos, user_id).await
}

/// [`delete_account`] without the app state (test-user creation rolls back through it).
pub async fn delete_account_in(
    db: &impl ConnectionTrait,
    store: &PhotoStore,
    user_id: Uuid,
) -> Result<(), AppError> {
    // Names must be read first: the cascade removes the rows that know them.
    let files: Vec<String> = photo::Entity::find()
        .filter(photo::Column::UserId.eq(user_id))
        .all(db)
        .await?
        .into_iter()
        .map(|p| p.file_name)
        .collect();
    user::Entity::delete_by_id(user_id).exec(db).await?;
    // The account is gone either way; objects that failed to delete are logged by name.
    store.remove_all(&files).await;
    Ok(())
}
