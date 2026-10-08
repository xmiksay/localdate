use std::path::{Path as FsPath, PathBuf};

use axum::extract::{DefaultBodyLimit, Multipart, Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, post, put};
use axum::{Json, Router};
use chrono::Utc;
use entity::{photo, user};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, Set, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::image_proc;
use crate::auth::AuthUser;
use crate::error::{AppError, AppJson, parse_id};
use crate::state::AppState;

pub const MAX_PHOTOS: u64 = 6;
const MAX_UPLOAD_BYTES: usize = 10 * 1024 * 1024;
/// Headroom for multipart framing so a file of exactly 10 MB isn't cut off by the body limit.
const BODY_LIMIT: usize = MAX_UPLOAD_BYTES + 64 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/me/photos",
            post(upload).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .route("/me/photos/order", put(reorder))
        .route("/me/photos/{id}", delete(remove))
}

#[derive(Serialize)]
pub struct PhotoDto {
    pub id: Uuid,
    pub url: String,
    pub position: i16,
}

/// Public URL of a stored photo file.
pub fn media_url(file_name: &str) -> String {
    format!("/media/{file_name}")
}

impl From<photo::Model> for PhotoDto {
    fn from(p: photo::Model) -> Self {
        Self {
            id: p.id,
            url: media_url(&p.file_name),
            position: p.position,
        }
    }
}

#[derive(Deserialize)]
struct OrderBody {
    photo_ids: Vec<Uuid>,
}

pub async fn list(db: &impl ConnectionTrait, user_id: Uuid) -> Result<Vec<PhotoDto>, AppError> {
    let rows = photo::Entity::find()
        .filter(photo::Column::UserId.eq(user_id))
        .order_by_asc(photo::Column::Position)
        .all(db)
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Best effort: a leftover file is garbage, not a reason to fail the request.
pub async fn remove_files(dir: &FsPath, names: &[String]) {
    for name in names {
        if let Err(e) = tokio::fs::remove_file(dir.join(name)).await
            && e.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(error = %e, file = %name, "failed to delete photo file");
        }
    }
}

/// `field.bytes()` fails with 413 when the route's body limit trips.
fn multipart_error(err: axum::extract::multipart::MultipartError) -> AppError {
    if err.status() == StatusCode::PAYLOAD_TOO_LARGE {
        AppError::UnsupportedImage
    } else {
        AppError::validation("invalid multipart body")
    }
}

async fn read_file_field(mut multipart: Multipart) -> Result<Vec<u8>, AppError> {
    while let Some(field) = multipart.next_field().await.map_err(multipart_error)? {
        if field.name() == Some("file") {
            let bytes = field.bytes().await.map_err(multipart_error)?;
            return if bytes.len() > MAX_UPLOAD_BYTES {
                Err(AppError::UnsupportedImage)
            } else {
                Ok(bytes.to_vec())
            };
        }
    }
    Err(AppError::validation("multipart field `file` is required"))
}

async fn count(db: &impl ConnectionTrait, user_id: Uuid) -> Result<u64, AppError> {
    Ok(photo::Entity::find()
        .filter(photo::Column::UserId.eq(user_id))
        .count(db)
        .await?)
}

/// Row-locks the user so concurrent uploads/deletes/reorders of one account serialise.
async fn lock_user(db: &impl ConnectionTrait, user_id: Uuid) -> Result<(), AppError> {
    user::Entity::find_by_id(user_id)
        .lock_exclusive()
        .one(db)
        .await?
        .ok_or(AppError::Unauthorized)?;
    Ok(())
}

async fn upload(
    State(state): State<AppState>,
    auth: AuthUser,
    multipart: Multipart,
) -> Result<(StatusCode, Json<PhotoDto>), AppError> {
    // Cheap early exit; the authoritative check runs under the lock below.
    if count(&state.db, auth.id).await? >= MAX_PHOTOS {
        return Err(AppError::PhotoLimit);
    }
    let bytes = read_file_field(multipart).await?;
    // Each decode can take ~128 MiB plus resize buffers; the permit bounds concurrent ones so a
    // burst of uploads queues instead of blowing the pod's memory limit.
    let permit = state
        .image_permits
        .clone()
        .acquire_owned()
        .await
        .map_err(|e| anyhow::anyhow!("image semaphore closed: {e}"))?;
    let webp = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        image_proc::to_webp(&bytes)
    })
    .await
    .map_err(|e| anyhow::anyhow!("image task failed: {e}"))?
    .map_err(|_| AppError::UnsupportedImage)?;

    let id = Uuid::new_v4();
    let file_name = format!("{id}.webp");
    write_atomically(&state.config.photo_dir, &file_name, &webp).await?;

    match insert_row(&state, auth.id, id, &file_name).await {
        Ok(model) => Ok((StatusCode::CREATED, Json(model.into()))),
        Err(e) => {
            remove_files(&state.config.photo_dir, &[file_name]).await;
            Err(e)
        }
    }
}

