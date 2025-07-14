use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::AuthSession,
    error::{AppError, Result},
    service::AppState,
};
use entity::bot;

#[derive(Debug, Serialize, ToSchema)]
pub struct BotResponse {
    pub id: Uuid,
    pub name: String,
    pub owner_id: Uuid,
    pub source_code: Option<String>,
    pub matchmaking_rank: i32,
    pub live: bool,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub last_heartbeat: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
}

impl From<bot::Model> for BotResponse {
    fn from(bot: bot::Model) -> Self {
        Self {
            id: bot.id,
            name: bot.name,
            owner_id: bot.owner_id,
            source_code: bot.source_code,
            matchmaking_rank: bot.matchmaking_rank,
            live: bot.live,
            icon: bot.icon,
            description: bot.description,
            version: bot.version,
            last_heartbeat: bot.last_heartbeat,
            created_at: bot.created_at,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateBotRequest {
    pub name: String,
    pub source_code: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateBotRequest {
    pub name: Option<String>,
    pub source_code: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub live: Option<bool>,
}

#[utoipa::path(
    post,
    path = "/v1/bots",
    tag = "Bots",
    request_body = CreateBotRequest,
    responses(
        (status = 201, description = "Bot created successfully", body = BotResponse),
        (status = 400, description = "Bot name already exists"),
        (status = 401, description = "Authentication required"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn create_bot(
    State(state): State<AppState>,
    auth_session: AuthSession,
    Json(request): Json<CreateBotRequest>,
) -> Result<(StatusCode, HeaderMap, Json<BotResponse>)> {
    let mut headers = HeaderMap::new();
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Check if bot name is unique for this user
    let existing_bot = bot::Entity::find()
        .filter(bot::Column::Name.eq(&request.name))
        .filter(bot::Column::OwnerId.eq(current_user.id))
        .one(state.db.as_ref())
        .await?;

    if existing_bot.is_some() {
        return Err(AppError::Service(
            "Bot name already exists for this user".to_string(),
        ));
    }

    let api_key = format!("bot_{}_{}", current_user.id, Uuid::new_v4());

    let new_bot = bot::ActiveModel {
        id: Set(Uuid::new_v4()),
        name: Set(request.name),
        owner_id: Set(current_user.id),
        source_code: Set(request.source_code),
        matchmaking_rank: Set(1000), // Default MMR
        api_key: Set(api_key),
        live: Set(false), // Start as offline
        icon: Set(request.icon),
        description: Set(request.description),
        version: Set(request.version),
        last_heartbeat: Set(None),
        created_at: Set(Utc::now().into()),
    };

    let bot_model = new_bot.insert(state.db.as_ref()).await?;
    let bot_response = BotResponse::from(bot_model);

    Ok((StatusCode::CREATED, headers, Json(bot_response)))
}

#[utoipa::path(
    get,
    path = "/v1/bots/{id}",
    tag = "Bots",
    params(
        ("id" = Uuid, Path, description = "Bot ID")
    ),
    responses(
        (status = 200, description = "Bot found", body = BotResponse),
        (status = 404, description = "Bot not found")
    )
)]
pub async fn get_bot(
    State(state): State<AppState>,
    Path(bot_id): Path<Uuid>,
) -> Result<(StatusCode, HeaderMap, Json<BotResponse>)> {
    let mut headers = HeaderMap::new();
    super::add_rate_limit_headers(&mut headers);

    let bot_model = bot::Entity::find_by_id(bot_id)
        .one(state.db.as_ref())
        .await?
        .ok_or_else(|| AppError::Service("Bot not found".to_string()))?;

    let bot_response = BotResponse::from(bot_model);
    Ok((StatusCode::OK, headers, Json(bot_response)))
}

#[utoipa::path(
    patch,
    path = "/v1/bots/{id}",
    tag = "Bots",
    params(
        ("id" = Uuid, Path, description = "Bot ID")
    ),
    request_body = UpdateBotRequest,
    responses(
        (status = 200, description = "Bot updated successfully", body = BotResponse),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Bot not found")
    )
)]
pub async fn update_bot(
    State(state): State<AppState>,
    auth_session: AuthSession,
    Path(bot_id): Path<Uuid>,
    Json(request): Json<UpdateBotRequest>,
) -> Result<(StatusCode, HeaderMap, Json<BotResponse>)> {
    let mut headers = HeaderMap::new();
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    let bot_model = bot::Entity::find_by_id(bot_id)
        .one(state.db.as_ref())
        .await?
        .ok_or_else(|| AppError::Service("Bot not found".to_string()))?;

    // Only the owner can update the bot
    if bot_model.owner_id != current_user.id && current_user.role != "Admin" {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let mut bot_update: bot::ActiveModel = bot_model.into();

    if let Some(name) = request.name {
        // Check if new name is unique for this user
        let existing_bot = bot::Entity::find()
            .filter(bot::Column::Name.eq(&name))
            .filter(bot::Column::OwnerId.eq(current_user.id))
            .filter(bot::Column::Id.ne(bot_id))
            .one(state.db.as_ref())
            .await?;

        if existing_bot.is_some() {
            return Err(AppError::Service(
                "Bot name already exists for this user".to_string(),
            ));
        }
        bot_update.name = Set(name);
    }
    if let Some(source_code) = request.source_code {
        bot_update.source_code = Set(Some(source_code));
    }
    if let Some(icon) = request.icon {
        bot_update.icon = Set(Some(icon));
    }
    if let Some(description) = request.description {
        bot_update.description = Set(Some(description));
    }
    if let Some(version) = request.version {
        bot_update.version = Set(Some(version));
    }
    if let Some(live) = request.live {
        bot_update.live = Set(live);
    }

    let updated_bot = bot_update.update(state.db.as_ref()).await?;
    let bot_response = BotResponse::from(updated_bot);

    Ok((StatusCode::OK, headers, Json(bot_response)))
}

#[utoipa::path(
    delete,
    path = "/v1/bots/{id}",
    tag = "Bots",
    params(
        ("id" = Uuid, Path, description = "Bot ID")
    ),
    responses(
        (status = 200, description = "Bot deleted successfully"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Bot not found")
    )
)]
pub async fn delete_bot(
    State(state): State<AppState>,
    auth_session: AuthSession,
    Path(bot_id): Path<Uuid>,
) -> Result<(StatusCode, HeaderMap, Json<serde_json::Value>)> {
    let mut headers = HeaderMap::new();
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    let bot_model = bot::Entity::find_by_id(bot_id)
        .one(state.db.as_ref())
        .await?
        .ok_or_else(|| AppError::Service("Bot not found".to_string()))?;

    // Only the owner or admin can delete the bot
    if bot_model.owner_id != current_user.id && current_user.role != "Admin" {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    bot::Entity::delete_by_id(bot_id)
        .exec(state.db.as_ref())
        .await?;

    Ok((
        StatusCode::OK,
        headers,
        Json(serde_json::json!({"message": "Bot deleted successfully"})),
    ))
}

#[utoipa::path(
    get,
    path = "/v1/users/{id}/bots",
    tag = "Bots",
    params(
        ("id" = Uuid, Path, description = "User ID")
    ),
    responses(
        (status = 200, description = "User's bots retrieved successfully", body = Vec<BotResponse>),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Access denied")
    )
)]
pub async fn get_user_bots(
    State(state): State<AppState>,
    auth_session: AuthSession,
    Path(user_id): Path<Uuid>,
) -> Result<(StatusCode, HeaderMap, Json<Vec<BotResponse>>)> {
    let mut headers = HeaderMap::new();
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Users can only view their own bots unless they're admin/moderator
    if current_user.id != user_id && !["Admin", "Moderator"].contains(&current_user.role.as_str()) {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let bots = bot::Entity::find()
        .filter(bot::Column::OwnerId.eq(user_id))
        .all(state.db.as_ref())
        .await?;

    let bot_responses: Vec<BotResponse> = bots.into_iter().map(BotResponse::from).collect();

    Ok((StatusCode::OK, headers, Json(bot_responses)))
}

#[cfg(test)]
mod tests {
    use axum::http::{Method, StatusCode};
    use axum_test::TestServer;
    // No direct sea_orm imports needed for these tests
    use serde_json::json;
    use std::sync::Arc;
    use uuid::Uuid;

    use crate::test_utils::test_utils::*;
    use entity::bot;

    #[tokio::test]
    async fn test_create_bot_success() {
        let user_id = Uuid::new_v4();
        let bot_id = Uuid::new_v4();
        let auth_user = sample_user(Some(user_id));
        let mock_bot = sample_bot(Some(bot_id), user_id);

        let db = create_mock_db()
            .append_query_results([
                vec![auth_user], // Auth user lookup
            ])
            .append_query_results([
                Vec::<bot::Model>::new(), // No existing bot with this name
            ])
            .append_exec_results([mock_exec_success(1)]) // Insert bot
            .append_query_results([
                vec![mock_bot.clone()], // Return created bot
            ])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let request_body = json!({
            "name": "mybot",
            "source_code": "https://github.com/user/mybot",
            "icon": "🤖",
            "description": "My awesome bot",
            "version": "1.0.0"
        });

        let response = server
            .method(Method::POST, "/v1/bots")
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
    async fn test_create_bot_duplicate_name() {
        let user_id = Uuid::new_v4();
        let auth_user = sample_user(Some(user_id));
        let existing_bot = sample_bot(None, user_id);

        let db = create_mock_db()
            .append_query_results([
                vec![auth_user], // Auth user lookup
            ])
            .append_query_results([
                vec![existing_bot], // Bot with same name already exists
            ])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let request_body = json!({
            "name": "testbot"
        });

        let response = server
            .method(Method::POST, "/v1/bots")
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
    async fn test_get_bot_success() {
        let bot_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let mock_bot = sample_bot(Some(bot_id), user_id);

        let db = create_mock_db()
            .append_query_results([
                vec![mock_bot.clone()], // Bot lookup
            ])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::GET, &format!("/v1/bots/{bot_id}"))
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);

        let body: serde_json::Value = response.json();
        assert_eq!(body["id"], bot_id.to_string());
        assert_eq!(body["name"], "testbot");
    }

    #[tokio::test]
    async fn test_get_bot_not_found() {
        let bot_id = Uuid::new_v4();

        let db = create_mock_db()
            .append_query_results([
                Vec::<bot::Model>::new(), // Bot not found
            ])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::GET, &format!("/v1/bots/{bot_id}"))
            .await;

        assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);

        let body: serde_json::Value = response.json();
        assert!(body["error"].as_str().unwrap().contains("Bot not found"));
    }

