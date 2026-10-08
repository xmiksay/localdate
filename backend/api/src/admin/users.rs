use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::Utc;
use entity::{ReportResolution, report, user};
use sea_orm::sea_query::Expr;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QuerySelect, Set, TransactionTrait};
use uuid::Uuid;

use crate::auth::{AdminUser, refresh};
use crate::discovery::window;
use crate::error::{AppError, parse_id};
use crate::state::AppState;
use crate::ws::CloseReason;

/// The columns that mark a report resolved by `admin`, now.
pub fn resolution(admin: Uuid, outcome: ReportResolution) -> report::ActiveModel {
    report::ActiveModel {
        resolved_at: Set(Some(Utc::now().fixed_offset())),
        resolved_by: Set(Some(admin)),
        resolution: Set(Some(outcome)),
        ..Default::default()
    }
}

/// Soft ban (docs/api.md "Admin"): idempotent, refused for admins. Everything that keeps the
/// user signed in or visible is cut in one transaction; open sockets are closed after commit.
pub async fn ban(state: &AppState, admin: Uuid, target: Uuid) -> Result<(), AppError> {
    let now = Utc::now();
    let txn = state.db.begin().await?;
    // The row lock serializes concurrent bans/unbans and `admin grant` of the same account.
    let subject = user::Entity::find_by_id(target)
        .lock_exclusive()
        .one(&txn)
        .await?
        .ok_or(AppError::NotFound)?;
    if subject.is_admin {
        return Err(AppError::CannotBanAdmin);
    }
    if subject.banned_at.is_none() {
        user::Entity::update_many()
            .col_expr(user::Column::BannedAt, Expr::value(now.fixed_offset()))
            .filter(user::Column::Id.eq(target))
            .exec(&txn)
            .await?;
    }
    refresh::revoke_all(&txn, target).await?;
    if let Some(w) = window::open_window(&txn, target).await? {
        window::end(&txn, w, now).await?;
    }
    report::Entity::update_many()
        .set(resolution(admin, ReportResolution::Banned))
        .filter(report::Column::ReportedId.eq(target))
        .filter(report::Column::ResolvedAt.is_null())
        .exec(&txn)
        .await?;
    txn.commit().await?;
    state.hub.disconnect(target, CloseReason::Banned);
    Ok(())
}

pub async fn post_ban(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    ban(&state, admin.id, parse_id(&id)?).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Clears `banned_at` only: tokens, window and resolved reports stay as the ban left them.
pub async fn post_unban(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let updated = user::Entity::update_many()
        .col_expr(
            user::Column::BannedAt,
            Expr::value(Option::<chrono::DateTime<chrono::FixedOffset>>::None),
        )
        .filter(user::Column::Id.eq(parse_id(&id)?))
        .exec(&state.db)
        .await?
        .rows_affected;
    if updated == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}
