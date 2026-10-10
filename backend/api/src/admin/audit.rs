//! `admin_audit`: what admins did while acting as other users (`GET /admin/audit`).

use axum::Json;
use axum::extract::State;
use chrono::{DateTime, Utc};
use entity::admin_audit;
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{
    ConnectionTrait, DbBackend, EntityTrait, FromQueryResult, JsonValue, Set, Statement,
};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use super::reports::{UserRef, user_ref};
use crate::auth::AdminUser;
use crate::error::AppError;
use crate::state::AppState;

pub const IMPERSONATE: &str = "impersonate";
pub const IMPERSONATED_REQUEST: &str = "impersonated_request";
/// `method` of the `impersonated_request` rows for an impersonated WebSocket's start and end.
pub const WS_OPEN: &str = "WS OPEN";
pub const WS_CLOSE: &str = "WS CLOSE";
const MAX_ROWS: i64 = 200;

pub async fn record(
    db: &impl ConnectionTrait,
    admin: Uuid,
    target: Uuid,
    action: &str,
    meta: JsonValue,
) -> Result<(), AppError> {
    admin_audit::Entity::insert(admin_audit::ActiveModel {
        id: Set(Uuid::new_v4()),
        admin_id: Set(Some(admin)),
        target_user_id: Set(Some(target)),
        action: Set(action.to_owned()),
        meta: Set(meta),
        created_at: Set(Utc::now().fixed_offset()),
    })
    .exec_without_returning(db)
    .await?;
    Ok(())
}

/// One request made with an impersonation token: method and route template, never the body.
pub async fn record_request(
    db: &impl ConnectionTrait,
    admin: Uuid,
    target: Uuid,
    method: &str,
    route: &str,
) -> Result<(), AppError> {
    let meta = json!({ "method": method, "route": route });
    record(db, admin, target, IMPERSONATED_REQUEST, meta).await
}

const SQL: &str = r#"
SELECT a.id, a.created_at, a.action, a.meta,
       ad.id AS admin_id, ad.username AS admin_username,
       t.id AS target_id, t.username AS target_username
FROM admin_audit a
LEFT JOIN "user" ad ON ad.id = a.admin_id
LEFT JOIN "user" t ON t.id = a.target_user_id
ORDER BY a.created_at DESC, a.id
LIMIT $1
"#;

#[derive(FromQueryResult)]
struct Row {
    id: Uuid,
    created_at: DateTimeWithTimeZone,
    action: String,
    meta: JsonValue,
    admin_id: Option<Uuid>,
    admin_username: Option<String>,
    target_id: Option<Uuid>,
    target_username: Option<String>,
}

#[derive(Serialize)]
pub struct AuditEntry {
    id: Uuid,
    created_at: DateTime<Utc>,
    action: String,
    admin: Option<UserRef>,
    target: Option<UserRef>,
    meta: JsonValue,
}

pub async fn list(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<Vec<AuditEntry>>, AppError> {
    let stmt = Statement::from_sql_and_values(DbBackend::Postgres, SQL, [MAX_ROWS.into()]);
    let rows = Row::find_by_statement(stmt).all(&state.db).await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| AuditEntry {
                id: r.id,
                created_at: r.created_at.to_utc(),
                action: r.action,
                admin: user_ref(r.admin_id, r.admin_username),
                target: user_ref(r.target_id, r.target_username),
                meta: r.meta,
            })
            .collect(),
    ))
}
