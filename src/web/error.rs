use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::errors::HvtError;

/// Wraps any error a web handler can produce into a 500 response, logging the underlying
/// cause. There's no user-facing error detail beyond the message since this is a personal,
/// single-user tool with no untrusted clients (see the security note in the plan/config).
pub struct AppError(HvtError);

pub type AppResult<T> = Result<T, AppError>;

impl From<HvtError> for AppError {
    fn from(e: HvtError) -> Self {
        AppError(e)
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError(HvtError::Database(e))
    }
}

impl From<askama::Error> for AppError {
    fn from(e: askama::Error) -> Self {
        AppError(HvtError::Generic(format!("Template error: {}", e)))
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError(HvtError::Io(e))
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!("web UI error: {}", self.0);
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Internal error: {}", self.0)).into_response()
    }
}
