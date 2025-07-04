use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),

    #[error("Migration error: {0}")]
    Migration(String),

    #[error("Service error: {0}")]
    Service(String),

    #[error("Worker error: {0}")]
    Worker(String),

    #[error("Job error: {0}")]
    Job(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Authorization error: {0}")]
    Forbidden(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::Database(ref e) => {
                tracing::error!("Database error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
            }
            AppError::Migration(ref e) => {
                tracing::error!("Migration error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "Migration error")
            }
            AppError::Service(ref e) => {
                tracing::error!("Service error: {}", e);
                (StatusCode::BAD_REQUEST, e.as_str())
            }
            AppError::Worker(ref e) => {
                tracing::error!("Worker error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "Worker error")
            }
            AppError::Job(ref e) => {
                tracing::error!("Job error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "Job error")
            }
            AppError::Config(ref e) => {
                tracing::error!("Configuration error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "Configuration error")
            }
            AppError::Io(ref e) => {
                tracing::error!("IO error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "IO error")
            }
            AppError::Auth(ref e) => {
                tracing::warn!("Authentication error: {}", e);
                (StatusCode::UNAUTHORIZED, e.as_str())
            }
            AppError::Forbidden(ref e) => {
                tracing::warn!("Authorization error: {}", e);
                (StatusCode::FORBIDDEN, e.as_str())
            }
            AppError::NotFound(ref e) => {
                tracing::info!("Not found: {}", e);
                (StatusCode::NOT_FOUND, e.as_str())
            }
            AppError::BadRequest(ref e) => {
                tracing::info!("Bad request: {}", e);
                (StatusCode::BAD_REQUEST, e.as_str())
            }
            AppError::Validation(ref e) => {
                tracing::info!("Validation error: {}", e);
                (StatusCode::BAD_REQUEST, e.as_str())
            }
            AppError::Other(ref e) => {
                tracing::error!("Other error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
            }
        };

        let body = Json(json!({
            "error": error_message,
            "status": status.as_u16()
        }));

        (status, body).into_response()
    }
}
