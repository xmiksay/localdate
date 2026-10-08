use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::de::DeserializeOwned;
use serde_json::json;

/// Every error code of docs/api.md "Errors". Handlers return `Result<_, AppError>`.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Validation(String),
    #[error("missing or invalid access token")]
    Unauthorized,
    #[error("invalid username or password")]
    InvalidCredentials,
    #[error("invalid refresh token")]
    InvalidRefreshToken,
    #[error("forbidden")]
    Forbidden,
    #[error("this account has been suspended")]
    Banned,
    #[error("not found")]
    NotFound,
    #[error("username already taken")]
    UsernameTaken,
    #[error("no active visibility window")]
    NoActiveWindow,
    #[error("user is not currently visible")]
    NotVisible,
    #[error("admins cannot be banned")]
    CannotBanAdmin,
    #[error("report is already resolved")]
    AlreadyResolved,
    #[error("you are not inside this area")]
    OutsideArea,
    #[error("you left the area, the window has ended")]
    LeftArea,
    #[error("less than 30 minutes are left until midnight")]
    TooCloseToMidnight,
    #[error("area is referenced by windows, deactivate it instead")]
    AreaInUse,
    #[error("push notifications are not configured on this server")]
    PushDisabled,
    #[error("you must be at least 18 years old")]
    Underage,
    #[error("profile, filter and at least one photo are required")]
    ProfileIncomplete,
    #[error("photo limit reached")]
    PhotoLimit,
    #[error("unsupported or too large image")]
    UnsupportedImage,
    #[error("too many requests")]
    RateLimited,
    #[error("wave limit reached for this window")]
    WaveLimit,
    #[error("invalid, expired or already used link")]
    InvalidToken,
    #[error("the account would be left without a way to log in")]
    LastLoginMethod,
    #[error("email is not available on this server")]
    EmailDisabled,
    #[error("this login provider is not available on this server")]
    ProviderDisabled,
    #[error("this account is already linked to another user")]
    IdentityTaken,
    #[error("internal server error")]
    Internal,
}

impl AppError {
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    pub fn status(&self) -> StatusCode {
        match self {
            Self::Validation(_) | Self::InvalidToken => StatusCode::BAD_REQUEST,
            Self::Unauthorized | Self::InvalidCredentials | Self::InvalidRefreshToken => {
                StatusCode::UNAUTHORIZED
            }
            Self::Forbidden | Self::Banned => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::UsernameTaken
            | Self::NoActiveWindow
            | Self::NotVisible
            | Self::CannotBanAdmin
            | Self::AlreadyResolved
            | Self::OutsideArea
            | Self::LeftArea
            | Self::TooCloseToMidnight
            | Self::AreaInUse
            | Self::LastLoginMethod
            | Self::IdentityTaken
            | Self::PushDisabled => StatusCode::CONFLICT,
            Self::Underage
            | Self::ProfileIncomplete
            | Self::PhotoLimit
            | Self::UnsupportedImage => StatusCode::UNPROCESSABLE_ENTITY,
            Self::RateLimited | Self::WaveLimit => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            Self::EmailDisabled | Self::ProviderDisabled => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation(_) => "validation",
            Self::Unauthorized => "unauthorized",
            Self::InvalidCredentials => "invalid_credentials",
            Self::InvalidRefreshToken => "invalid_refresh_token",
            Self::Forbidden => "forbidden",
            Self::Banned => "banned",
            Self::NotFound => "not_found",
            Self::UsernameTaken => "username_taken",
            Self::NoActiveWindow => "no_active_window",
            Self::NotVisible => "not_visible",
            Self::CannotBanAdmin => "cannot_ban_admin",
            Self::AlreadyResolved => "already_resolved",
            Self::OutsideArea => "outside_area",
            Self::LeftArea => "left_area",
            Self::TooCloseToMidnight => "too_close_to_midnight",
            Self::AreaInUse => "area_in_use",
            Self::PushDisabled => "push_disabled",
            Self::Underage => "underage",
            Self::ProfileIncomplete => "profile_incomplete",
            Self::PhotoLimit => "photo_limit",
            Self::UnsupportedImage => "unsupported_image",
            Self::RateLimited => "rate_limited",
            Self::WaveLimit => "wave_limit",
            Self::InvalidToken => "invalid_token",
            Self::LastLoginMethod => "last_login_method",
            Self::EmailDisabled => "email_disabled",
            Self::ProviderDisabled => "provider_disabled",
            Self::IdentityTaken => "identity_taken",
            Self::Internal => "internal",
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let body = json!({ "error": { "code": self.code(), "message": self.to_string() } });
        (self.status(), Json(body)).into_response()
    }
}

impl From<sea_orm::DbErr> for AppError {
    fn from(err: sea_orm::DbErr) -> Self {
        tracing::error!(error = %err, "database error");
        Self::Internal
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        tracing::error!(error = ?err, "internal error");
        Self::Internal
    }
}

/// Path ids that aren't UUIDs can't name anything, so they are a plain 404 (not axum's text/plain 400).
pub fn parse_id(raw: &str) -> Result<uuid::Uuid, AppError> {
    raw.parse().map_err(|_| AppError::NotFound)
}

/// `axum::Json` whose rejections become the contract's `400 validation` envelope.
pub struct AppJson<T>(pub T);

impl<S, T> FromRequest<S> for AppJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(Self(value)),
            Err(rejection) => Err(json_rejection(rejection)),
        }
    }
}

