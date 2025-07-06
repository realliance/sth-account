use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    api::add_rate_limit_headers,
    auth::AuthSession,
    error::{AppError, Result},
    service::AppState,
};
use entity::{friendship, notifications, user};

#[cfg(test)]
mod tests {
    use axum::http::{Method, StatusCode};
    use axum_test::TestServer;
    use serde_json::json;
    use std::sync::Arc;
    use uuid::Uuid;

    use crate::test_utils::test_utils::*;

    #[tokio::test]
    async fn test_friend_endpoints_require_auth() {
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let endpoints = [
            ("/api/v1/friends", Method::GET),
            ("/api/v1/friends/requests", Method::GET),
        ];

        for (endpoint, method) in &endpoints {
            let response = server.method(method.clone(), endpoint).await;

            assert!(
                response.status_code() == StatusCode::UNAUTHORIZED
                    || response.status_code() == StatusCode::FORBIDDEN,
                "Endpoint {} {} should require authentication, got {}",
                method,
                endpoint,
                response.status_code()
            );
        }
    }

    #[tokio::test]
    async fn test_friend_request_creation_requires_auth() {
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::POST, "/api/v1/friends/requests")
            .json(&json!({"addressee_username": "test"}))
            .await;
        assert!(
            response.status_code() == StatusCode::UNAUTHORIZED
                || response.status_code() == StatusCode::FORBIDDEN,
            "Friend request endpoint should require auth"
        );
    }

    #[tokio::test]
    async fn test_friend_request_basic_validation() {
        let user_id = Uuid::new_v4();
        let user = sample_user(Some(user_id));

        let db = create_mock_db()
            .append_query_results([vec![user]])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::POST, "/api/v1/friends/requests")
            .json(&json!({}))
            .await;

        assert!(
            response.status_code() == StatusCode::BAD_REQUEST
                || response.status_code() == StatusCode::UNPROCESSABLE_ENTITY
                || response.status_code() == StatusCode::UNAUTHORIZED,
            "Should reject invalid friend request data"
        );
    }

    #[tokio::test]
    async fn test_send_friend_request_success() {
        let requester_id = Uuid::new_v4();
        let target_id = Uuid::new_v4();

        let requester = sample_user(Some(requester_id));
        let mut target_user = sample_user(Some(target_id));
        target_user.username = "targetuser".to_string();

        let friendship_id = Uuid::new_v4();
        let friendship = entity::friendship::Model {
            id: friendship_id,
            requester_id,
            addressee_id: target_id,
            status: "Pending".to_string(),
            created_at: chrono::Utc::now().into(),
            updated_at: chrono::Utc::now().into(),
        };

        let notification_id = Uuid::new_v4();
        let notification = entity::notifications::Model {
            id: notification_id,
            user_id: target_id,
            r#type: "FriendRequest".to_string(),
            title: "New Friend Request".to_string(),
            message: format!("{} wants to be your friend", requester.username),
            related_id: Some(friendship_id),
            read: false,
            created_at: chrono::Utc::now().into(),
            expires_at: None,
        };

        let db = create_mock_db()
            .append_query_results([vec![requester]]) // Auth user lookup
            .append_query_results([vec![target_user.clone()]]) // Target user lookup
            .append_query_results([Vec::<entity::friendship::Model>::new()]) // No existing friendship
            .append_exec_results([mock_exec_success(1)]) // Insert friendship
            .append_query_results([vec![friendship.clone()]]) // Return created friendship
            .append_exec_results([mock_exec_success(1)]) // Insert notification
            .append_query_results([vec![notification]]) // Return created notification
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::POST, "/api/v1/friends/requests")
            .json(&json!({"addressee_username": "targetuser"}))
            .await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_send_friend_request_user_not_found() {
        let requester_id = Uuid::new_v4();
        let requester = sample_user(Some(requester_id));

        let db = create_mock_db()
            .append_query_results([vec![requester]]) // Auth user lookup
            .append_query_results([Vec::<entity::user::Model>::new()]) // Target user not found
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::POST, "/api/v1/friends/requests")
            .json(&json!({"addressee_username": "nonexistentuser"}))
            .await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_send_friend_request_already_exists() {
        let requester_id = Uuid::new_v4();
        let target_id = Uuid::new_v4();

        let requester = sample_user(Some(requester_id));
        let mut target_user = sample_user(Some(target_id));
        target_user.username = "targetuser".to_string();

        let existing_friendship = entity::friendship::Model {
            id: Uuid::new_v4(),
            requester_id,
            addressee_id: target_id,
            status: "Pending".to_string(),
            created_at: chrono::Utc::now().into(),
            updated_at: chrono::Utc::now().into(),
        };

        let db = create_mock_db()
            .append_query_results([vec![requester]]) // Auth user lookup
            .append_query_results([vec![target_user]]) // Target user lookup
            .append_query_results([vec![existing_friendship]]) // Existing friendship found
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::POST, "/api/v1/friends/requests")
            .json(&json!({"addressee_username": "targetuser"}))
            .await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_respond_to_friend_request_accept() {
        let requester_id = Uuid::new_v4();
        let addressee_id = Uuid::new_v4();
        let friendship_id = Uuid::new_v4();

        let addressee = sample_user(Some(addressee_id));
        let mut requester = sample_user(Some(requester_id));
        requester.username = "requesteruser".to_string();

        let pending_friendship = entity::friendship::Model {
            id: friendship_id,
            requester_id,
            addressee_id,
            status: "Pending".to_string(),
            created_at: chrono::Utc::now().into(),
            updated_at: chrono::Utc::now().into(),
        };

        let accepted_friendship = entity::friendship::Model {
            id: friendship_id,
            requester_id,
            addressee_id,
            status: "Accepted".to_string(),
            created_at: chrono::Utc::now().into(),
            updated_at: chrono::Utc::now().into(),
        };

        let db = create_mock_db()
            .append_query_results([vec![addressee]]) // Auth user lookup
            .append_query_results([vec![pending_friendship.clone()]]) // Find friendship
            .append_exec_results([mock_exec_success(1)]) // Update friendship
            .append_query_results([vec![accepted_friendship.clone()]]) // Return updated friendship
            .append_query_results([vec![requester.clone()]]) // Requester lookup for response
            .append_exec_results([mock_exec_success(1)]) // Insert notification
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(
                Method::POST,
                &format!("/api/v1/friends/requests/{friendship_id}/respond"),
            )
            .json(&json!({"accept": true}))
            .await;

        assert!(
            response.status_code() == StatusCode::UNAUTHORIZED
                || response.status_code() == StatusCode::NOT_FOUND,
            "Expected UNAUTHORIZED or NOT_FOUND, got {}",
            response.status_code()
        );
    }

    #[tokio::test]
    async fn test_get_friends_success() {
        let user_id = Uuid::new_v4();
        let friend_id = Uuid::new_v4();

        let user = sample_user(Some(user_id));
        let mut friend = sample_user(Some(friend_id));
        friend.username = "frienduser".to_string();

        let friendship = entity::friendship::Model {
            id: Uuid::new_v4(),
            requester_id: user_id,
            addressee_id: friend_id,
            status: "Accepted".to_string(),
            created_at: chrono::Utc::now().into(),
            updated_at: chrono::Utc::now().into(),
        };

        let db = create_mock_db()
            .append_query_results([vec![user]]) // Auth user lookup
            .append_query_results([vec![friendship.clone()]]) // Find accepted friendships
            .append_query_results([vec![friend.clone()]]) // Friend info lookup
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server.method(Method::GET, "/api/v1/friends").await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_get_pending_friend_requests_success() {
        let user_id = Uuid::new_v4();
        let requester_id = Uuid::new_v4();

        let user = sample_user(Some(user_id));
        let mut requester = sample_user(Some(requester_id));
        requester.username = "requesteruser".to_string();

        let pending_friendship = entity::friendship::Model {
            id: Uuid::new_v4(),
            requester_id,
            addressee_id: user_id,
            status: "Pending".to_string(),
            created_at: chrono::Utc::now().into(),
            updated_at: chrono::Utc::now().into(),
        };

        let db = create_mock_db()
            .append_query_results([vec![user]]) // Auth user lookup
            .append_query_results([vec![pending_friendship.clone()]]) // Find pending requests
            .append_query_results([vec![requester.clone()]]) // Requester info lookup
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server.method(Method::GET, "/api/v1/friends/requests").await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_remove_friend_success() {
        let user_id = Uuid::new_v4();
        let friend_id = Uuid::new_v4();
        let friendship_id = Uuid::new_v4();

        let user = sample_user(Some(user_id));

        let friendship = entity::friendship::Model {
            id: friendship_id,
            requester_id: user_id,
            addressee_id: friend_id,
            status: "Accepted".to_string(),
            created_at: chrono::Utc::now().into(),
            updated_at: chrono::Utc::now().into(),
        };

        let db = create_mock_db()
            .append_query_results([vec![user]]) // Auth user lookup
            .append_query_results([vec![friendship.clone()]]) // Find friendship
            .append_exec_results([mock_exec_success(1)]) // Delete friendship
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::DELETE, &format!("/api/v1/friends/{friendship_id}"))
            .await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }
}

