use axum::{
    http::{HeaderMap, StatusCode},
    response::Json,
    Form,
};
use serde::{Deserialize, Serialize};

use crate::{
    auth::{AuthSession, Credentials},
    error::{AppError, Result},
};

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub success: bool,
    pub user_id: Option<uuid::Uuid>,
    pub username: Option<String>,
    pub message: String,
}

pub async fn login(
    mut auth_session: AuthSession,
    mut headers: HeaderMap,
    Form(request): Form<LoginRequest>,
) -> Result<(StatusCode, HeaderMap, Json<LoginResponse>)> {
    super::add_rate_limit_headers(&mut headers);

    let creds = Credentials {
        username: request.username.clone(),
        password: request.password,
    };

    let user = match auth_session.authenticate(creds).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return Ok((
                StatusCode::UNAUTHORIZED,
                headers,
                Json(LoginResponse {
                    success: false,
                    user_id: None,
                    username: None,
                    message: "Invalid credentials".to_string(),
                }),
            ));
        }
        Err(e) => return Err(crate::error::AppError::Auth(format!("Authentication failed: {}", e))),
    };

    if let Err(e) = auth_session.login(&user).await {
        return Err(AppError::Service(format!("Failed to create session: {}", e)));
    }

    Ok((
        StatusCode::OK,
        headers,
        Json(LoginResponse {
            success: true,
            user_id: Some(user.id),
            username: Some(user.username.clone()),
            message: "Login successful".to_string(),
        }),
    ))
}

pub async fn logout(
    mut auth_session: AuthSession,
    mut headers: HeaderMap,
) -> Result<(StatusCode, HeaderMap, Json<LoginResponse>)> {
    super::add_rate_limit_headers(&mut headers);

    match auth_session.logout().await {
        Ok(_) => Ok((
            StatusCode::OK,
            headers,
            Json(LoginResponse {
                success: true,
                user_id: None,
                username: None,
                message: "Logout successful".to_string(),
            }),
        )),
        Err(e) => Err(AppError::Service(format!("Failed to logout: {}", e))),
    }
}

#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub user_id: uuid::Uuid,
    pub username: String,
    pub role: String,
}

