use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use chrono::Utc;
use rand::{Rng, distributions::Alphanumeric};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::AuthSession,
    error::{AppError, Result},
    service::AppState,
};

use entity::{private_room, room_invitation, room_participants, user};

#[derive(Debug, Serialize)]
pub struct PrivateRoomResponse {
    pub id: Uuid,
    pub host_id: Uuid,
    pub room_name: String,
    pub room_code: String,
    pub max_players: i32,
    pub allow_bots: bool,
    pub invite_only: bool,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub participant_count: usize,
}

impl From<(private_room::Model, usize)> for PrivateRoomResponse {
    fn from((room, participant_count): (private_room::Model, usize)) -> Self {
        Self {
            id: room.id,
            host_id: room.host_id,
            room_name: room.room_name,
            room_code: room.room_code,
            max_players: room.max_players,
            allow_bots: room.allow_bots,
            invite_only: room.invite_only,
            status: room.status,
            created_at: room.created_at,
            participant_count,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateRoomRequest {
    pub room_name: String,
    pub password: Option<String>,
    pub max_players: Option<i32>, // 3 or 4
    pub allow_bots: Option<bool>,
    pub invite_only: Option<bool>,
    pub room_settings: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct JoinRoomRequest {
    pub room_code: String,
    pub password: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct InviteToRoomRequest {
    pub invitee_id: Uuid,
}

/// Create a private room
pub async fn create_room(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Json(request): Json<CreateRoomRequest>,
) -> Result<(StatusCode, HeaderMap, Json<PrivateRoomResponse>)> {
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Validate max_players
    let max_players = request.max_players.unwrap_or(4);
    if max_players < 3 || max_players > 4 {
        return Err(AppError::Validation(
            "max_players must be 3 or 4".to_string(),
        ));
    }

    // Validate room name
    if request.room_name.trim().is_empty() {
        return Err(AppError::Validation(
            "room_name cannot be empty".to_string(),
        ));
    }

    // Generate unique room code
    let room_code = generate_room_code();

    // Check if room code already exists (very unlikely but possible)
    if private_room::Entity::find()
        .filter(private_room::Column::RoomCode.eq(&room_code))
        .one(state.db.as_ref())
        .await
        .map_err(AppError::Database)?
        .is_some()
    {
        return Err(AppError::Service(
            "Room code generation failed, please try again".to_string(),
        ));
    }

    // Create the room
    let new_room = private_room::ActiveModel {
        id: Set(Uuid::new_v4()),
        host_id: Set(current_user.id),
        room_name: Set(request.room_name.trim().to_string()),
        room_code: Set(room_code),
        password: Set(request.password),
        max_players: Set(max_players),
        allow_bots: Set(request.allow_bots.unwrap_or(false)),
        invite_only: Set(request.invite_only.unwrap_or(false)),
        status: Set("Waiting".to_string()),
        room_settings: Set(request.room_settings.map(|v| v.into())),
        created_at: Set(Utc::now().into()),
        started_at: Set(None),
        completed_at: Set(None),
    };

    let room = new_room
        .insert(state.db.as_ref())
        .await
        .map_err(AppError::Database)?;

    // Add the host as a participant
    let host_participant = room_participants::ActiveModel {
        id: Set(Uuid::new_v4()),
        room_id: Set(room.id),
        participant_type: Set("Human".to_string()),
        participant_id: Set(current_user.id),
        status: Set("Joined".to_string()),
        joined_at: Set(Utc::now().into()),
        left_at: Set(None),
    };

    host_participant
        .insert(state.db.as_ref())
        .await
        .map_err(AppError::Database)?;

    let response = PrivateRoomResponse::from((room, 1)); // Host is first participant

    Ok((StatusCode::CREATED, headers, Json(response)))
}

pub fn generate_room_code() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(6)
        .map(char::from)
        .collect::<String>()
        .to_uppercase()
}

/// Join a private room by room code
pub async fn join_room(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Json(request): Json<JoinRoomRequest>,
) -> Result<(StatusCode, HeaderMap, Json<PrivateRoomResponse>)> {
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Find the room by code
    let room = private_room::Entity::find()
        .filter(private_room::Column::RoomCode.eq(&request.room_code))
        .one(state.db.as_ref())
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound("Room not found".to_string()))?;

    // Check if room is still accepting players
    if room.status != "Waiting" {
        return Err(AppError::Validation(
            "Room is no longer accepting players".to_string(),
        ));
    }

    // Check password if required
    if let Some(room_password) = &room.password {
        if request.password.as_ref() != Some(room_password) {
            return Err(AppError::Auth("Incorrect room password".to_string()));
        }
    }

    // Check if user is already in the room
    if room_participants::Entity::find()
        .filter(room_participants::Column::RoomId.eq(room.id))
        .filter(room_participants::Column::ParticipantId.eq(current_user.id))
        .filter(room_participants::Column::Status.eq("Joined"))
        .one(state.db.as_ref())
        .await
        .map_err(AppError::Database)?
        .is_some()
    {
        return Err(AppError::Validation(
            "You are already in this room".to_string(),
        ));
    }

    // Check if room is full
    let current_participants = room_participants::Entity::find()
        .filter(room_participants::Column::RoomId.eq(room.id))
        .filter(room_participants::Column::Status.eq("Joined"))
        .count(state.db.as_ref())
        .await
        .map_err(AppError::Database)?;

    if current_participants >= room.max_players as u64 {
        return Err(AppError::Validation("Room is full".to_string()));
    }

    // Add participant to room
    let new_participant = room_participants::ActiveModel {
        id: Set(Uuid::new_v4()),
        room_id: Set(room.id),
        participant_type: Set("Human".to_string()),
        participant_id: Set(current_user.id),
        status: Set("Joined".to_string()),
        joined_at: Set(Utc::now().into()),
        left_at: Set(None),
    };

    new_participant
        .insert(state.db.as_ref())
        .await
        .map_err(AppError::Database)?;

    let response = PrivateRoomResponse::from((room, current_participants as usize + 1));

    Ok((StatusCode::OK, headers, Json(response)))
}

/// Leave a private room
pub async fn leave_room(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Path(room_id): Path<Uuid>,
) -> Result<(StatusCode, HeaderMap, Json<serde_json::Value>)> {
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Find the participant record
    let participant = room_participants::Entity::find()
        .filter(room_participants::Column::RoomId.eq(room_id))
        .filter(room_participants::Column::ParticipantId.eq(current_user.id))
        .filter(room_participants::Column::Status.eq("Joined"))
        .one(state.db.as_ref())
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound("You are not in this room".to_string()))?;

    // Update participant status to "Left"
    let mut participant_update: room_participants::ActiveModel = participant.into();
    participant_update.status = Set("Left".to_string());
    participant_update.left_at = Set(Some(Utc::now().into()));

    participant_update
        .update(state.db.as_ref())
        .await
        .map_err(AppError::Database)?;

    // Check if the room is now empty and update status
    let remaining_participants = room_participants::Entity::find()
        .filter(room_participants::Column::RoomId.eq(room_id))
        .filter(room_participants::Column::Status.eq("Joined"))
        .count(state.db.as_ref())
        .await
        .map_err(AppError::Database)?;

    if remaining_participants == 0 {
        // Update room status to "Empty"
        if let Some(room) = private_room::Entity::find_by_id(room_id)
            .one(state.db.as_ref())
            .await
            .map_err(AppError::Database)?
        {
            let mut room_update: private_room::ActiveModel = room.into();
            room_update.status = Set("Empty".to_string());
            room_update
                .update(state.db.as_ref())
                .await
                .map_err(AppError::Database)?;
        }
    }

    Ok((
        StatusCode::OK,
        headers,
        Json(serde_json::json!({"message": "Successfully left room"})),
    ))
}

/// Get user's private rooms
pub async fn get_user_rooms(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
) -> Result<(StatusCode, HeaderMap, Json<Vec<PrivateRoomResponse>>)> {
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Find rooms where user is a participant
    let participant_rooms = room_participants::Entity::find()
        .filter(room_participants::Column::ParticipantId.eq(current_user.id))
        .filter(room_participants::Column::Status.eq("Joined"))
        .find_also_related(private_room::Entity)
        .all(state.db.as_ref())
        .await
        .map_err(AppError::Database)?;

    let mut rooms = Vec::new();
    for (_, room_opt) in participant_rooms {
        if let Some(room) = room_opt {
            // Get participant count for this room
            let participant_count = room_participants::Entity::find()
                .filter(room_participants::Column::RoomId.eq(room.id))
                .filter(room_participants::Column::Status.eq("Joined"))
                .count(state.db.as_ref())
                .await
                .map_err(AppError::Database)?;

            rooms.push(PrivateRoomResponse::from((
                room,
                participant_count as usize,
            )));
        }
    }

    Ok((StatusCode::OK, headers, Json(rooms)))
}

/// Invite a user to a private room
pub async fn invite_to_room(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Path(room_id): Path<Uuid>,
    Json(request): Json<InviteToRoomRequest>,
) -> Result<(StatusCode, HeaderMap, Json<serde_json::Value>)> {
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Find the room and verify user is host or participant
    let room = private_room::Entity::find_by_id(room_id)
        .one(state.db.as_ref())
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound("Room not found".to_string()))?;

    // Check if current user is the host or a participant
    let is_host = room.host_id == current_user.id;
    let is_participant = room_participants::Entity::find()
        .filter(room_participants::Column::RoomId.eq(room_id))
        .filter(room_participants::Column::ParticipantId.eq(current_user.id))
        .filter(room_participants::Column::Status.eq("Joined"))
        .one(state.db.as_ref())
        .await
        .map_err(AppError::Database)?
        .is_some();

    if !is_host && !is_participant {
        return Err(AppError::Auth(
            "Only room host or participants can send invitations".to_string(),
        ));
    }

    // Check if invitee exists
    if !user::Entity::find_by_id(request.invitee_id)
        .one(state.db.as_ref())
        .await
        .map_err(AppError::Database)?
        .is_some()
    {
        return Err(AppError::NotFound("User to invite not found".to_string()));
    }

    // Check if invitee is already in the room
    if room_participants::Entity::find()
        .filter(room_participants::Column::RoomId.eq(room_id))
        .filter(room_participants::Column::ParticipantId.eq(request.invitee_id))
        .filter(room_participants::Column::Status.eq("Joined"))
        .one(state.db.as_ref())
        .await
        .map_err(AppError::Database)?
        .is_some()
    {
        return Err(AppError::Validation(
            "User is already in the room".to_string(),
        ));
    }

    // Check for existing pending invitation
    if room_invitation::Entity::find()
        .filter(room_invitation::Column::RoomId.eq(room_id))
        .filter(room_invitation::Column::InviteeId.eq(request.invitee_id))
        .filter(room_invitation::Column::Status.eq("Pending"))
        .one(state.db.as_ref())
        .await
        .map_err(AppError::Database)?
        .is_some()
    {
        return Err(AppError::Validation(
            "User already has a pending invitation to this room".to_string(),
        ));
    }

    // Create invitation
    let invitation = room_invitation::ActiveModel {
        id: Set(Uuid::new_v4()),
        room_id: Set(room_id),
        inviter_id: Set(current_user.id),
        invitee_id: Set(request.invitee_id),
        status: Set("Pending".to_string()),
        created_at: Set(Utc::now().into()),
        responded_at: Set(None),
        expires_at: Set(Some((Utc::now() + chrono::Duration::hours(24)).into())), // 24 hour expiry
    };

    invitation
        .insert(state.db.as_ref())
        .await
        .map_err(AppError::Database)?;

    Ok((
        StatusCode::CREATED,
        headers,
        Json(serde_json::json!({"message": "Invitation sent successfully"})),
    ))
}

/// Get room details
pub async fn get_room(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Path(room_id): Path<Uuid>,
) -> Result<(StatusCode, HeaderMap, Json<PrivateRoomResponse>)> {
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Find the room
    let room = private_room::Entity::find_by_id(room_id)
        .one(state.db.as_ref())
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound("Room not found".to_string()))?;

