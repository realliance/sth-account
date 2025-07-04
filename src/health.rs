use axum::{http::StatusCode, response::Json};
use serde_json::{Value, json};

pub async fn health_check() -> (StatusCode, Json<Value>) {
    (
        StatusCode::OK,
        Json(json!({
            "status": "healthy",
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "service": "sth-account"
        })),
    )
}

pub async fn readiness_check() -> (StatusCode, Json<Value>) {
    // TODO: Add database connectivity check
    (
        StatusCode::OK,
        Json(json!({
            "status": "ready",
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "service": "sth-account"
        })),
    )
}
