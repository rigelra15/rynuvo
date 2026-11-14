use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rynuvo_proto::ApiError;

#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    Coded(StatusCode, &'static str, String),
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict(&'static str, String),
    TooLarge,
    DiskFull,
    Internal(anyhow::Error),
}

impl AppError {
    fn parts(&self) -> (StatusCode, &'static str, String) {
        match self {
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, "bad_request", m.clone()),
            AppError::Coded(s, c, m) => (*s, c, m.clone()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", "Invalid session".into()),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden", "You don't have permission".into()),
            AppError::NotFound => (StatusCode::NOT_FOUND, "not_found", "Not found".into()),
            AppError::Conflict(c, m) => (StatusCode::CONFLICT, c, m.clone()),
            AppError::TooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "too_large", "File is too large (max 5 MB)".into()),
            AppError::DiskFull => (StatusCode::INSUFFICIENT_STORAGE, "disk_full", "Server storage is full".into()),
            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal", "Server error".into()),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if let AppError::Internal(e) = &self {
            tracing::error!("internal error: {e:#}");
        }
        let (status, code, message) = self.parts();
        (status, Json(ApiError { code: code.into(), message })).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Internal(e.into())
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Internal(e)
    }
}

pub type AppResult<T> = Result<T, AppError>;