    // Check if user has access to view this room
    let is_host = room.host_id == current_user.id;
    let is_participant = room_participants::Entity::find()
        .filter(room_participants::Column::RoomId.eq(room_id))
        .filter(room_participants::Column::ParticipantId.eq(current_user.id))
        .filter(room_participants::Column::Status.eq("Joined"))
        .one(state.db.as_ref())
        .await
        .map_err(AppError::Database)?
        .is_some();

    if !is_host && !is_participant && room.invite_only {
        return Err(AppError::Auth(
            "Access denied to this private room".to_string(),
        ));
    }

    // Get participant count
    let participant_count = room_participants::Entity::find()
        .filter(room_participants::Column::RoomId.eq(room_id))
        .filter(room_participants::Column::Status.eq("Joined"))
        .count(state.db.as_ref())
        .await
        .map_err(AppError::Database)? as usize;

    let response = PrivateRoomResponse::from((room, participant_count));

    Ok((StatusCode::OK, headers, Json(response)))
}

#[cfg(test)]
mod tests {
    use axum::http::{Method, StatusCode};
    use axum_test::TestServer;
    use serde_json::json;
    use std::sync::Arc;
    use uuid::Uuid;

    use super::generate_room_code;
    use crate::test_utils::test_utils::*;
    use entity::{private_room, user};


    #[tokio::test]
    async fn test_create_room_unauthorized() {
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let request_body = json!({
            "room_name": "Test Room",
            "max_players": 4,
            "allow_bots": false,
            "invite_only": false
        });

        let response = server
            .method(Method::POST, "/api/v1/rooms")
            .json(&request_body)
            .await;

        // Should return unauthorized since we're not authenticated
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_join_room_unauthorized() {
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let request_body = json!({
            "room_code": "ABC123"
        });

        let response = server
            .method(Method::POST, "/api/v1/rooms/join")
            .json(&request_body)
            .await;

        // Should return unauthorized since we're not authenticated
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_get_user_rooms_unauthorized() {
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server.method(Method::GET, "/api/v1/rooms").await;

        // Should return unauthorized since we're not authenticated
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_room_validation() {
        // Test room code generation uniqueness
        let code1 = generate_room_code();
        let code2 = generate_room_code();

        // Room codes should be 6 characters and uppercase
        assert_eq!(code1.len(), 6);
        assert_eq!(code2.len(), 6);
        assert!(
            code1
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        );
        assert!(
            code2
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        );

        // Different calls should generate different codes (very high probability)
        assert_ne!(code1, code2);
    }
}
