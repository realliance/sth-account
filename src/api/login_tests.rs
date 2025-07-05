#[cfg(test)]
mod tests {
    use axum::http::{Method, StatusCode};
    use axum_test::TestServer;
    use serde_json::json;
    use std::sync::Arc;
    use uuid::Uuid;

    use crate::test_utils::test_utils::*;
    use entity::user;

    #[tokio::test]
    async fn test_login_success() {
        let user_id = Uuid::new_v4();
        let mut mock_user = sample_user(Some(user_id));

        // Generate a proper hash for "password123" - let's first test our hashing
        use crate::auth::Backend;
        let proper_hash = Backend::hash_password("password123").await.unwrap();
        mock_user.password = proper_hash;

        let session_model = entity::user_session::Model {
            id: Uuid::new_v4(),
            user_id: mock_user.id,
            token_hash: "session_token".to_string(),
            device_info: None,
            ip_address: "127.0.0.1".to_string(),
            created_at: chrono::Utc::now().into(),
            expires_at: chrono::Utc::now()
                .checked_add_signed(chrono::Duration::hours(1))
                .unwrap()
                .into(),
            last_active_at: Some(chrono::Utc::now().into()),
            status: "Active".to_string(),
        };

        let db = create_mock_db()
            .append_query_results([
                vec![mock_user.clone()], // User found during authentication
            ])
            // Session creation during login - from session store operations
            .append_query_results([
                Vec::<entity::user_session::Model>::new(), // Session collision check
            ])
            .append_query_results([
                Vec::<entity::user_session::Model>::new(), // Session exists check
            ])
            .append_exec_results([mock_exec_success(1)]) // Session insert
            .append_query_results([
                vec![session_model], // Session returned after insert
            ])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let request_body = json!({
            "username": "testuser",
            "password": "password123"
        });

        let response = server
            .method(Method::POST, "/auth/login")
            .form(&request_body)
            .await;

        // Debug: check what we actually got
        let body: serde_json::Value = response.json();
        if response.status_code() != StatusCode::OK {
            panic!(
                "Login failed with status {}: {:?}",
                response.status_code(),
                body
            );
        }

        assert_eq!(response.status_code(), StatusCode::OK);
        assert_eq!(body["success"], true);
        assert_eq!(body["user_id"], user_id.to_string());
        assert_eq!(body["username"], "testuser");
    }

    #[tokio::test]
    async fn test_login_invalid_username() {
        let db = create_mock_db()
            .append_query_results([
                Vec::<user::Model>::new(), // User not found
            ])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let request_body = json!({
            "username": "nonexistent",
            "password": "password123"
        });

        let response = server
            .method(Method::POST, "/auth/login")
            .form(&request_body)
            .await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);

        let body: serde_json::Value = response.json();
        assert_eq!(body["success"], false);
        assert_eq!(body["message"], "Invalid credentials");
    }

    #[tokio::test]
    async fn test_login_invalid_password() {
        let user_id = Uuid::new_v4();
        let mock_user = sample_user(Some(user_id));

        let db = create_mock_db()
            .append_query_results([
                vec![mock_user], // User found but password will be wrong
            ])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let request_body = json!({
            "username": "testuser",
            "password": "wrongpassword"
        });

        let response = server
            .method(Method::POST, "/auth/login")
            .form(&request_body)
            .await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);

        let body: serde_json::Value = response.json();
        assert_eq!(body["success"], false);
        assert_eq!(body["message"], "Invalid credentials");
    }

    #[tokio::test]
    async fn test_logout_success() {
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server.method(Method::POST, "/api/v1/auth/logout").await;

        assert_eq!(response.status_code(), StatusCode::OK);

        let body: serde_json::Value = response.json();
        assert_eq!(body["success"], true);
        assert_eq!(body["message"], "Logout successful");
    }

    #[tokio::test]
    async fn test_me_authenticated() {
        let user_id = Uuid::new_v4();
        let mock_user = sample_user(Some(user_id));

        let db = create_mock_db()
            .append_query_results([
                vec![mock_user.clone()], // Current authenticated user
            ])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server.method(Method::GET, "/api/v1/auth/me").await;

        assert_eq!(response.status_code(), StatusCode::OK);

        let body: serde_json::Value = response.json();
        if !body.is_null() {
            assert_eq!(body["user_id"], user_id.to_string());
            assert_eq!(body["username"], "testuser");
            assert_eq!(body["role"], "Active");
        }
    }

    #[tokio::test]
    async fn test_me_not_authenticated() {
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server.method(Method::GET, "/api/v1/auth/me").await;

        assert_eq!(response.status_code(), StatusCode::OK);

        let body: serde_json::Value = response.json();
        assert!(body.is_null()); // No authenticated user
    }
}