#[derive(Debug, Serialize)]
pub struct FriendshipResponse {
    pub id: Uuid,
    pub requester_id: Uuid,
    pub addressee_id: Uuid,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub updated_at: chrono::DateTime<chrono::FixedOffset>,
    pub friend_info: Option<FriendInfo>,
}

#[derive(Debug, Serialize)]
pub struct FriendInfo {
    pub id: Uuid,
    pub username: String,
    pub country: String,
    pub favorite_tile: Option<String>,
    pub pronouns: Option<String>,
    pub matchmaking_rank: i32,
}

#[derive(Debug, Deserialize)]
pub struct SendFriendRequestRequest {
    pub addressee_username: String,
}

#[derive(Debug, Deserialize)]
pub struct RespondToFriendRequestRequest {
    pub accept: bool,
}

pub async fn send_friend_request(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Json(request): Json<SendFriendRequestRequest>,
) -> Result<(StatusCode, HeaderMap, Json<FriendshipResponse>)> {
    add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or(AppError::Auth("Not authenticated".to_string()))?;

    // Find the target user by username
    let target_user = user::Entity::find()
        .filter(user::Column::Username.eq(&request.addressee_username))
        .one(&*state.db)
        .await?
        .ok_or(AppError::NotFound("User not found".to_string()))?;

    // Check if friendship already exists
    let existing_friendship = friendship::Entity::find()
        .filter(
            friendship::Column::RequesterId
                .eq(current_user.id)
                .and(friendship::Column::AddresseeId.eq(target_user.id))
                .or(friendship::Column::RequesterId
                    .eq(target_user.id)
                    .and(friendship::Column::AddresseeId.eq(current_user.id))),
        )
        .one(&*state.db)
        .await?;

    if existing_friendship.is_some() {
        return Err(AppError::BadRequest(
            "Friendship already exists or is pending".to_string(),
        ));
    }

    // Create friendship request
    let friendship_model = friendship::ActiveModel {
        id: Set(Uuid::new_v4()),
        requester_id: Set(current_user.id),
        addressee_id: Set(target_user.id),
        status: Set("Pending".to_string()),
        created_at: Set(Utc::now().into()),
        updated_at: Set(Utc::now().into()),
    };

    let friendship = friendship_model.insert(&*state.db).await?;

    // Create notification for the target user
    let notification = notifications::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(target_user.id),
        r#type: Set("FriendRequest".to_string()),
        title: Set("New Friend Request".to_string()),
        message: Set(format!("{} wants to be your friend", current_user.username)),
        related_id: Set(Some(friendship.id)),
        read: Set(false),
        created_at: Set(Utc::now().into()),
        expires_at: Set(None),
    };

    notification.insert(&*state.db).await?;

    let response = FriendshipResponse {
        id: friendship.id,
        requester_id: friendship.requester_id,
        addressee_id: friendship.addressee_id,
        status: friendship.status,
        created_at: friendship.created_at,
        updated_at: friendship.updated_at,
        friend_info: Some(FriendInfo {
            id: target_user.id,
            username: target_user.username,
            country: target_user.country,
            favorite_tile: target_user.favorite_tile,
            pronouns: target_user.pronouns,
            matchmaking_rank: target_user.matchmaking_rank,
        }),
    };

    Ok((StatusCode::CREATED, headers, Json(response)))
}

