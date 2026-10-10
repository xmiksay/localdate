//! Test users (`is_test`): password-less, onboarded accounts an admin creates for alpha/beta
//! testing and drives through impersonation.

use axum::extract::{DefaultBodyLimit, Multipart, Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, post};
use axum::{Json, Router};
use chrono::{NaiveDate, Utc};
use entity::{Gender, user};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set, SqlErr,
};
use serde::Deserialize;
use uuid::Uuid;

use super::avatar;
use super::directory::{self, AdminUserRow};
use crate::auth::{AdminUser, validation};
use crate::error::{AppError, AppJson, parse_id};
use crate::me::filter::{self, FilterDto};
use crate::me::photos::{self, BODY_LIMIT, PhotoDto};
use crate::me::profile::{self, ProfileBody};
use crate::media::PhotoStore;
use crate::state::AppState;
use crate::ws::CloseReason;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/test-users",
            post(post_create).get(directory::test_users),
        )
        .route("/admin/test-users/{id}", delete(remove))
        .route(
            "/admin/test-users/{id}/photos",
            post(upload).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
}

#[derive(Deserialize)]
pub struct NewTestUser {
    pub username: String,
    pub display_name: String,
    pub gender: Gender,
    pub birth_date: NaiveDate,
    #[serde(default)]
    pub bio: String,
    pub interest_ids: Vec<i32>,
    #[serde(default = "yes")]
    pub placeholder_photo: bool,
}

fn yes() -> bool {
    true
}

/// Wide enough that a tester finds the test users wherever they stand in town.
fn test_filter() -> FilterDto {
    FilterDto {
        max_distance_m: 10_000,
        ..FilterDto::default()
    }
}

/// The placeholder avatar for `seed`, already through the upload pipeline (WebP).
pub async fn placeholder_webp(seed: u64) -> Result<Vec<u8>, AppError> {
    let webp = tokio::task::spawn_blocking(move || {
        let png = avatar::placeholder_png(seed)?;
        crate::me::image_proc::to_webp(&png)
            .map_err(|e| anyhow::anyhow!("placeholder rejected by the photo pipeline: {e:?}"))
    })
    .await
    .map_err(|e| anyhow::anyhow!("placeholder task failed: {e}"))??;
    Ok(webp)
}

/// Creates the account, profile, filter and (with `photo`) its first photo. Anything failing after
/// the account row exists deletes the account again, so no half-made test user is left behind.
pub async fn create(
    db: &DatabaseConnection,
    store: &PhotoStore,
    new: NewTestUser,
    photo: Option<Vec<u8>>,
) -> Result<Uuid, AppError> {
    let username = validation::normalize_username(&new.username)?;
    let id = Uuid::new_v4();
    user::ActiveModel {
        id: Set(id),
        username: Set(username.name),
        username_key: Set(username.key),
        password_hash: Set(None),
        created_at: Set(Utc::now().fixed_offset()),
        is_admin: Set(false),
        banned_at: Set(None),
        credentials_changed_at: Set(None),
        is_test: Set(true),
    }
    .insert(db)
    .await
    .map_err(|e| match e.sql_err() {
        Some(SqlErr::UniqueConstraintViolation(_)) => AppError::UsernameTaken,
        _ => e.into(),
    })?;
    let profile = ProfileBody {
        display_name: new.display_name,
        birth_date: new.birth_date,
        gender: new.gender,
        bio: new.bio,
        interest_ids: new.interest_ids,
    };
    let filled = async {
        profile::save(db, id, profile).await?;
        filter::save(db, id, test_filter()).await?;
        if let Some(webp) = photo {
            photos::add(db, store, id, webp).await?;
        }
        Ok::<_, AppError>(())
    }
    .await;
    if let Err(e) = filled {
        // Also removes any photo object stored for the account (`photos::add` cleans up its own
        // failed insert; this covers every other order of failures).
        if let Err(cleanup) = crate::me::delete_account_in(db, store, id).await {
            tracing::error!(error = %cleanup, "removing a half-created test user failed");
        }
        return Err(e);
    }
    Ok(id)
}

async fn post_create(
    State(state): State<AppState>,
    _admin: AdminUser,
    AppJson(body): AppJson<NewTestUser>,
) -> Result<(StatusCode, Json<AdminUserRow>), AppError> {
    let photo = if body.placeholder_photo {
        Some(placeholder_webp(Uuid::new_v4().as_u128() as u64).await?)
    } else {
        None
    };
    let id = create(&state.db, &state.photos, body, photo).await?;
    Ok((
        StatusCode::CREATED,
        Json(directory::one(&state.db, id).await?),
    ))
}

/// `404` unless `id` names a non-admin test user: real accounts are never edited or deleted here.
async fn test_user(state: &AppState, id: &str) -> Result<Uuid, AppError> {
    let id = parse_id(id)?;
    user::Entity::find_by_id(id)
        .filter(user::Column::IsTest.eq(true))
        // An admin flagged as a test user (by hand in the DB) must stay out of reach of this API.
        .filter(user::Column::IsAdmin.eq(false))
        .one(&state.db)
        .await?
        .map(|u| u.id)
        .ok_or(AppError::NotFound)
}

async fn upload(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
    multipart: Multipart,
) -> Result<(StatusCode, Json<PhotoDto>), AppError> {
    let id = test_user(&state, &id).await?;
    photos::upload_for(&state, id, multipart).await
}

async fn remove(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let id = test_user(&state, &id).await?;
    crate::me::delete_account(&state, id).await?;
    // Sockets of an admin acting as this user would otherwise wait for their periodic re-check.
    state.hub.disconnect(id, CloseReason::Unauthorized);
    Ok(StatusCode::NO_CONTENT)
}
