pub mod admin;
pub mod auth;
pub mod bots;
pub mod exports;
pub mod friends;
pub mod lobbies;
pub mod matches;
pub mod matchmaking;
pub mod notifications;
pub mod rooms;
pub mod users;

#[cfg(test)]
mod login_tests;

use axum::http::{HeaderMap, HeaderName, HeaderValue};
use std::str::FromStr;

pub fn add_rate_limit_headers(headers: &mut HeaderMap) {
    headers.insert(
        HeaderName::from_str("X-Rate-Limit-Limit").unwrap(),
        HeaderValue::from_static("100"),
    );
    headers.insert(
        HeaderName::from_str("X-Rate-Limit-Remaining").unwrap(),
        HeaderValue::from_static("99"),
    );
    headers.insert(
        HeaderName::from_str("X-Rate-Limit-Reset").unwrap(),
        HeaderValue::from_static("3600"),
    );
}

#[cfg(test)]
mod tests {
    use axum::http::{Method, StatusCode};
    use axum_test::TestServer;
    use std::sync::Arc;

    use crate::test_utils::test_utils::*;

    #[tokio::test]
    async fn test_rate_limit_headers_concept() {
        let mut headers = axum::http::HeaderMap::new();

        crate::api::add_rate_limit_headers(&mut headers);

        assert!(headers.contains_key("X-Rate-Limit-Limit"));
        assert!(headers.contains_key("X-Rate-Limit-Remaining"));
        assert!(headers.contains_key("X-Rate-Limit-Reset"));
    }

    #[tokio::test]
    async fn test_routes_exist() {
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let endpoints = [
            "/v1/friends",
            "/v1/friends/requests",
            "/v1/notifications",
            "/v1/notifications/summary",
            "/v1/exports",
            "/v1/admin/reports",
            "/v1/admin/config",
            "/v1/admin/audit-logs",
        ];

        for endpoint in &endpoints {
            let response = server.method(Method::GET, endpoint).await;

            assert_ne!(
                response.status_code(),
                StatusCode::NOT_FOUND,
                "Endpoint {endpoint} should exist (not return 404)"
            );
        }
    }

    #[tokio::test]
    async fn test_json_response_format() {
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server.method(Method::GET, "/v1/friends").await;

        let content_type = response
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap_or(""))
            .unwrap_or("");

        assert!(
            content_type.contains("application/json"),
            "Response should be JSON format"
        );

        let _: serde_json::Value = response.json();
    }
}
