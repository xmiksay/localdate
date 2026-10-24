//! Account lookup for admins: `GET /admin/users?q=` and `GET /admin/test-users`.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Query, State};
use chrono::{DateTime, NaiveDate, Utc};
use entity::Gender;
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{ActiveEnum, ConnectionTrait, DbBackend, FromQueryResult, Statement, Value};
use serde::Serialize;
use uuid::Uuid;

use crate::auth::AdminUser;
use crate::auth::validation::username_key;
use crate::error::AppError;
use crate::me::media_url;
use crate::me::profile::age_on;
use crate::state::AppState;

const MAX_SEARCH_ROWS: i64 = 50;
const MAX_TEST_ROWS: i64 = 500;
const MAX_QUERY_CHARS: usize = 64;

/// $1 = username-key LIKE pattern or NULL, $2 = display-name ILIKE pattern, $3 = test users only,
/// $4 = one id or NULL, $5 = row limit.
const SQL: &str = r#"
SELECT u.id, u.username, u.is_test, u.is_admin, u.banned_at, u.created_at,
       p.display_name, p.gender::text AS gender, p.birth_date,
       (SELECT ph.file_name FROM photo ph WHERE ph.user_id = u.id
        ORDER BY ph.position LIMIT 1) AS photo
FROM "user" u
LEFT JOIN profile p ON p.user_id = u.id
WHERE ($1::text IS NULL OR u.username_key LIKE $1 OR p.display_name ILIKE $2)
  AND (NOT $3::bool OR u.is_test)
  AND ($4::uuid IS NULL OR u.id = $4)
ORDER BY u.created_at DESC, u.id
LIMIT $5
"#;

#[derive(FromQueryResult)]
struct Row {
    id: Uuid,
    username: String,
    is_test: bool,
    is_admin: bool,
    banned_at: Option<DateTimeWithTimeZone>,
    created_at: DateTimeWithTimeZone,
    display_name: Option<String>,
    gender: Option<String>,
    birth_date: Option<NaiveDate>,
    photo: Option<String>,
}

#[derive(Serialize)]
pub struct AdminUserRow {
    pub id: Uuid,
    username: String,
    display_name: Option<String>,
    photo_url: Option<String>,
    gender: Option<Gender>,
    age: Option<i32>,
    is_test: bool,
    is_admin: bool,
    banned_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

impl TryFrom<Row> for AdminUserRow {
    type Error = AppError;

    fn try_from(r: Row) -> Result<Self, AppError> {
        let today = Utc::now().date_naive();
        Ok(Self {
            id: r.id,
            username: r.username,
            display_name: r.display_name,
            photo_url: r.photo.as_deref().map(media_url),
            gender: r.gender.map(|g| Gender::try_from_value(&g)).transpose()?,
            age: r.birth_date.map(|b| age_on(b, today)),
            is_test: r.is_test,
            is_admin: r.is_admin,
            banned_at: r.banned_at.map(|t| t.to_utc()),
            created_at: r.created_at.to_utc(),
        })
    }
}

/// `%q%` for ILIKE with the pattern characters escaped (`\` is Postgres' default escape).
fn like_pattern(q: &str) -> String {
    let mut out = String::with_capacity(q.len() + 2);
    out.push('%');
    for c in q.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

/// Trimmed search text; empty = none, over the cap = 400.
fn search_text(raw: Option<&String>) -> Result<Option<String>, AppError> {
    let q = raw.map(|s| s.trim()).unwrap_or_default();
    if q.chars().count() > MAX_QUERY_CHARS {
        return Err(AppError::validation(format!(
            "q must be at most {MAX_QUERY_CHARS} characters"
        )));
    }
    Ok((!q.is_empty()).then(|| q.to_owned()))
}

/// Patterns for `$1` (usernames, by the same key as the unique index — Unicode lowercase computed
/// by the app, not the database locale) and `$2` (display names, ILIKE).
fn patterns(q: Option<&str>) -> (Option<String>, Option<String>) {
    match q {
        Some(q) => (Some(like_pattern(&username_key(q))), Some(like_pattern(q))),
        None => (None, None),
    }
}

async fn rows(
    db: &impl ConnectionTrait,
    q: Option<&str>,
    test_only: bool,
    id: Option<Uuid>,
    limit: i64,
) -> Result<Vec<AdminUserRow>, AppError> {
    let (key, display) = patterns(q);
    let values: [Value; 5] = [
        key.into(),
        display.into(),
        test_only.into(),
        id.into(),
        limit.into(),
    ];
    let stmt = Statement::from_sql_and_values(DbBackend::Postgres, SQL, values);
    Row::find_by_statement(stmt)
        .all(db)
        .await?
        .into_iter()
        .map(AdminUserRow::try_from)
        .collect()
}

/// The row of one account; `404` when it is gone.
pub async fn one(db: &impl ConnectionTrait, id: Uuid) -> Result<AdminUserRow, AppError> {
    rows(db, None, false, Some(id), 1)
        .await?
        .into_iter()
        .next()
        .ok_or(AppError::NotFound)
}

pub async fn search(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<AdminUserRow>>, AppError> {
    let q = search_text(params.get("q"))?;
    Ok(Json(
        rows(&state.db, q.as_deref(), false, None, MAX_SEARCH_ROWS).await?,
    ))
}

pub async fn test_users(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<Vec<AdminUserRow>>, AppError> {
    Ok(Json(
        rows(&state.db, None, true, None, MAX_TEST_ROWS).await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_pattern_escapes_wildcards() {
        assert_eq!(like_pattern("eva"), "%eva%");
        assert_eq!(like_pattern("a_b%c\\"), "%a\\_b\\%c\\\\%");
    }

    #[test]
    fn search_text_is_trimmed_optional_and_capped() {
        assert_eq!(search_text(None).ok(), Some(None));
        assert_eq!(search_text(Some(&"  ".into())).ok(), Some(None));
        assert_eq!(
            search_text(Some(&" Eva ".into())).ok(),
            Some(Some("Eva".into()))
        );
        assert!(search_text(Some(&"x".repeat(65))).is_err());
        assert!(search_text(Some(&"ž".repeat(64))).is_ok());
    }

    #[test]
    fn usernames_match_by_key_display_names_as_typed() {
        assert_eq!(patterns(None), (None, None));
        let (key, display) = patterns(Some("Petr Novák_%"));
        assert_eq!(key.as_deref(), Some("%petr novák\\_\\%%"));
        assert_eq!(display.as_deref(), Some("%Petr Novák\\_\\%%"));
    }
}