fn json_rejection(rejection: JsonRejection) -> AppError {
    AppError::validation(format!("invalid JSON body: {}", rejection.body_text()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    #[tokio::test]
    async fn renders_error_envelope() {
        let resp = AppError::UsernameTaken.into_response();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["error"]["code"], "username_taken");
        assert!(v["error"]["message"].is_string());
    }

    #[tokio::test]
    async fn internal_errors_do_not_leak_details() {
        let err: AppError = anyhow::anyhow!("secret connection string").into();
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
        let text = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(!text.contains("secret"));
        assert!(text.contains("\"internal\""));
    }

    #[test]
    fn status_and_code_match_contract() {
        let cases = [
            (AppError::validation("x"), 400, "validation"),
            (AppError::Unauthorized, 401, "unauthorized"),
            (AppError::InvalidCredentials, 401, "invalid_credentials"),
            (AppError::InvalidRefreshToken, 401, "invalid_refresh_token"),
            (AppError::Forbidden, 403, "forbidden"),
            (AppError::Banned, 403, "banned"),
            (AppError::NotFound, 404, "not_found"),
            (AppError::UsernameTaken, 409, "username_taken"),
            (AppError::NoActiveWindow, 409, "no_active_window"),
            (AppError::NotVisible, 409, "not_visible"),
            (AppError::CannotBanAdmin, 409, "cannot_ban_admin"),
            (AppError::AlreadyResolved, 409, "already_resolved"),
            (AppError::OutsideArea, 409, "outside_area"),
            (AppError::LeftArea, 409, "left_area"),
            (AppError::TooCloseToMidnight, 409, "too_close_to_midnight"),
            (AppError::AreaInUse, 409, "area_in_use"),
            (AppError::PushDisabled, 409, "push_disabled"),
            (AppError::Underage, 422, "underage"),
            (AppError::ProfileIncomplete, 422, "profile_incomplete"),
            (AppError::PhotoLimit, 422, "photo_limit"),
            (AppError::UnsupportedImage, 422, "unsupported_image"),
            (AppError::RateLimited, 429, "rate_limited"),
            (AppError::WaveLimit, 429, "wave_limit"),
            (AppError::InvalidToken, 400, "invalid_token"),
            (AppError::LastLoginMethod, 409, "last_login_method"),
            (AppError::EmailDisabled, 503, "email_disabled"),
            (AppError::ProviderDisabled, 503, "provider_disabled"),
            (AppError::IdentityTaken, 409, "identity_taken"),
            (AppError::Internal, 500, "internal"),
        ];
        for (err, status, code) in cases {
            assert_eq!(err.status().as_u16(), status);
            assert_eq!(err.code(), code);
        }
    }
}