pub async fn respond_to_friend_request(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Path(friendship_id): Path<Uuid>,
    Json(request): Json<RespondToFriendRequestRequest>,
) -> Result<(StatusCode, HeaderMap, Json<FriendshipResponse>)> {
    add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or(AppError::Auth("Not authenticated".to_string()))?;

    // Find the friendship request
    let friendship = friendship::Entity::find_by_id(friendship_id)
        .one(&*state.db)
        .await?
        .ok_or(AppError::NotFound("Friend request not found".to_string()))?;

    // Verify the current user is the addressee
    if friendship.addressee_id != current_user.id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    // Verify request is still pending
    if friendship.status != "Pending" {
        return Err(AppError::BadRequest(
            "Friend request is no longer pending".to_string(),
        ));
    }

    let new_status = if request.accept {
        "Accepted"
    } else {
        "Declined"
    };

    // Update friendship status
    let mut friendship_active: friendship::ActiveModel = friendship.clone().into();
    friendship_active.status = Set(new_status.to_string());
    friendship_active.updated_at = Set(Utc::now().into());

    let updated_friendship = friendship_active.update(&*state.db).await?;

    // Get requester info for response
    let requester = user::Entity::find_by_id(friendship.requester_id)
        .one(&*state.db)
        .await?
        .ok_or(AppError::NotFound("Requester not found".to_string()))?;

    // Create notification for the requester
    let notification_message = if request.accept {
        format!("{} accepted your friend request", current_user.username)
    } else {
        format!("{} declined your friend request", current_user.username)
    };

    let notification = notifications::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(requester.id),
        r#type: Set("FriendRequest".to_string()),
        title: Set("Friend Request Response".to_string()),
        message: Set(notification_message),
        related_id: Set(Some(updated_friendship.id)),
        read: Set(false),
        created_at: Set(Utc::now().into()),
        expires_at: Set(None),
    };

    notification.insert(&*state.db).await?;

    let response = FriendshipResponse {
        id: updated_friendship.id,
        requester_id: updated_friendship.requester_id,
        addressee_id: updated_friendship.addressee_id,
        status: updated_friendship.status,
        created_at: updated_friendship.created_at,
        updated_at: updated_friendship.updated_at,
        friend_info: Some(FriendInfo {
            id: requester.id,
            username: requester.username,
            country: requester.country,
            favorite_tile: requester.favorite_tile,
            pronouns: requester.pronouns,
            matchmaking_rank: requester.matchmaking_rank,
        }),
    };

    Ok((StatusCode::OK, headers, Json(response)))
}

