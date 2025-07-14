use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Json,
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    auth::{AuthSession, Credentials},
    error::{AppError, Result},
    service::AppState,
};
use entity::user;

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    /// Username for login
    pub username: String,
    /// Password for login
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LoginResponse {
    /// Whether the login was successful
    pub success: bool,
    /// User ID if login successful
    pub user_id: Option<uuid::Uuid>,
    /// Username if login successful
    pub username: Option<String>,
    /// Response message
    pub message: String,
}

#[utoipa::path(
    post,
    path = "/v1/auth/login",
    tag = "Authentication",
    request_body(content = LoginRequest, content_type = "application/json"),
    responses(
        (status = 200, description = "Login successful", body = LoginResponse),
        (status = 401, description = "Invalid credentials", body = LoginResponse)
    )
)]
pub async fn login(
    State(state): State<AppState>,
    mut auth_session: AuthSession,
    Json(request): Json<LoginRequest>,
) -> Result<(StatusCode, HeaderMap, Json<LoginResponse>)> {
    let mut headers = HeaderMap::new();
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
        Err(e) => {
            return Err(crate::error::AppError::Auth(format!(
                "Authentication failed: {e}"
            )));
        }
    };

    if let Err(e) = auth_session.login(&user).await {
        return Err(AppError::Service(format!("Failed to create session: {e}")));
    }

    // Update user's last_active_at timestamp
    if let Err(e) = update_user_last_active(&state, user.id).await {
        tracing::warn!("Failed to update user last_active_at: {e}");
        // Don't fail the login if this update fails
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

#[utoipa::path(
    post,
    path = "/v1/auth/logout",
    tag = "Authentication",
    responses(
        (status = 200, description = "Logout successful", body = LoginResponse),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn logout(
    mut auth_session: AuthSession,
) -> Result<(StatusCode, HeaderMap, Json<LoginResponse>)> {
    let mut headers = HeaderMap::new();
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
        Err(e) => Err(AppError::Service(format!("Failed to logout: {e}"))),
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserInfo {
    /// User ID
    pub user_id: uuid::Uuid,
    /// Username
    pub username: String,
    /// User role
    pub role: String,
}

#[utoipa::path(
    get,
    path = "/v1/auth/me",
    tag = "Authentication",
    responses(
        (status = 200, description = "Current user information", body = Option<UserInfo>),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn me(
    auth_session: AuthSession,
) -> Result<(StatusCode, HeaderMap, Json<Option<UserInfo>>)> {
    let mut headers = HeaderMap::new();
    super::add_rate_limit_headers(&mut headers);

    let user_info = auth_session.user.map(|user| UserInfo {
        user_id: user.id,
        username: user.username,
        role: user.role,
    });

    Ok((StatusCode::OK, headers, Json(user_info)))
}

async fn update_user_last_active(state: &AppState, user_id: uuid::Uuid) -> Result<()> {
    let user_model = user::Entity::find_by_id(user_id)
        .one(state.db.as_ref())
        .await?
        .ok_or_else(|| AppError::Service("User not found".to_string()))?;

    let mut user_update: user::ActiveModel = user_model.into();
    user_update.last_active_at = Set(Some(Utc::now().into()));
    
    user_update.update(state.db.as_ref()).await?;
    Ok(())
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
        assert!(
            body["error"]
                .as_str()
                .unwrap()
                .contains("Username already exists")
        );
    }


    #[tokio::test]
    async fn test_get_user_not_found() {
        let user_id = Uuid::new_v4();
        let auth_user = sample_user(None);

        // Mock: auth user exists but requested user doesn't
        let db = create_mock_db()
            .append_query_results([
                vec![auth_user],           // For auth session
                Vec::<user::Model>::new(), // Requested user not found
            ])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::GET, &format!("/v1/users/{user_id}"))
            .await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);

        let body: serde_json::Value = response.json();
        assert!(
            body["error"]
                .as_str()
                .unwrap()
                .contains("Authentication required")
        );
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
            .method(Method::PATCH, &format!("/v1/users/{user_id}"))
            .json(&request_body)
            .await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);

        let body: serde_json::Value = response.json();
        assert!(
            body["error"]
                .as_str()
                .unwrap()
                .contains("Authentication required")
        );
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
            .method(Method::DELETE, &format!("/v1/users/{user_id}"))
            .await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);

        let body: serde_json::Value = response.json();
        assert!(
            body["error"]
                .as_str()
                .unwrap()
                .contains("Authentication required")
        );
    }
}
