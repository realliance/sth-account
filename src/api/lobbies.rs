use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::AuthSession,
    error::{AppError, Result},
    service::AppState,
};
use entity::lobby_pool;

#[derive(Debug, Serialize)]
pub struct LobbyResponse {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub preset: String,
    pub active: bool,
}

impl From<lobby_pool::Model> for LobbyResponse {
    fn from(lobby: lobby_pool::Model) -> Self {
        Self {
            id: lobby.id,
            name: lobby.name,
            description: lobby.description,
            preset: lobby.preset,
            active: lobby.active,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateLobbyRequest {
    pub name: String,
    pub description: Option<String>,
    pub preset: LobbyPreset,
}

#[derive(Debug, Deserialize)]
pub struct UpdateLobbyRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub preset: Option<LobbyPreset>,
    pub active: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum LobbyPreset {
    GeneralFourPlayer,
    AllBotsFourPlayer,
}

impl ToString for LobbyPreset {
    fn to_string(&self) -> String {
        match self {
            LobbyPreset::GeneralFourPlayer => "GeneralFourPlayer".to_string(),
            LobbyPreset::AllBotsFourPlayer => "AllBotsFourPlayer".to_string(),
        }
    }
}

impl From<String> for LobbyPreset {
    fn from(s: String) -> Self {
        match s.as_str() {
            "AllBotsFourPlayer" => LobbyPreset::AllBotsFourPlayer,
            _ => LobbyPreset::GeneralFourPlayer, // Default
        }
    }
}

/// Create a new lobby (Admin only)
pub async fn create_lobby(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Json(request): Json<CreateLobbyRequest>,
) -> Result<(StatusCode, HeaderMap, Json<LobbyResponse>)> {
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Only admins can create lobbies
    if current_user.role != "Admin" {
        return Err(AppError::Forbidden(
            "Only admins can create lobbies".to_string(),
        ));
    }

    // Check if lobby name is unique
    let existing_lobby = lobby_pool::Entity::find()
        .filter(lobby_pool::Column::Name.eq(&request.name))
        .one(state.db.as_ref())
        .await?;

    if existing_lobby.is_some() {
        return Err(AppError::Service("Lobby name already exists".to_string()));
    }

    let new_lobby = lobby_pool::ActiveModel {
        id: Set(Uuid::new_v4()),
        name: Set(request.name),
        description: Set(request.description),
        preset: Set(request.preset.to_string()),
        active: Set(true),
    };

    let lobby_model = new_lobby.insert(state.db.as_ref()).await?;
    let lobby_response = LobbyResponse::from(lobby_model);

    Ok((StatusCode::CREATED, headers, Json(lobby_response)))
}

/// Get all lobbies
pub async fn get_lobbies(
    State(state): State<AppState>,
    mut headers: HeaderMap,
) -> Result<(StatusCode, HeaderMap, Json<Vec<LobbyResponse>>)> {
    super::add_rate_limit_headers(&mut headers);

    // Return only active lobbies for public API
    let lobbies = lobby_pool::Entity::find()
        .filter(lobby_pool::Column::Active.eq(true))
        .all(state.db.as_ref())
        .await?;

    let lobby_responses: Vec<LobbyResponse> =
        lobbies.into_iter().map(LobbyResponse::from).collect();

    Ok((StatusCode::OK, headers, Json(lobby_responses)))
}

/// Get all lobbies (Admin view - includes inactive)
pub async fn get_all_lobbies(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
) -> Result<(StatusCode, HeaderMap, Json<Vec<LobbyResponse>>)> {
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Only admins can see all lobbies
    if current_user.role != "Admin" {
        return Err(AppError::Forbidden(
            "Only admins can view all lobbies".to_string(),
        ));
    }

    let lobbies = lobby_pool::Entity::find().all(state.db.as_ref()).await?;

    let lobby_responses: Vec<LobbyResponse> =
        lobbies.into_iter().map(LobbyResponse::from).collect();

    Ok((StatusCode::OK, headers, Json(lobby_responses)))
}

/// Get a specific lobby
pub async fn get_lobby(
    State(state): State<AppState>,
    mut headers: HeaderMap,
    Path(lobby_id): Path<Uuid>,
) -> Result<(StatusCode, HeaderMap, Json<LobbyResponse>)> {
    super::add_rate_limit_headers(&mut headers);

    let lobby_model = lobby_pool::Entity::find_by_id(lobby_id)
        .one(state.db.as_ref())
        .await?
        .ok_or_else(|| AppError::Service("Lobby not found".to_string()))?;

    let lobby_response = LobbyResponse::from(lobby_model);
    Ok((StatusCode::OK, headers, Json(lobby_response)))
}

/// Update a lobby (Admin only)
pub async fn update_lobby(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Path(lobby_id): Path<Uuid>,
    Json(request): Json<UpdateLobbyRequest>,
) -> Result<(StatusCode, HeaderMap, Json<LobbyResponse>)> {
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Only admins can update lobbies
    if current_user.role != "Admin" {
        return Err(AppError::Forbidden(
            "Only admins can update lobbies".to_string(),
        ));
    }

    let lobby_model = lobby_pool::Entity::find_by_id(lobby_id)
        .one(state.db.as_ref())
        .await?
        .ok_or_else(|| AppError::Service("Lobby not found".to_string()))?;

    let mut lobby_update: lobby_pool::ActiveModel = lobby_model.into();

    if let Some(name) = request.name {
        // Check if new name is unique
        let existing_lobby = lobby_pool::Entity::find()
            .filter(lobby_pool::Column::Name.eq(&name))
            .filter(lobby_pool::Column::Id.ne(lobby_id))
            .one(state.db.as_ref())
            .await?;

        if existing_lobby.is_some() {
            return Err(AppError::Service("Lobby name already exists".to_string()));
        }
        lobby_update.name = Set(name);
    }
    if let Some(description) = request.description {
        lobby_update.description = Set(Some(description));
    }
    if let Some(preset) = request.preset {
        lobby_update.preset = Set(preset.to_string());
    }
    if let Some(active) = request.active {
        lobby_update.active = Set(active);
    }

    let updated_lobby = lobby_update.update(state.db.as_ref()).await?;
    let lobby_response = LobbyResponse::from(updated_lobby);

    Ok((StatusCode::OK, headers, Json(lobby_response)))
}

/// Delete a lobby (Admin only) - Soft delete by setting active = false
pub async fn delete_lobby(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Path(lobby_id): Path<Uuid>,
) -> Result<(StatusCode, HeaderMap, Json<serde_json::Value>)> {
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Only admins can delete lobbies
    if current_user.role != "Admin" {
        return Err(AppError::Forbidden(
            "Only admins can delete lobbies".to_string(),
        ));
    }

    let lobby_model = lobby_pool::Entity::find_by_id(lobby_id)
        .one(state.db.as_ref())
        .await?
        .ok_or_else(|| AppError::Service("Lobby not found".to_string()))?;

    // Soft delete by setting active = false
    let mut lobby_update: lobby_pool::ActiveModel = lobby_model.into();
    lobby_update.active = Set(false);
    lobby_update.update(state.db.as_ref()).await?;

    Ok((
        StatusCode::OK,
        headers,
        Json(serde_json::json!({"message": "Lobby deactivated successfully"})),
    ))
}

#[cfg(test)]
mod tests {
    use axum::http::{Method, StatusCode};
    use axum_test::TestServer;
    use serde_json::json;
    use std::sync::Arc;
    use uuid::Uuid;

    use crate::test_utils::test_utils::*;
    use entity::lobby_pool;

    #[tokio::test]
    async fn test_get_lobbies_success() {
        let lobby_id = Uuid::new_v4();
        let mock_lobby = lobby_pool::Model {
            id: lobby_id,
            name: "Test Lobby".to_string(),
            description: Some("A test lobby".to_string()),
            preset: "GeneralFourPlayer".to_string(),
            active: true,
        };

        let db = create_mock_db()
            .append_query_results([
                vec![mock_lobby.clone()], // Active lobbies
            ])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server.method(Method::GET, "/lobbies").await;

        assert_eq!(response.status_code(), StatusCode::OK);

        let body: serde_json::Value = response.json();
        assert!(body.is_array());
        let lobbies = body.as_array().unwrap();
        assert_eq!(lobbies.len(), 1);
        assert_eq!(lobbies[0]["id"], lobby_id.to_string());
        assert_eq!(lobbies[0]["name"], "Test Lobby");
    }

    #[tokio::test]
    async fn test_get_lobby_success() {
        let lobby_id = Uuid::new_v4();
        let mock_lobby = lobby_pool::Model {
            id: lobby_id,
            name: "Test Lobby".to_string(),
            description: Some("A test lobby".to_string()),
            preset: "GeneralFourPlayer".to_string(),
            active: true,
        };

        let db = create_mock_db()
            .append_query_results([
                vec![mock_lobby.clone()], // Lobby lookup
            ])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::GET, &format!("/api/v1/lobbies/{}", lobby_id))
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);

        let body: serde_json::Value = response.json();
        assert_eq!(body["id"], lobby_id.to_string());
        assert_eq!(body["name"], "Test Lobby");
        assert_eq!(body["preset"], "GeneralFourPlayer");
    }

    #[tokio::test]
    async fn test_get_lobby_not_found() {
        let lobby_id = Uuid::new_v4();

        let db = create_mock_db()
            .append_query_results([
                Vec::<lobby_pool::Model>::new(), // Lobby not found
            ])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::GET, &format!("/api/v1/lobbies/{}", lobby_id))
            .await;

        assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);

        let body: serde_json::Value = response.json();
        assert!(body["error"].as_str().unwrap().contains("Lobby not found"));
    }

    #[tokio::test]
    async fn test_create_lobby_unauthorized() {
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let request_body = json!({
            "name": "New Lobby",
            "description": "A new test lobby",
            "preset": "GeneralFourPlayer"
        });

        let response = server
            .method(Method::POST, "/api/v1/lobbies")
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
    async fn test_update_lobby_unauthorized() {
        let lobby_id = Uuid::new_v4();
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let request_body = json!({
            "name": "Updated Lobby"
        });

        let response = server
            .method(Method::PATCH, &format!("/api/v1/lobbies/{}", lobby_id))
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
    async fn test_delete_lobby_unauthorized() {
        let lobby_id = Uuid::new_v4();
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::DELETE, &format!("/api/v1/lobbies/{}", lobby_id))
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
