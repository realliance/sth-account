use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    auth::{AuthSession, Backend},
    error::{AppError, Result},
};
use entity::user;

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub username: String,
    pub country: String,
    pub favorite_tile: Option<String>,
    pub pronouns: Option<String>,
    pub matchmaking_rank: i32,
    pub role: String,
    pub email: Option<String>,
    pub account_status: String,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub last_active_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}

impl From<user::Model> for UserResponse {
    fn from(user: user::Model) -> Self {
        Self {
            id: user.id,
            username: user.username,
            country: user.country,
            favorite_tile: user.favorite_tile,
            pronouns: user.pronouns,
            matchmaking_rank: user.matchmaking_rank,
            role: user.role,
            email: user.email,
            account_status: user.account_status,
            created_at: user.created_at,
            last_active_at: user.last_active_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub country: String,
    pub favorite_tile: Option<String>,
    pub pronouns: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub country: Option<String>,
    pub favorite_tile: Option<String>,
    pub pronouns: Option<String>,
    pub email: Option<String>,
}

pub async fn create_user(
    State(db): State<Arc<DatabaseConnection>>,
    mut headers: HeaderMap,
    Json(request): Json<CreateUserRequest>,
) -> Result<(StatusCode, HeaderMap, Json<UserResponse>)> {
    super::add_rate_limit_headers(&mut headers);

    let existing_user = user::Entity::find()
        .filter(user::Column::Username.eq(&request.username))
        .filter(user::Column::DeletedAt.is_null())
        .one(db.as_ref())
        .await?;

    if existing_user.is_some() {
        return Err(AppError::Service("Username already exists".to_string()));
    }

    let password_hash = Backend::hash_password(&request.password).await?;

    let new_user = user::ActiveModel {
        id: Set(Uuid::new_v4()),
        username: Set(request.username),
        country: Set(request.country),
        favorite_tile: Set(request.favorite_tile),
        pronouns: Set(request.pronouns),
        matchmaking_rank: Set(1000), // Default MMR
        password: Set(password_hash),
        created_at: Set(Utc::now().into()),
        role: Set("Active".to_string()),
        email: Set(request.email),
        account_status: Set("Active".to_string()),
        last_active_at: Set(Some(Utc::now().into())),
        passkey: Set(None),
        settings: Set(None),
        deleted_at: Set(None),
    };

    let user_model = new_user.insert(db.as_ref()).await?;
    let user_response = UserResponse::from(user_model);

    Ok((StatusCode::CREATED, headers, Json(user_response)))
}

pub async fn get_user(
    State(db): State<Arc<DatabaseConnection>>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Path(user_id): Path<Uuid>,
) -> Result<(StatusCode, HeaderMap, Json<UserResponse>)> {
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Users can only view their own profile unless they're admin/moderator
    if current_user.id != user_id && !["Admin", "Moderator"].contains(&current_user.role.as_str()) {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let user_model = user::Entity::find_by_id(user_id)
        .filter(user::Column::DeletedAt.is_null())
        .one(db.as_ref())
        .await?
        .ok_or_else(|| AppError::Service("User not found".to_string()))?;

    let user_response = UserResponse::from(user_model);
    Ok((StatusCode::OK, headers, Json(user_response)))
}

pub async fn update_user(
    State(db): State<Arc<DatabaseConnection>>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Path(user_id): Path<Uuid>,
    Json(request): Json<UpdateUserRequest>,
) -> Result<(StatusCode, HeaderMap, Json<UserResponse>)> {
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Users can only update their own profile unless they're admin/moderator
    if current_user.id != user_id && !["Admin", "Moderator"].contains(&current_user.role.as_str()) {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let user_model = user::Entity::find_by_id(user_id)
        .filter(user::Column::DeletedAt.is_null())
        .one(db.as_ref())
        .await?
        .ok_or_else(|| AppError::Service("User not found".to_string()))?;

    let mut user_update: user::ActiveModel = user_model.into();

    if let Some(country) = request.country {
        user_update.country = Set(country);
    }
    if let Some(favorite_tile) = request.favorite_tile {
        user_update.favorite_tile = Set(Some(favorite_tile));
    }
    if let Some(pronouns) = request.pronouns {
        user_update.pronouns = Set(Some(pronouns));
    }
    if let Some(email) = request.email {
        user_update.email = Set(Some(email));
    }

    let updated_user = user_update.update(db.as_ref()).await?;
    let user_response = UserResponse::from(updated_user);

    Ok((StatusCode::OK, headers, Json(user_response)))
}

pub async fn delete_user(
    State(db): State<Arc<DatabaseConnection>>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Path(user_id): Path<Uuid>,
) -> Result<(StatusCode, HeaderMap, Json<serde_json::Value>)> {
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Users can only delete their own account unless they're admin
    if current_user.id != user_id && current_user.role != "Admin" {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let user_model = user::Entity::find_by_id(user_id)
        .filter(user::Column::DeletedAt.is_null())
        .one(db.as_ref())
        .await?
        .ok_or_else(|| AppError::Service("User not found".to_string()))?;

    // Soft delete by setting deleted_at timestamp
    let mut user_update: user::ActiveModel = user_model.into();
    user_update.deleted_at = Set(Some(Utc::now().into()));

    // Anonymize username for data retention compliance
    let anonymous_id = Uuid::new_v4()
        .to_string()
        .chars()
        .take(8)
        .collect::<String>();
    user_update.username = Set(format!("Anonymous_User_{}", anonymous_id));

    user_update.update(db.as_ref()).await?;

    Ok((
        StatusCode::OK,
        headers,
        Json(serde_json::json!({"message": "User deleted successfully"})),
    ))
}