async fn write_atomically(dir: &FsPath, file_name: &str, data: &[u8]) -> Result<(), AppError> {
    let target: PathBuf = dir.join(file_name);
    let tmp = dir.join(format!("{file_name}.tmp"));
    let result = async {
        tokio::fs::write(&tmp, data).await?;
        tokio::fs::rename(&tmp, &target).await
    }
    .await;
    if let Err(e) = result {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(anyhow::Error::new(e).context("writing photo file").into());
    }
    Ok(())
}

async fn insert_row(
    state: &AppState,
    user_id: Uuid,
    id: Uuid,
    file_name: &str,
) -> Result<photo::Model, AppError> {
    let txn = state.db.begin().await?;
    lock_user(&txn, user_id).await?;
    let position = count(&txn, user_id).await?;
    if position >= MAX_PHOTOS {
        return Err(AppError::PhotoLimit);
    }
    let model = photo::ActiveModel {
        id: Set(id),
        user_id: Set(user_id),
        file_name: Set(file_name.to_owned()),
        position: Set(i16::try_from(position).map_err(|_| AppError::Internal)?),
        created_at: Set(Utc::now().fixed_offset()),
    }
    .insert(&txn)
    .await?;
    txn.commit().await?;
    Ok(model)
}

async fn remove(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let id = parse_id(&id)?;
    let txn = state.db.begin().await?;
    lock_user(&txn, auth.id).await?;
    let target = photo::Entity::find_by_id(id)
        .filter(photo::Column::UserId.eq(auth.id))
        .one(&txn)
        .await?
        .ok_or(AppError::NotFound)?;
    photo::Entity::delete_by_id(id).exec(&txn).await?;
    let remaining = photo::Entity::find()
        .filter(photo::Column::UserId.eq(auth.id))
        .order_by_asc(photo::Column::Position)
        .all(&txn)
        .await?;
    let ids: Vec<Uuid> = remaining.iter().map(|p| p.id).collect();
    write_positions(&txn, &remaining, &ids).await?;
    txn.commit().await?;
    remove_files(&state.config.photo_dir, &[target.file_name]).await;
    Ok(StatusCode::NO_CONTENT)
}

/// Sets `position = index in order` for every row whose stored position differs.
async fn write_positions(
    db: &impl ConnectionTrait,
    rows: &[photo::Model],
    order: &[Uuid],
) -> Result<(), AppError> {
    for (index, id) in order.iter().enumerate() {
        let position = i16::try_from(index).map_err(|_| AppError::Internal)?;
        if rows.iter().any(|r| r.id == *id && r.position != position) {
            photo::ActiveModel {
                id: Set(*id),
                position: Set(position),
                ..Default::default()
            }
            .update(db)
            .await?;
        }
    }
    Ok(())
}

/// `requested` must be a permutation of `current`.
fn validate_order(current: &[Uuid], requested: &[Uuid]) -> Result<(), AppError> {
    let mut a = current.to_vec();
    let mut b = requested.to_vec();
    a.sort_unstable();
    b.sort_unstable();
    if a == b {
        Ok(())
    } else {
        Err(AppError::validation(
            "photo_ids must contain each of your photos exactly once",
        ))
    }
}

async fn reorder(
    State(state): State<AppState>,
    auth: AuthUser,
    AppJson(body): AppJson<OrderBody>,
) -> Result<Json<Vec<PhotoDto>>, AppError> {
    let txn = state.db.begin().await?;
    lock_user(&txn, auth.id).await?;
    let rows = photo::Entity::find()
        .filter(photo::Column::UserId.eq(auth.id))
        .all(&txn)
        .await?;
    let current: Vec<Uuid> = rows.iter().map(|p| p.id).collect();
    validate_order(&current, &body.photo_ids)?;
    write_positions(&txn, &rows, &body.photo_ids).await?;
    txn.commit().await?;
    Ok(Json(list(&state.db, auth.id).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_must_be_exact_permutation() {
        let (a, b, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        assert!(validate_order(&[a, b], &[b, a]).is_ok());
        assert!(validate_order(&[], &[]).is_ok());
        assert!(validate_order(&[a, b], &[a]).is_err());
        assert!(validate_order(&[a, b], &[a, b, c]).is_err());
        assert!(validate_order(&[a, b], &[a, a]).is_err());
        assert!(validate_order(&[a, b], &[a, c]).is_err());
    }
}