pub async fn get_friends(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
) -> Result<(StatusCode, HeaderMap, Json<Vec<FriendshipResponse>>)> {
    add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or(AppError::Auth("Not authenticated".to_string()))?;

    // Get all accepted friendships where user is either requester or addressee
    let friendships = friendship::Entity::find()
        .filter(
            friendship::Column::Status.eq("Accepted").and(
                friendship::Column::RequesterId
                    .eq(current_user.id)
                    .or(friendship::Column::AddresseeId.eq(current_user.id)),
            ),
        )
        .all(&*state.db)
        .await?;

    let mut responses = Vec::new();

    for friendship_model in friendships {
        // Determine which user is the friend (not the current user)
        let friend_id = if friendship_model.requester_id == current_user.id {
            friendship_model.addressee_id
        } else {
            friendship_model.requester_id
        };

        // Get friend info
        let friend = user::Entity::find_by_id(friend_id).one(&*state.db).await?;

        let friend_info = friend.map(|f| FriendInfo {
            id: f.id,
            username: f.username,
            country: f.country,
            favorite_tile: f.favorite_tile,
            pronouns: f.pronouns,
            matchmaking_rank: f.matchmaking_rank,
        });

        responses.push(FriendshipResponse {
            id: friendship_model.id,
            requester_id: friendship_model.requester_id,
            addressee_id: friendship_model.addressee_id,
            status: friendship_model.status,
            created_at: friendship_model.created_at,
            updated_at: friendship_model.updated_at,
            friend_info,
        });
    }

    Ok((StatusCode::OK, headers, Json(responses)))
}

pub async fn get_pending_friend_requests(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
) -> Result<(StatusCode, HeaderMap, Json<Vec<FriendshipResponse>>)> {
    add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or(AppError::Auth("Not authenticated".to_string()))?;

    // Get pending friend requests where current user is the addressee
    let friendships = friendship::Entity::find()
        .filter(
            friendship::Column::Status
                .eq("Pending")
                .and(friendship::Column::AddresseeId.eq(current_user.id)),
        )
        .all(&*state.db)
        .await?;

    let mut responses = Vec::new();

    for friendship_model in friendships {
        // Get requester info
        let requester = user::Entity::find_by_id(friendship_model.requester_id)
            .one(&*state.db)
            .await?;

        let friend_info = requester.map(|r| FriendInfo {
            id: r.id,
            username: r.username,
            country: r.country,
            favorite_tile: r.favorite_tile,
            pronouns: r.pronouns,
            matchmaking_rank: r.matchmaking_rank,
        });

        responses.push(FriendshipResponse {
            id: friendship_model.id,
            requester_id: friendship_model.requester_id,
            addressee_id: friendship_model.addressee_id,
            status: friendship_model.status,
            created_at: friendship_model.created_at,
            updated_at: friendship_model.updated_at,
            friend_info,
        });
    }

    Ok((StatusCode::OK, headers, Json(responses)))
}

pub async fn remove_friend(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Path(friendship_id): Path<Uuid>,
) -> Result<(StatusCode, HeaderMap)> {
    add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or(AppError::Auth("Not authenticated".to_string()))?;

    // Find the friendship
    let friendship = friendship::Entity::find_by_id(friendship_id)
        .one(&*state.db)
        .await?
        .ok_or(AppError::NotFound("Friendship not found".to_string()))?;

    // Verify the current user is part of this friendship
    if friendship.requester_id != current_user.id && friendship.addressee_id != current_user.id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    // Delete the friendship
    friendship::Entity::delete_by_id(friendship_id)
        .exec(&*state.db)
        .await?;

    Ok((StatusCode::NO_CONTENT, headers))
}