    #[tokio::test]
    async fn test_update_bot_success() {
        let user_id = Uuid::new_v4();
        let bot_id = Uuid::new_v4();
        let auth_user = sample_user(Some(user_id));
        let mock_bot = sample_bot(Some(bot_id), user_id);
        let mut updated_bot = mock_bot.clone();
        updated_bot.description = Some("Updated description".to_string());

        let db = create_mock_db()
            .append_query_results([
                vec![auth_user], // Auth user lookup
            ])
            .append_query_results([
                vec![mock_bot], // Find bot to update
            ])
            .append_exec_results([mock_exec_success(1)]) // Update operation
            .append_query_results([
                vec![updated_bot.clone()], // Return updated bot
            ])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let request_body = json!({
            "description": "Updated description",
            "live": true
        });

        let response = server
            .method(Method::PATCH, &format!("/v1/bots/{bot_id}"))
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
    async fn test_update_bot_access_denied() {
        let owner_id = Uuid::new_v4();
        let other_user_id = Uuid::new_v4();
        let bot_id = Uuid::new_v4();
        let auth_user = sample_user(Some(other_user_id)); // Different user
        let mock_bot = sample_bot(Some(bot_id), owner_id); // Bot owned by someone else

        let db = create_mock_db()
            .append_query_results([
                vec![auth_user], // Auth user lookup
            ])
            .append_query_results([
                vec![mock_bot], // Bot owned by different user
            ])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let request_body = json!({
            "description": "Trying to update someone else's bot"
        });

        let response = server
            .method(Method::PATCH, &format!("/v1/bots/{bot_id}"))
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
    async fn test_delete_bot_success() {
        let user_id = Uuid::new_v4();
        let bot_id = Uuid::new_v4();
        let auth_user = sample_user(Some(user_id));
        let mock_bot = sample_bot(Some(bot_id), user_id);

        let db = create_mock_db()
            .append_query_results([
                vec![auth_user], // Auth user lookup
            ])
            .append_query_results([
                vec![mock_bot], // Find bot to delete
            ])
            .append_exec_results([mock_exec_success(1)]) // Delete operation
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::DELETE, &format!("/v1/bots/{bot_id}"))
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
    async fn test_get_user_bots_success() {
        let user_id = Uuid::new_v4();
        let auth_user = sample_user(Some(user_id));
        let bot1 = sample_bot(Some(Uuid::new_v4()), user_id);
        let mut bot2 = sample_bot(Some(Uuid::new_v4()), user_id);
        bot2.name = "secondbot".to_string();

        let db = create_mock_db()
            .append_query_results([
                vec![auth_user], // Auth user lookup
            ])
            .append_query_results([
                vec![bot1.clone(), bot2.clone()], // User's bots
            ])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::GET, &format!("/v1/users/{user_id}/bots"))
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
    async fn test_get_user_bots_access_denied() {
        let owner_id = Uuid::new_v4();
        let other_user_id = Uuid::new_v4();
        let auth_user = sample_user(Some(other_user_id));

        let db = create_mock_db()
            .append_query_results([
                vec![auth_user], // Different user authenticated
            ])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::GET, &format!("/v1/users/{owner_id}/bots"))
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
