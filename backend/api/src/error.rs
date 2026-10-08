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
    #[error("internal server error")]
    Internal,
}

impl AppError {
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    pub fn status(&self) -> StatusCode {
        match self {
            Self::Validation(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized | Self::InvalidCredentials | Self::InvalidRefreshToken => {
                StatusCode::UNAUTHORIZED
            }
            Self::Forbidden | Self::Banned => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::UsernameTaken
            | Self::NoActiveWindow
            | Self::NotVisible
            | Self::CannotBanAdmin
            | Self::AlreadyResolved => StatusCode::CONFLICT,
            Self::Underage
            | Self::ProfileIncomplete
            | Self::PhotoLimit
            | Self::UnsupportedImage => StatusCode::UNPROCESSABLE_ENTITY,
            Self::RateLimited | Self::WaveLimit => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
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
            Self::Underage => "underage",
            Self::ProfileIncomplete => "profile_incomplete",
            Self::PhotoLimit => "photo_limit",
            Self::UnsupportedImage => "unsupported_image",
            Self::RateLimited => "rate_limited",
            Self::WaveLimit => "wave_limit",
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
            (AppError::Underage, 422, "underage"),
            (AppError::ProfileIncomplete, 422, "profile_incomplete"),
            (AppError::PhotoLimit, 422, "photo_limit"),
            (AppError::UnsupportedImage, 422, "unsupported_image"),
            (AppError::RateLimited, 429, "rate_limited"),
            (AppError::WaveLimit, 429, "wave_limit"),
            (AppError::Internal, 500, "internal"),
        ];
        for (err, status, code) in cases {
            assert_eq!(err.status().as_u16(), status);
            assert_eq!(err.code(), code);
        }
    }
}
