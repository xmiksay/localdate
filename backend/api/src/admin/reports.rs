use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use entity::{ReportReason, ReportResolution, report};
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{
    ActiveEnum, ColumnTrait, DbBackend, EntityTrait, FromQueryResult, QueryFilter, Statement,
};
use serde::Serialize;
use uuid::Uuid;

use super::users::resolution;
use crate::auth::AdminUser;
use crate::error::{AppError, parse_id};
use crate::me::media_url;
use crate::state::AppState;

const MAX_ROWS: i64 = 200;

/// $1 = 'open' | 'resolved' | NULL (both), $2 = row limit. Open first, oldest first (queue
/// order); resolved ones most recently resolved first.
const SQL: &str = r#"
SELECT r.id, r.reason::text AS reason, r.note, r.created_at, r.resolved_at,
       r.resolution::text AS resolution,
       rb.id AS resolved_by_id, rb.username AS resolved_by_username,
       rp.id AS reporter_id, rp.username AS reporter_username,
       s.id AS subject_id, s.username AS subject_username, s.banned_at AS subject_banned_at,
       s.is_admin AS subject_is_admin, p.display_name AS subject_display_name,
       (SELECT ph.file_name FROM photo ph WHERE ph.user_id = s.id
        ORDER BY ph.position LIMIT 1) AS subject_photo,
       (SELECT count(*) FROM report o
        WHERE o.reported_id = s.id AND o.resolved_at IS NULL) AS open_reports
FROM report r
JOIN "user" s ON s.id = r.reported_id
LEFT JOIN profile p ON p.user_id = s.id
LEFT JOIN "user" rp ON rp.id = r.reporter_id
LEFT JOIN "user" rb ON rb.id = r.resolved_by
WHERE $1::text IS NULL OR ($1::text = 'open') = (r.resolved_at IS NULL)
ORDER BY r.resolved_at IS NOT NULL, r.resolved_at DESC, r.created_at, r.id
LIMIT $2
"#;

#[derive(FromQueryResult)]
struct Row {
    id: Uuid,
    reason: String,
    note: Option<String>,
    created_at: DateTimeWithTimeZone,
    resolved_at: Option<DateTimeWithTimeZone>,
    resolution: Option<String>,
    resolved_by_id: Option<Uuid>,
    resolved_by_username: Option<String>,
    reporter_id: Option<Uuid>,
    reporter_username: Option<String>,
    subject_id: Uuid,
    subject_username: String,
    subject_banned_at: Option<DateTimeWithTimeZone>,
    subject_is_admin: bool,
    subject_display_name: Option<String>,
    subject_photo: Option<String>,
    open_reports: i64,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct UserRef {
    id: Uuid,
    username: String,
}

#[derive(Serialize)]
pub struct SubjectDto {
    id: Uuid,
    username: String,
    display_name: Option<String>,
    photo_url: Option<String>,
    banned_at: Option<DateTime<Utc>>,
    is_admin: bool,
    open_reports: i64,
}

#[derive(Serialize)]
pub struct AdminReport {
    id: Uuid,
    reason: ReportReason,
    note: Option<String>,
    created_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
    resolution: Option<ReportResolution>,
    resolved_by: Option<UserRef>,
    reporter: Option<UserRef>,
    subject: SubjectDto,
}

fn user_ref(id: Option<Uuid>, username: Option<String>) -> Option<UserRef> {
    Some(UserRef {
        id: id?,
        username: username?,
    })
}

impl TryFrom<Row> for AdminReport {
    type Error = AppError;

    fn try_from(r: Row) -> Result<Self, AppError> {
        Ok(Self {
            id: r.id,
            reason: ReportReason::try_from_value(&r.reason)?,
            note: r.note,
            created_at: r.created_at.to_utc(),
            resolved_at: r.resolved_at.map(|t| t.to_utc()),
            resolution: r
                .resolution
                .map(|s| ReportResolution::try_from_value(&s))
                .transpose()?,
            resolved_by: user_ref(r.resolved_by_id, r.resolved_by_username),
            reporter: user_ref(r.reporter_id, r.reporter_username),
            subject: SubjectDto {
                id: r.subject_id,
                username: r.subject_username,
                display_name: r.subject_display_name,
                photo_url: r.subject_photo.as_deref().map(media_url),
                banned_at: r.subject_banned_at.map(|t| t.to_utc()),
                is_admin: r.subject_is_admin,
                open_reports: r.open_reports,
            },
        })
    }
}

/// `None` = both; anything but `open`/`resolved` is a 400.
fn parse_status(raw: Option<&String>) -> Result<Option<&'static str>, AppError> {
    match raw.map(String::as_str) {
        None => Ok(None),
        Some("open") => Ok(Some("open")),
        Some("resolved") => Ok(Some("resolved")),
        Some(_) => Err(AppError::validation("status must be open or resolved")),
    }
}

/// Query is read as a raw map so malformed values get the contract's 400 envelope.
pub async fn list(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<AdminReport>>, AppError> {
    let status = parse_status(params.get("status"))?;
    let stmt = Statement::from_sql_and_values(
        DbBackend::Postgres,
        SQL,
        [status.map(str::to_owned).into(), MAX_ROWS.into()],
    );
    let rows = Row::find_by_statement(stmt).all(&state.db).await?;
    Ok(Json(
        rows.into_iter()
            .map(AdminReport::try_from)
            .collect::<Result<_, _>>()?,
    ))
}

pub async fn dismiss(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let id = parse_id(&id)?;
    // Conditional write: two admins dismissing at once can't both "win" or overwrite a ban.
    let updated = report::Entity::update_many()
        .set(resolution(admin.id, ReportResolution::Dismissed))
        .filter(report::Column::Id.eq(id))
        .filter(report::Column::ResolvedAt.is_null())
        .exec(&state.db)
        .await?
        .rows_affected;
    if updated > 0 {
        return Ok(StatusCode::NO_CONTENT);
    }
    match report::Entity::find_by_id(id).one(&state.db).await? {
        Some(_) => Err(AppError::AlreadyResolved),
        None => Err(AppError::NotFound),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_is_open_resolved_or_absent() {
        assert_eq!(parse_status(None).ok(), Some(None));
        assert_eq!(parse_status(Some(&"open".into())).ok(), Some(Some("open")));
        assert_eq!(
            parse_status(Some(&"resolved".into())).ok(),
            Some(Some("resolved"))
        );
        for bad in ["", "OPEN", "all"] {
            assert!(parse_status(Some(&bad.to_owned())).is_err(), "{bad}");
        }
    }

    #[test]
    fn user_ref_needs_both_parts() {
        let id = Uuid::new_v4();
        assert_eq!(
            user_ref(Some(id), Some("eva".into())),
            Some(UserRef {
                id,
                username: "eva".into()
            })
        );
        assert_eq!(user_ref(None, None), None);
        assert_eq!(user_ref(Some(id), None), None);
    }
}
