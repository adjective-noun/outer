//! Error types for the application

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("OpenCode error: {0}")]
    OpenCode(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::Database(e) => {
                tracing::error!("Database error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
            }
            AppError::OpenCode(e) => {
                tracing::error!("OpenCode error: {}", e);
                (StatusCode::BAD_GATEWAY, format!("OpenCode error: {}", e))
            }
            AppError::NotFound(e) => (StatusCode::NOT_FOUND, e.clone()),
            AppError::BadRequest(e) => (StatusCode::BAD_REQUEST, e.clone()),
            AppError::Internal(e) => {
                tracing::error!("Internal error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, e.clone())
            }
        };

        (status, message).into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
