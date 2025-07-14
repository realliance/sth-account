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
    queue::messages::*,
    service::AppState,
};
use entity::{bot, lobby_pool, queue};

#[derive(Debug, Serialize, ToSchema)]
pub struct QueueResponse {
    pub id: Uuid,
    pub participant_type: String,
    pub participant_id: Uuid,
    pub lobby_id: Uuid,
    pub preferred_mmr_range: Option<String>,
    pub joined_at: chrono::DateTime<chrono::FixedOffset>,
    pub status: String,
}

impl From<queue::Model> for QueueResponse {
    fn from(queue_entry: queue::Model) -> Self {
        Self {
            id: queue_entry.id,
            participant_type: queue_entry.participant_type,
            participant_id: queue_entry.participant_id,
            lobby_id: queue_entry.lobby_id,
            preferred_mmr_range: queue_entry.preferred_mmr_range,
            joined_at: queue_entry.joined_at,
            status: queue_entry.status,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct JoinQueueRequest {
    pub lobby_id: Uuid,
    pub preferred_mmr_range: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct JoinQueueAsBotRequest {
    pub bot_id: Uuid,
    pub lobby_id: Uuid,
    pub preferred_mmr_range: Option<String>,
}

#[utoipa::path(
    post,
    path = "/v1/queue/join",
    tag = "Matchmaking",
    request_body = JoinQueueRequest,
    responses(
        (status = 201, description = "Joined queue successfully", body = QueueResponse),
        (status = 400, description = "Bad request (e.g., already in queue, lobby not active)"),
        (status = 401, description = "Authentication required"),
        (status = 404, description = "Lobby not found"),
        (status = 500, description = "Internal server error")
    )
)]
/// Join matchmaking queue as a human player
pub async fn join_queue(
    State(state): State<AppState>,
    auth_session: AuthSession,
    Json(request): Json<JoinQueueRequest>,
) -> Result<(StatusCode, HeaderMap, Json<QueueResponse>)> {
    let mut headers = HeaderMap::new();
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Verify lobby exists and is active
    let lobby = lobby_pool::Entity::find_by_id(request.lobby_id)
        .one(state.db.as_ref())
        .await?
        .ok_or_else(|| AppError::Service("Lobby not found".to_string()))?;

    if !lobby.active {
        return Err(AppError::Service("Lobby is not active".to_string()));
    }

    // Check if user is already in any queue
    let existing_queue = queue::Entity::find()
        .filter(queue::Column::ParticipantId.eq(current_user.id))
        .filter(queue::Column::Status.eq("Waiting"))
        .one(state.db.as_ref())
        .await?;

    if existing_queue.is_some() {
        return Err(AppError::Service("User is already in a queue".to_string()));
    }

    let queue_id = Uuid::new_v4();
    let joined_at = Utc::now();

    // Create queue entry in database
    let new_queue_entry = queue::ActiveModel {
        id: Set(queue_id),
        participant_type: Set("Human".to_string()),
        participant_id: Set(current_user.id),
        lobby_id: Set(request.lobby_id),
        preferred_mmr_range: Set(request.preferred_mmr_range.clone()),
        joined_at: Set(joined_at.into()),
        status: Set("Waiting".to_string()),
    };

    let queue_model = new_queue_entry.insert(state.db.as_ref()).await?;

    // Send message to RabbitMQ for matchmaking service
    let outgoing_message = OutgoingMessage::QueueJoin {
        queue_id,
        participant_type: ParticipantType::Human,
        participant_id: current_user.id,
        lobby_id: request.lobby_id,
        preferred_mmr_range: request.preferred_mmr_range,
        joined_at,
    };

    // Publish to queue, but don't fail the request if queue is down
    if let Err(e) = state.queue.publish_message(&outgoing_message).await {
        tracing::warn!("Failed to publish QueueJoin message: {}", e);
    }

    let queue_response = QueueResponse::from(queue_model);
    Ok((StatusCode::CREATED, headers, Json(queue_response)))
}

#[utoipa::path(
    post,
    path = "/v1/queue/join-bot",
    tag = "Matchmaking",
    request_body = JoinQueueAsBotRequest,
    responses(
        (status = 201, description = "Bot joined queue successfully", body = QueueResponse),
        (status = 400, description = "Bad request (e.g., bot already in queue, lobby not active, bot not live)"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Bot or lobby not found"),
        (status = 500, description = "Internal server error")
    )
)]
/// Join matchmaking queue as a bot
pub async fn join_queue_as_bot(
    State(state): State<AppState>,
    auth_session: AuthSession,
    Json(request): Json<JoinQueueAsBotRequest>,
) -> Result<(StatusCode, HeaderMap, Json<QueueResponse>)> {
    let mut headers = HeaderMap::new();
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Verify bot exists and is owned by current user
    let bot = bot::Entity::find_by_id(request.bot_id)
        .one(state.db.as_ref())
        .await?
        .ok_or_else(|| AppError::Service("Bot not found".to_string()))?;

    if bot.owner_id != current_user.id && current_user.role != "Admin" {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    if !bot.live {
        return Err(AppError::Service("Bot is not currently live".to_string()));
    }

    // Verify lobby exists and is active
    let lobby = lobby_pool::Entity::find_by_id(request.lobby_id)
        .one(state.db.as_ref())
        .await?
        .ok_or_else(|| AppError::Service("Lobby not found".to_string()))?;

    if !lobby.active {
        return Err(AppError::Service("Lobby is not active".to_string()));
    }

    // Check if bot is already in any queue
    let existing_queue = queue::Entity::find()
        .filter(queue::Column::ParticipantId.eq(request.bot_id))
        .filter(queue::Column::Status.eq("Waiting"))
        .one(state.db.as_ref())
        .await?;

    if existing_queue.is_some() {
        return Err(AppError::Service("Bot is already in a queue".to_string()));
    }

    let queue_id = Uuid::new_v4();
    let joined_at = Utc::now();

    // Create queue entry in database
    let new_queue_entry = queue::ActiveModel {
        id: Set(queue_id),
        participant_type: Set("Bot".to_string()),
        participant_id: Set(request.bot_id),
        lobby_id: Set(request.lobby_id),
        preferred_mmr_range: Set(request.preferred_mmr_range.clone()),
        joined_at: Set(joined_at.into()),
        status: Set("Waiting".to_string()),
    };

    let queue_model = new_queue_entry.insert(state.db.as_ref()).await?;

    // Send message to RabbitMQ for matchmaking service
    let outgoing_message = OutgoingMessage::QueueJoin {
        queue_id,
        participant_type: ParticipantType::Bot,
        participant_id: request.bot_id,
        lobby_id: request.lobby_id,
        preferred_mmr_range: request.preferred_mmr_range,
        joined_at,
    };

    // Publish to queue, but don't fail the request if queue is down
    if let Err(e) = state.queue.publish_message(&outgoing_message).await {
        tracing::warn!("Failed to publish QueueJoin message: {}", e);
    }

    let queue_response = QueueResponse::from(queue_model);
    Ok((StatusCode::CREATED, headers, Json(queue_response)))
}

#[utoipa::path(
    delete,
    path = "/v1/queue/{id}",
    tag = "Matchmaking",
    params(
        ("id" = Uuid, Path, description = "Queue entry ID")
    ),
    responses(
        (status = 200, description = "Left queue successfully"),
        (status = 400, description = "Cannot leave queue (e.g., not in waiting status)"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Queue entry not found"),
        (status = 500, description = "Internal server error")
    )
)]
/// Leave matchmaking queue
pub async fn leave_queue(
    State(state): State<AppState>,
    auth_session: AuthSession,
    Path(queue_id): Path<Uuid>,
) -> Result<(StatusCode, HeaderMap, Json<serde_json::Value>)> {
    let mut headers = HeaderMap::new();
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Find the queue entry
    let queue_entry = queue::Entity::find_by_id(queue_id)
        .one(state.db.as_ref())
        .await?
        .ok_or_else(|| AppError::Service("Queue entry not found".to_string()))?;

    // Verify ownership (user can leave their own queue, or bot owner can leave bot queue, or admin)
    let can_leave = match queue_entry.participant_type.as_str() {
        "Human" => queue_entry.participant_id == current_user.id || current_user.role == "Admin",
        "Bot" => {
            // Check if current user owns the bot
            let bot = bot::Entity::find_by_id(queue_entry.participant_id)
                .one(state.db.as_ref())
                .await?;
            if let Some(bot) = bot {
                bot.owner_id == current_user.id || current_user.role == "Admin"
            } else {
                current_user.role == "Admin"
            }
        }
        _ => current_user.role == "Admin",
    };

    if !can_leave {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    if queue_entry.status != "Waiting" {
        return Err(AppError::Service(
            "Cannot leave queue - not in waiting status".to_string(),
        ));
    }

    // Update queue entry status to Cancelled
    let mut queue_update: queue::ActiveModel = queue_entry.clone().into();
    queue_update.status = Set("Cancelled".to_string());
    queue_update.update(state.db.as_ref()).await?;

    // Send message to RabbitMQ to notify matchmaking service
    let outgoing_message = OutgoingMessage::QueueLeave {
        queue_id,
        participant_id: queue_entry.participant_id,
        left_at: Utc::now(),
    };

    // Publish to queue, but don't fail the request if queue is down
    if let Err(e) = state.queue.publish_message(&outgoing_message).await {
        tracing::warn!("Failed to publish QueueLeave message: {}", e);
    }

    Ok((
        StatusCode::OK,
        headers,
        Json(serde_json::json!({"message": "Left queue successfully"})),
    ))
}

#[utoipa::path(
    get,
    path = "/v1/queue/status",
    tag = "Matchmaking",
    responses(
        (status = 200, description = "Current queue status for user", body = Vec<QueueResponse>),
        (status = 401, description = "Authentication required"),
        (status = 500, description = "Internal server error")
    )
)]
/// Get current queue status for a user
pub async fn get_queue_status(
    State(state): State<AppState>,
    auth_session: AuthSession,
) -> Result<(StatusCode, HeaderMap, Json<Vec<QueueResponse>>)> {
    let mut headers = HeaderMap::new();
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Get user's current queue entries (both as human and as bot owner)
    let mut user_queues = queue::Entity::find()
        .filter(queue::Column::ParticipantId.eq(current_user.id))
        .filter(queue::Column::ParticipantType.eq("Human"))
        .filter(queue::Column::Status.eq("Waiting"))
        .all(state.db.as_ref())
        .await?;

    // Get queues for bots owned by this user
    let user_bots = bot::Entity::find()
        .filter(bot::Column::OwnerId.eq(current_user.id))
        .all(state.db.as_ref())
        .await?;

    for bot in user_bots {
        let bot_queues = queue::Entity::find()
            .filter(queue::Column::ParticipantId.eq(bot.id))
            .filter(queue::Column::ParticipantType.eq("Bot"))
            .filter(queue::Column::Status.eq("Waiting"))
            .all(state.db.as_ref())
            .await?;
        user_queues.extend(bot_queues);
    }

    let queue_responses: Vec<QueueResponse> =
        user_queues.into_iter().map(QueueResponse::from).collect();

    Ok((StatusCode::OK, headers, Json(queue_responses)))
}