pub async fn me(
    auth_session: AuthSession,
    mut headers: HeaderMap,
) -> Result<(StatusCode, HeaderMap, Json<Option<UserInfo>>)> {
    super::add_rate_limit_headers(&mut headers);

    let user_info = auth_session.user.map(|user| UserInfo {
        user_id: user.id,
        username: user.username,
        role: user.role,
    });

    Ok((StatusCode::OK, headers, Json(user_info)))
}

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
    async fn test_create_user_success() {
        let user_id = Uuid::new_v4();
        let mock_user = sample_user(Some(user_id));

        // Mock database: first query checks if username exists (should return None)
        // Second operation inserts the new user
        let db = create_mock_db()
            .append_query_results([
                Vec::<user::Model>::new(), // No existing user with this username
            ])
            .append_exec_results([mock_exec_success(1)])
            .append_query_results([
                vec![mock_user.clone()], // Return the inserted user
            ])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let request_body = json!({
            "username": "newuser",
            "password": "password123",
            "country": "USA",
            "favorite_tile": "Man1",
            "pronouns": "they/them",
            "email": "newuser@example.com"
        });

        let response = server
            .method(Method::POST, "/users")
            .json(&request_body)
            .await;

        assert_eq!(response.status_code(), StatusCode::CREATED);
        
        let body: serde_json::Value = response.json();
        assert_eq!(body["username"], "testuser"); // From mock_user
        assert_eq!(body["country"], "USA");
        assert_eq!(body["role"], "Active");
    }

    #[tokio::test]
    async fn test_create_user_duplicate_username() {
        let existing_user = sample_user(None);

        // Mock database: username already exists
        let db = create_mock_db()
            .append_query_results([
                vec![existing_user], // User already exists
            ])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let request_body = json!({
            "username": "testuser",
            "password": "password123",
            "country": "USA"
        });

        let response = server
            .method(Method::POST, "/users")
            .json(&request_body)
            .await;

        assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
        
        let body: serde_json::Value = response.json();
        assert!(body["error"].as_str().unwrap().contains("Username already exists"));
    }

    #[tokio::test]
    async fn test_get_user_success() {
        let user_id = Uuid::new_v4();
        let mut mock_user = sample_user(Some(user_id));
        
        // Generate proper password hash
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
            expires_at: chrono::Utc::now().checked_add_signed(chrono::Duration::hours(1)).unwrap().into(),
            last_active_at: Some(chrono::Utc::now().into()),
            status: "Active".to_string(),
        };

        // Mock database with queries for: login, session creation, and user lookup
        let db = create_mock_db()
            // Login authentication
            .append_query_results([
                vec![mock_user.clone()], // User found during login authentication
            ])
            // Session creation during login
            .append_query_results([
                Vec::<entity::user_session::Model>::new(), // Session collision check
            ])
            .append_query_results([
                Vec::<entity::user_session::Model>::new(), // Session exists check
            ])
            .append_exec_results([mock_exec_success(1)]) // Session insert
            .append_query_results([
                vec![session_model.clone()], // Session returned after insert
            ])
            // Protected endpoint: user lookup for auth context
            .append_query_results([
                vec![session_model.clone()], // Session lookup for auth context
            ])
            .append_query_results([
                vec![mock_user.clone()], // User lookup for auth context
            ])
            // Protected endpoint: actual user lookup
            .append_query_results([
                vec![mock_user.clone()], // The actual get user request
            ])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        // First, perform login to establish authenticated session
        let login_request = json!({
            "username": "testuser",
            "password": "password123"
        });
        
        let login_response = server
            .method(Method::POST, "/auth/login")
            .form(&login_request)
            .await;
        
        assert_eq!(login_response.status_code(), StatusCode::OK);

        // Extract session cookie from login response
        let session_cookie = login_response
            .headers()
            .get("set-cookie")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        // Now make the protected request with the session cookie
        let response = server
            .method(Method::GET, &format!("/users/{}", user_id))
            .add_header("Cookie", session_cookie)
            .await;

        // Debug: check what we actually got  
        if response.status_code() != StatusCode::OK {
            let body: serde_json::Value = response.json();
            println!("Session cookie: '{}'", session_cookie);
            panic!("Protected endpoint failed with status {}: {:?}", response.status_code(), body);
        }

        assert_eq!(response.status_code(), StatusCode::OK);
        
        let body: serde_json::Value = response.json();
        assert_eq!(body["id"], user_id.to_string());
        assert_eq!(body["username"], "testuser");
    }

    #[tokio::test]
    async fn test_get_user_not_found() {
        let user_id = Uuid::new_v4();
        let auth_user = sample_user(None);

        // Mock: auth user exists but requested user doesn't
        let db = create_mock_db()
            .append_query_results([
                vec![auth_user], // For auth session
                Vec::<user::Model>::new(), // Requested user not found
            ])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::GET, &format!("/users/{}", user_id))
            .await;

        assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
        
        let body: serde_json::Value = response.json();
        assert!(body["error"].as_str().unwrap().contains("User not found"));
    }

    #[tokio::test]
    async fn test_update_user_success() {
        let user_id = Uuid::new_v4();
        let mock_user = sample_user(Some(user_id));
        let mut updated_user = mock_user.clone();
        updated_user.country = "CAN".to_string();

        let db = create_mock_db()
            .append_query_results([
                vec![mock_user.clone()], // Auth user lookup
                vec![mock_user.clone()], // Find user to update
            ])
            .append_exec_results([mock_exec_success(1)]) // Update operation
            .append_query_results([
                vec![updated_user.clone()], // Return updated user
            ])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let request_body = json!({
            "country": "CAN",
            "pronouns": "she/her"
        });

        let response = server
            .method(Method::PATCH, &format!("/users/{}", user_id))
            .json(&request_body)
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
        
        let body: serde_json::Value = response.json();
        assert_eq!(body["country"], "CAN");
    }

    #[tokio::test]
    async fn test_delete_user_success() {
        let user_id = Uuid::new_v4();
        let mock_user = sample_user(Some(user_id));

        let db = create_mock_db()
            .append_query_results([
                vec![mock_user.clone()], // Auth user lookup
                vec![mock_user.clone()], // Find user to delete
            ])
            .append_exec_results([mock_exec_success(1)]) // Soft delete operation
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::DELETE, &format!("/users/{}", user_id))
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
        
        let body: serde_json::Value = response.json();
        assert_eq!(body["message"], "User deleted successfully");
    }
}
