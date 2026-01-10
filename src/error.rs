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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn test_app_error_display() {
        let err = AppError::NotFound("user".to_string());
        assert_eq!(format!("{}", err), "Not found: user");

        let err = AppError::BadRequest("invalid input".to_string());
        assert_eq!(format!("{}", err), "Bad request: invalid input");

        let err = AppError::OpenCode("connection failed".to_string());
        assert_eq!(format!("{}", err), "OpenCode error: connection failed");

        let err = AppError::Internal("something broke".to_string());
        assert_eq!(format!("{}", err), "Internal error: something broke");
    }

    #[test]
    fn test_app_error_debug() {
        let err = AppError::NotFound("test".to_string());
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("NotFound"));
    }

    #[test]
    fn test_not_found_into_response() {
        let err = AppError::NotFound("resource".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_bad_request_into_response() {
        let err = AppError::BadRequest("bad data".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_opencode_into_response() {
        let err = AppError::OpenCode("upstream error".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    }

    #[test]
    fn test_internal_into_response() {
        let err = AppError::Internal("internal issue".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_database_error_from_sqlx() {
        // Create a mock sqlx error by trying to parse an invalid connection string
        // This tests the From<sqlx::Error> implementation
        let sqlx_err = sqlx::Error::Configuration("test".into());
        let app_err: AppError = sqlx_err.into();
        assert!(matches!(app_err, AppError::Database(_)));
    }

    #[test]
    fn test_database_into_response() {
        let sqlx_err = sqlx::Error::Configuration("test".into());
        let err: AppError = sqlx_err.into();
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_result_type_alias() {
        fn test_fn() -> Result<i32> {
            Ok(42)
        }
        assert_eq!(test_fn().unwrap(), 42);

        fn test_err_fn() -> Result<i32> {
            Err(AppError::NotFound("test".to_string()))
        }
        assert!(test_err_fn().is_err());
    }
}
