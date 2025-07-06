use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use async_trait::async_trait;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use axum_login::{AuthUser, AuthnBackend, UserId};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::{AppError, Result};
use entity::user;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub role: String,
    pub account_status: String,
}

impl AuthUser for User {
    type Id = Uuid;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn session_auth_hash(&self) -> &[u8] {
        self.username.as_bytes()
    }
}

#[derive(Debug, Clone)]
pub struct Backend {
    db: Arc<DatabaseConnection>,
}

impl Backend {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    pub async fn hash_password(password: &str) -> Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let password_hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| AppError::Service(format!("Password hashing failed: {e}")))?;
        Ok(password_hash.to_string())
    }

    pub async fn verify_password(password: &str, hash: &str) -> Result<bool> {
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|e| AppError::Service(format!("Invalid password hash: {e}")))?;
        let argon2 = Argon2::default();
        Ok(argon2
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    }
}

#[async_trait]
impl AuthnBackend for Backend {
    type User = User;
    type Credentials = Credentials;
    type Error = AppError;

    async fn authenticate(&self, creds: Self::Credentials) -> Result<Option<Self::User>> {
        let user_model = user::Entity::find()
            .filter(user::Column::Username.eq(&creds.username))
            .filter(user::Column::DeletedAt.is_null())
            .one(self.db.as_ref())
            .await?;

        if let Some(user) = user_model {
            if Self::verify_password(&creds.password, &user.password).await? {
                return Ok(Some(User {
                    id: user.id,
                    username: user.username,
                    role: user.role,
                    account_status: user.account_status,
                }));
            }
        }

        Ok(None)
    }

    async fn get_user(&self, user_id: &UserId<Self>) -> Result<Option<Self::User>> {
        let user_model = user::Entity::find_by_id(*user_id)
            .filter(user::Column::DeletedAt.is_null())
            .one(self.db.as_ref())
            .await?;

        Ok(user_model.map(|user| User {
            id: user.id,
            username: user.username,
            role: user.role,
            account_status: user.account_status,
        }))
    }
}

#[derive(Debug, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthError {
    pub message: String,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        (StatusCode::UNAUTHORIZED, Json(self)).into_response()
    }
}

pub type AuthSession = axum_login::AuthSession<Backend>;
