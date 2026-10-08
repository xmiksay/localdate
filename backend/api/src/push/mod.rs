//! Web Push: config, the caller's subscriptions and preferences (docs/api/push.md).
//! Sending lives in [`Notifier`].

mod endpoint;
mod message;
mod notifier;
mod sender;
pub mod vapid;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::routing::{get, post};
use axum::{Json, Router};
use entity::{push_prefs, push_subscription};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait, QueryFilter,
    Statement,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{AppError, AppJson};
use crate::sql;
use crate::state::AppState;

pub use endpoint::{validate_endpoint, validate_keys};
pub use message::{Kind, Lang, Push, TopicKey, Urgency};
pub use notifier::{MESSAGE_COALESCE, Notifier};
pub use sender::{Outcome, PushSender, SendFuture, Target};

/// Subscriptions kept per user (newest registrations win); bounds the fan-out of one event.
pub const MAX_SUBSCRIPTIONS_PER_USER: u64 = 10;
const MAX_USER_AGENT_CHARS: usize = 255;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/push/config", get(config))
        .route(
            "/me/push/subscriptions",
            post(subscribe).delete(unsubscribe),
        )
        .route("/me/push/prefs", get(get_prefs).patch(patch_prefs))
}

#[derive(Serialize)]
struct ConfigDto {
    enabled: bool,
    public_key: Option<String>,
}

async fn config(State(state): State<AppState>) -> Json<ConfigDto> {
    let public_key = state.notify.public_key().map(str::to_owned);
    Json(ConfigDto {
        enabled: public_key.is_some(),
        public_key,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Keys {
    p256dh: String,
    auth: String,
}

#[derive(Deserialize)]
struct SubscribeBody {
    endpoint: String,
    keys: Keys,
    #[serde(default)]
    lang: Lang,
}

const UPSERT: &str = "INSERT INTO push_subscription \
     (id, user_id, endpoint, p256dh, auth, user_agent, lang, created_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, now()) \
     ON CONFLICT (endpoint) DO UPDATE SET user_id = EXCLUDED.user_id, p256dh = EXCLUDED.p256dh, \
     auth = EXCLUDED.auth, user_agent = EXCLUDED.user_agent, lang = EXCLUDED.lang, created_at = now(), \
     last_success_at = CASE WHEN push_subscription.user_id = EXCLUDED.user_id \
     THEN push_subscription.last_success_at END";
const TRIM: &str = "DELETE FROM push_subscription WHERE user_id = $1 AND id NOT IN \
     (SELECT id FROM push_subscription WHERE user_id = $1 ORDER BY created_at DESC, id DESC LIMIT $2)";

async fn subscribe(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    AppJson(body): AppJson<SubscribeBody>,
) -> Result<StatusCode, AppError> {
    if state.notify.public_key().is_none() {
        return Err(AppError::PushDisabled);
    }
    validate_endpoint(&body.endpoint)?;
    validate_keys(&body.keys.p256dh, &body.keys.auth)?;
    let user_agent: Option<String> = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|ua| ua.chars().take(MAX_USER_AGENT_CHARS).collect());
    let values = [
        Uuid::new_v4().into(),
        auth.id.into(),
        body.endpoint.into(),
        body.keys.p256dh.trim().into(),
        body.keys.auth.trim().into(),
        user_agent.into(),
        body.lang.as_str().into(),
    ];
    sql::exec(&state.db, UPSERT, values.to_vec()).await?;
    sql::exec(
        &state.db,
        TRIM,
        vec![auth.id.into(), MAX_SUBSCRIPTIONS_PER_USER.into()],
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct UnsubscribeBody {
    endpoint: String,
}

async fn unsubscribe(
    State(state): State<AppState>,
    auth: AuthUser,
    AppJson(body): AppJson<UnsubscribeBody>,
) -> Result<StatusCode, AppError> {
    push_subscription::Entity::delete_many()
        .filter(push_subscription::Column::UserId.eq(auth.id))
        .filter(push_subscription::Column::Endpoint.eq(body.endpoint))
        .exec(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct PrefsDto {
    pub waves: bool,
    pub matches: bool,
    pub messages: bool,
}

impl Default for PrefsDto {
    fn default() -> Self {
        Self {
            waves: true,
            matches: true,
            messages: true,
        }
    }
}

/// The user's push preferences; everything on when never saved.
pub async fn load_prefs(db: &DatabaseConnection, user: Uuid) -> Result<PrefsDto, sea_orm::DbErr> {
    Ok(push_prefs::Entity::find_by_id(user)
        .one(db)
        .await?
        .map_or_else(PrefsDto::default, |p| PrefsDto {
            waves: p.waves,
            matches: p.matches,
            messages: p.messages,
        }))
}

async fn get_prefs(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<PrefsDto>, AppError> {
    Ok(Json(load_prefs(&state.db, auth.id).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PatchPrefs {
    waves: Option<bool>,
    matches: Option<bool>,
    messages: Option<bool>,
}

const UPSERT_PREFS: &str = "INSERT INTO push_prefs (user_id, waves, matches, messages) \
     VALUES ($1, COALESCE($2, true), COALESCE($3, true), COALESCE($4, true)) \
     ON CONFLICT (user_id) DO UPDATE SET waves = COALESCE($2, push_prefs.waves), \
     matches = COALESCE($3, push_prefs.matches), messages = COALESCE($4, push_prefs.messages) \
     RETURNING waves, matches, messages";

async fn patch_prefs(
    State(state): State<AppState>,
    auth: AuthUser,
    AppJson(body): AppJson<PatchPrefs>,
) -> Result<Json<PrefsDto>, AppError> {
    let stmt = Statement::from_sql_and_values(
        DbBackend::Postgres,
        UPSERT_PREFS,
        [
            auth.id.into(),
            body.waves.into(),
            body.matches.into(),
            body.messages.into(),
        ],
    );
    let row = state.db.query_one(stmt).await?.ok_or(AppError::Internal)?;
    Ok(Json(PrefsDto {
        waves: row.try_get("", "waves")?,
        matches: row.try_get("", "matches")?,
        messages: row.try_get("", "messages")?,
    }))
}