#[utoipa::path(
    get,
    path = "/v1/queue/lobby/{id}/stats",
    tag = "Matchmaking",
    params(
        ("id" = Uuid, Path, description = "Lobby ID")
    ),
    responses(
        (status = 200, description = "Lobby queue statistics retrieved successfully", body = serde_json::Value),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Lobby not found"),
        (status = 500, description = "Internal server error")
    )
)]
/// Get queue statistics for a lobby (Admin only)
pub async fn get_lobby_queue_stats(
    State(state): State<AppState>,
    auth_session: AuthSession,
    Path(lobby_id): Path<Uuid>,
) -> Result<(StatusCode, HeaderMap, Json<serde_json::Value>)> {
    let mut headers = HeaderMap::new();
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Only admins can view queue statistics
    if current_user.role != "Admin" {
        return Err(AppError::Forbidden(
            "Only admins can view queue statistics".to_string(),
        ));
    }

    // Verify lobby exists
    let _lobby = lobby_pool::Entity::find_by_id(lobby_id)
        .one(state.db.as_ref())
        .await?
        .ok_or_else(|| AppError::Service("Lobby not found".to_string()))?;

    // Get queue statistics
    let waiting_entries = queue::Entity::find()
        .filter(queue::Column::LobbyId.eq(lobby_id))
        .filter(queue::Column::Status.eq("Waiting"))
        .all(state.db.as_ref())
        .await?;

    let human_count = waiting_entries
        .iter()
        .filter(|e| e.participant_type == "Human")
        .count();
    let bot_count = waiting_entries
        .iter()
        .filter(|e| e.participant_type == "Bot")
        .count();
    let total_waiting = waiting_entries.len();

    Ok((
        StatusCode::OK,
        headers,
        Json(serde_json::json!({
            "lobby_id": lobby_id,
            "total_waiting": total_waiting,
            "human_count": human_count,
            "bot_count": bot_count,
            "queue_entries": waiting_entries.into_iter().map(QueueResponse::from).collect::<Vec<_>>()
        })),
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

    #[tokio::test]
    async fn test_join_queue_unauthorized() {
        let lobby_id = Uuid::new_v4();
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let request_body = json!({
            "lobby_id": lobby_id,
            "preferred_mmr_range": "1000-1200"
        });

        let response = server
            .method(Method::POST, "/v1/queue/join")
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
    async fn test_join_queue_as_bot_unauthorized() {
        let bot_id = Uuid::new_v4();
        let lobby_id = Uuid::new_v4();
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let request_body = json!({
            "bot_id": bot_id,
            "lobby_id": lobby_id
        });

        let response = server
            .method(Method::POST, "/v1/queue/join-bot")
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
    async fn test_leave_queue_unauthorized() {
        let queue_id = Uuid::new_v4();
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::DELETE, &format!("/v1/queue/{queue_id}"))
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
    async fn test_get_queue_status_unauthorized() {
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server.method(Method::GET, "/v1/queue/status").await;

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
    async fn test_get_lobby_queue_stats_unauthorized() {
        let lobby_id = Uuid::new_v4();
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::GET, &format!("/v1/queue/lobby/{lobby_id}/stats"))
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
