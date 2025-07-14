use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    auth::AuthSession,
    error::{AppError, Result},
    service::AppState,
};
use entity::lobby_pool;

#[derive(Debug, Serialize, ToSchema)]
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

#[derive(Debug, Deserialize, IntoParams)]
pub struct GetAllLobbiesQuery {
    #[param(example = false)]
    pub include_inactive: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateLobbyRequest {
    pub name: String,
    pub description: Option<String>,
    pub preset: LobbyPreset,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateLobbyRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub preset: Option<LobbyPreset>,
    pub active: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub enum LobbyPreset {
    GeneralFourPlayer,
    AllBotsFourPlayer,
}

impl std::fmt::Display for LobbyPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LobbyPreset::GeneralFourPlayer => write!(f, "GeneralFourPlayer"),
            LobbyPreset::AllBotsFourPlayer => write!(f, "AllBotsFourPlayer"),
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

#[utoipa::path(
    post,
    path = "/v1/lobbies",
    tag = "Lobbies",
    request_body = CreateLobbyRequest,
    responses(
        (status = 201, description = "Lobby created successfully", body = LobbyResponse),
        (status = 400, description = "Lobby name already exists"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Only admins can create lobbies"),
        (status = 500, description = "Internal server error")
    )
)]
/// Create a new lobby (Admin only)
pub async fn create_lobby(
    State(state): State<AppState>,
    auth_session: AuthSession,
    Json(request): Json<CreateLobbyRequest>,
) -> Result<(StatusCode, HeaderMap, Json<LobbyResponse>)> {
    let mut headers = HeaderMap::new();
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

#[utoipa::path(
    get,
    path = "/v1/my-lobbies",
    tag = "Lobbies",
    responses(
        (status = 200, description = "Active lobbies retrieved successfully", body = Vec<LobbyResponse>),
        (status = 500, description = "Internal server error")
    )
)]
/// Get all lobbies
pub async fn get_lobbies(
    State(state): State<AppState>,
) -> Result<(StatusCode, HeaderMap, Json<Vec<LobbyResponse>>)> {
    let mut headers = HeaderMap::new();
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

#[utoipa::path(
    get,
    path = "/v1/all-lobbies",
    tag = "Lobbies",
    params(
        GetAllLobbiesQuery
    ),
    responses(
        (status = 200, description = "Active lobbies retrieved successfully (admins can include inactive with include_inactive=true)", body = Vec<LobbyResponse>),
        (status = 401, description = "Authentication required"),
        (status = 500, description = "Internal server error")
    )
)]
/// Get all active lobbies (admins can include inactive with include_inactive=true)
pub async fn get_all_lobbies(
    State(state): State<AppState>,
    auth_session: AuthSession,
    Query(query): Query<GetAllLobbiesQuery>,
) -> Result<(StatusCode, HeaderMap, Json<Vec<LobbyResponse>>)> {
    let mut headers = HeaderMap::new();
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Determine whether to include inactive lobbies
    let include_inactive = query.include_inactive.unwrap_or(false) && current_user.role == "Admin";

    // Build query - filter to active lobbies unless admin requested inactive ones
    let mut query_builder = lobby_pool::Entity::find();
    if !include_inactive {
        query_builder = query_builder.filter(lobby_pool::Column::Active.eq(true));
    }

    let lobbies = query_builder.all(state.db.as_ref()).await?;

    let lobby_responses: Vec<LobbyResponse> =
        lobbies.into_iter().map(LobbyResponse::from).collect();

    Ok((StatusCode::OK, headers, Json(lobby_responses)))
}

#[utoipa::path(
    get,
    path = "/v1/lobbies/{id}",
    tag = "Lobbies",
    params(
        ("id" = Uuid, Path, description = "Lobby ID")
    ),
    responses(
        (status = 200, description = "Lobby found", body = LobbyResponse),
        (status = 404, description = "Lobby not found"),
        (status = 500, description = "Internal server error")
    )
)]
/// Get a specific lobby
pub async fn get_lobby(
    State(state): State<AppState>,
    Path(lobby_id): Path<Uuid>,
) -> Result<(StatusCode, HeaderMap, Json<LobbyResponse>)> {
    let mut headers = HeaderMap::new();
    super::add_rate_limit_headers(&mut headers);

    let lobby_model = lobby_pool::Entity::find_by_id(lobby_id)
        .one(state.db.as_ref())
        .await?
        .ok_or_else(|| AppError::Service("Lobby not found".to_string()))?;

    let lobby_response = LobbyResponse::from(lobby_model);
    Ok((StatusCode::OK, headers, Json(lobby_response)))
}

#[utoipa::path(
    patch,
    path = "/v1/lobbies/{id}",
    tag = "Lobbies",
    params(
        ("id" = Uuid, Path, description = "Lobby ID")
    ),
    request_body = UpdateLobbyRequest,
    responses(
        (status = 200, description = "Lobby updated successfully", body = LobbyResponse),
        (status = 400, description = "Lobby name already exists"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Only admins can update lobbies"),
        (status = 404, description = "Lobby not found"),
        (status = 500, description = "Internal server error")
    )
)]
/// Update a lobby (Admin only)
pub async fn update_lobby(
    State(state): State<AppState>,
    auth_session: AuthSession,
    Path(lobby_id): Path<Uuid>,
    Json(request): Json<UpdateLobbyRequest>,
) -> Result<(StatusCode, HeaderMap, Json<LobbyResponse>)> {
    let mut headers = HeaderMap::new();
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

#[utoipa::path(
    delete,
    path = "/v1/lobbies/{id}",
    tag = "Lobbies",
    params(
        ("id" = Uuid, Path, description = "Lobby ID")
    ),
    responses(
        (status = 200, description = "Lobby deactivated successfully"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Only admins can delete lobbies"),
        (status = 404, description = "Lobby not found"),
        (status = 500, description = "Internal server error")
    )
)]
/// Delete a lobby (Admin only) - Soft delete by setting active = false
pub async fn delete_lobby(
    State(state): State<AppState>,
    auth_session: AuthSession,
    Path(lobby_id): Path<Uuid>,
) -> Result<(StatusCode, HeaderMap, Json<serde_json::Value>)> {
    let mut headers = HeaderMap::new();
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
            .method(Method::GET, &format!("/v1/lobbies/{lobby_id}"))
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
            .method(Method::GET, &format!("/v1/lobbies/{lobby_id}"))
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
            .method(Method::POST, "/v1/lobbies")
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
            .method(Method::PATCH, &format!("/v1/lobbies/{lobby_id}"))
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
            .method(Method::DELETE, &format!("/v1/lobbies/{lobby_id}"))
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
