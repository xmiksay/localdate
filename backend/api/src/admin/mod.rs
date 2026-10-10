//! Moderation: the report queue, soft bans and the admin role (`/api/admin`, `AdminUser` only);
//! for alpha/beta testing also test users, impersonation ("act as") and its audit log.

pub mod audit;
mod avatar;
mod directory;
mod impersonate;
mod reports;
pub mod seed;
pub mod test_users;
mod users;

use anyhow::bail;
use axum::Router;
use axum::routing::{get, post};
use entity::user;
use sea_orm::sea_query::Expr;
use sea_orm::{ConnectionTrait, EntityTrait, QueryFilter};

use crate::auth::validation;
use crate::state::AppState;

pub use users::ban;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/reports", get(reports::list))
        .route("/admin/reports/{id}/dismiss", post(reports::dismiss))
        .route("/admin/users/{id}/ban", post(users::post_ban))
        .route("/admin/users/{id}/unban", post(users::post_unban))
        .route("/admin/users", get(directory::search))
        .route(
            "/admin/users/{id}/impersonate",
            post(impersonate::impersonate),
        )
        .route("/admin/settings", get(impersonate::settings))
        .route("/admin/audit", get(audit::list))
        .merge(test_users::router())
}

/// `localdate-api admin grant|revoke <username>`: flips `is_admin`; unknown users are an error.
pub async fn set_admin(
    db: &impl ConnectionTrait,
    username: &str,
    is_admin: bool,
) -> anyhow::Result<()> {
    let updated = user::Entity::update_many()
        .col_expr(user::Column::IsAdmin, Expr::value(is_admin))
        .filter(validation::username_matches(username))
        .exec(db)
        .await?
        .rows_affected;
    if updated == 0 {
        bail!("no user named '{username}'");
    }
    Ok(())
}
