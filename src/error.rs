use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// Domain, infrastructure, and HTTP error type.
/// Public error responses always resolve through i18n keys without exposing internal details.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("validation error: {0}")]
    Validation(&'static str),

    #[error("unauthorized: {0}")]
    Unauthorized(&'static str),

    #[error("forbidden: {0}")]
    Forbidden(&'static str),

    #[error("not found: {0}")]
    NotFound(&'static str),

    #[error("conflict: {0}")]
    Conflict(&'static str),

    #[error("rate limited")]
    RateLimited,

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    /// Returns the translation key associated with this error for user-facing messages.
    pub fn i18n_key(&self) -> &'static str {
        match self {
            Self::Validation(key)
            | Self::Unauthorized(key)
            | Self::Forbidden(key)
            | Self::NotFound(key)
            | Self::Conflict(key) => key,
            Self::RateLimited => "error.rate_limited",
            Self::Database(_) | Self::Io(_) | Self::Internal(_) => "error.internal",
        }
    }

    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Database(_) | Self::Io(_) | Self::Internal(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        if status.is_server_error() {
            tracing::error!(error = %self, "request failed with internal error");
        } else {
            tracing::debug!(error = %self, status = %status, "request rejected");
        }
        (status, self.i18n_key()).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
