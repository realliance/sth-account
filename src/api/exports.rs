use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    api::add_rate_limit_headers,
    auth::AuthSession,
    error::{AppError, Result},
    service::AppState,
};
use entity::{
    bot, data_export_requests, friendship, lobby_pool, r#match, notifications, user,
    user_statistics,
};

#[derive(Debug, Serialize, ToSchema)]
pub struct DataExportResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub export_type: String,
    pub status: String,
    pub file_path: Option<String>,
    pub requested_at: chrono::DateTime<chrono::FixedOffset>,
    pub completed_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub expires_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}

impl From<data_export_requests::Model> for DataExportResponse {
    fn from(export: data_export_requests::Model) -> Self {
        Self {
            id: export.id,
            user_id: export.user_id,
            export_type: export.export_type,
            status: export.status,
            file_path: export.file_path,
            requested_at: export.requested_at,
            completed_at: export.completed_at,
            expires_at: export.expires_at,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RequestDataExportRequest {
    #[schema(example = "FullExport")]
    pub export_type: String, // "UserData", "MatchHistory", "BotStatistics", "FullExport"
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct GetExportsQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
    pub status: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserDataExport {
    pub user_profile: UserProfileExport,
    pub statistics: Option<UserStatisticsExport>,
    pub friends: Vec<FriendExport>,
    pub notifications: Vec<NotificationExport>,
    pub bots: Vec<BotExport>,
    pub recent_matches: Vec<MatchExport>,
    pub export_metadata: ExportMetadata,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserProfileExport {
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

#[derive(Debug, Serialize, ToSchema)]
pub struct UserStatisticsExport {
    pub total_games: i32,
    pub wins: i32,
    pub second_place: i32,
    pub third_place: i32,
    pub fourth_place: i32,
    pub average_score: Option<f64>,
    pub peak_mmr: Option<i32>,
    pub current_streak: i32,
    pub last_game_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FriendExport {
    pub friend_username: String,
    pub friendship_status: String,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationExport {
    pub r#type: String,
    pub title: String,
    pub message: String,
    pub read: bool,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BotExport {
    pub name: String,
    pub source_code: Option<String>,
    pub matchmaking_rank: i32,
    pub live: bool,
    pub description: Option<String>,
    pub version: Option<String>,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MatchExport {
    pub match_id: Uuid,
    pub lobby_name: String,
    pub started_at: chrono::DateTime<chrono::FixedOffset>,
    pub completed_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub participant_score: Option<i32>,
    pub participant_mmr_delta: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExportMetadata {
    pub export_date: chrono::DateTime<chrono::Utc>,
    pub export_type: String,
    pub data_retention_policy: String,
    pub contact_info: String,
}

#[utoipa::path(
    post,
    path = "/v1/exports",
    tag = "Exports",
    request_body = RequestDataExportRequest,
    responses(
        (status = 201, description = "Data export requested successfully", body = DataExportResponse),
        (status = 400, description = "Bad request (e.g., invalid type, pending request exists)"),
        (status = 401, description = "Authentication required"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn request_data_export(
    State(state): State<AppState>,
    auth_session: AuthSession,
    Json(request): Json<RequestDataExportRequest>,
) -> Result<(StatusCode, HeaderMap, Json<DataExportResponse>)> {
    let mut headers = HeaderMap::new();
    add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or(AppError::Auth("Not authenticated".to_string()))?;

    // Validate export type
    let valid_types = ["UserData", "MatchHistory", "BotStatistics", "FullExport"];
    if !valid_types.contains(&request.export_type.as_str()) {
        return Err(AppError::BadRequest("Invalid export type".to_string()));
    }

    // Check if user has a pending export request
    let existing_pending = data_export_requests::Entity::find()
        .filter(
            data_export_requests::Column::UserId
                .eq(current_user.id)
                .and(data_export_requests::Column::Status.eq("Pending")),
        )
        .one(&*state.db)
        .await?;

    if existing_pending.is_some() {
        return Err(AppError::BadRequest(
            "You already have a pending export request".to_string(),
        ));
    }

    // Create export request
    let export_request = data_export_requests::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(current_user.id),
        export_type: Set(request.export_type),
        status: Set("Pending".to_string()),
        file_path: Set(None),
        requested_at: Set(Utc::now().into()),
        completed_at: Set(None),
        expires_at: Set(None),
    };

    let created_export = export_request.insert(&*state.db).await?;

    Ok((
        StatusCode::CREATED,
        headers,
        Json(DataExportResponse::from(created_export)),
    ))
}

#[utoipa::path(
    get,
    path = "/v1/exports",
    tag = "Exports",
    params(
        GetExportsQuery
    ),
    responses(
        (status = 200, description = "List of user's export requests", body = serde_json::Value),
        (status = 401, description = "Authentication required"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_export_requests(
    State(state): State<AppState>,
    auth_session: AuthSession,
    Query(query): Query<GetExportsQuery>,
) -> Result<(StatusCode, HeaderMap, Json<serde_json::Value>)> {
    let mut headers = HeaderMap::new();
    add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or(AppError::Auth("Not authenticated".to_string()))?;

    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(10).clamp(1, 50);

    let mut query_builder = data_export_requests::Entity::find()
        .filter(data_export_requests::Column::UserId.eq(current_user.id))
        .order_by_desc(data_export_requests::Column::RequestedAt);

    if let Some(status) = &query.status {
        query_builder = query_builder.filter(data_export_requests::Column::Status.eq(status));
    }

    let total_count = query_builder.clone().count(&*state.db).await?;

    let exports = query_builder
        .offset(Some((page - 1) * per_page))
        .limit(Some(per_page))
        .all(&*state.db)
        .await?;

    let responses: Vec<DataExportResponse> =
        exports.into_iter().map(DataExportResponse::from).collect();
    let total_pages = total_count.div_ceil(per_page);

    Ok((
        StatusCode::OK,
        headers,
        Json(serde_json::json!({
            "exports": responses,
            "total_count": total_count,
            "page": page,
            "per_page": per_page,
            "total_pages": total_pages
        })),
    ))
}

#[utoipa::path(
    get,
    path = "/v1/exports/{id}/download",
    tag = "Exports",
    params(
        ("id" = Uuid, Path, description = "Export ID")
    ),
    responses(
        (status = 200, description = "Export data", body = UserDataExport),
        (status = 400, description = "Bad request (e.g., not completed, expired)"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Export request not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn download_export(
    State(state): State<AppState>,
    auth_session: AuthSession,
    Path(export_id): Path<Uuid>,
) -> Result<(StatusCode, HeaderMap, Json<UserDataExport>)> {
    let mut headers = HeaderMap::new();
    add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or(AppError::Auth("Not authenticated".to_string()))?;

    // Find the export request
    let export_request = data_export_requests::Entity::find_by_id(export_id)
        .one(&*state.db)
        .await?
        .ok_or(AppError::NotFound("Export request not found".to_string()))?;

    // Verify ownership
    if export_request.user_id != current_user.id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    // Check if export is completed
    if export_request.status != "Completed" {
        return Err(AppError::BadRequest(
            "Export is not yet completed".to_string(),
        ));
    }

    // Check if export has expired
    if let Some(expires_at) = export_request.expires_at {
        if Utc::now().naive_utc() > expires_at.naive_utc() {
            return Err(AppError::BadRequest("Export has expired".to_string()));
        }
    }

    // Generate the export data
    let export_data =
        generate_user_data_export(&state, &current_user, &export_request.export_type).await?;

    Ok((StatusCode::OK, headers, Json(export_data)))
}

#[utoipa::path(
    delete,
    path = "/v1/exports/{id}/cancel",
    tag = "Exports",
    params(
        ("id" = Uuid, Path, description = "Export ID")
    ),
    responses(
        (status = 204, description = "Export request cancelled successfully"),
        (status = 400, description = "Bad request (e.g., already completed)"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Export request not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn cancel_export_request(
    State(state): State<AppState>,
    auth_session: AuthSession,
    Path(export_id): Path<Uuid>,
) -> Result<(StatusCode, HeaderMap)> {
    let mut headers = HeaderMap::new();
    add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or(AppError::Auth("Not authenticated".to_string()))?;

    // Find the export request
    let export_request = data_export_requests::Entity::find_by_id(export_id)
        .one(&*state.db)
        .await?
        .ok_or(AppError::NotFound("Export request not found".to_string()))?;

    // Verify ownership
    if export_request.user_id != current_user.id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    // Check if export can be cancelled
    if export_request.status == "Completed" {
        return Err(AppError::BadRequest(
            "Cannot cancel completed export".to_string(),
        ));
    }

    // Delete the export request
    data_export_requests::Entity::delete_by_id(export_id)
        .exec(&*state.db)
        .await?;

    Ok((StatusCode::NO_CONTENT, headers))
}

async fn generate_user_data_export(
    state: &AppState,
    auth_user: &crate::auth::User,
    export_type: &str,
) -> Result<UserDataExport> {
    // Get the full user model from the database
    let user = user::Entity::find_by_id(auth_user.id)
        .one(&*state.db)
        .await?
        .ok_or(AppError::NotFound("User not found".to_string()))?;
    // User profile
    let user_profile = UserProfileExport {
        id: user.id,
        username: user.username.clone(),
        country: user.country.clone(),
        favorite_tile: user.favorite_tile.clone(),
        pronouns: user.pronouns.clone(),
        matchmaking_rank: user.matchmaking_rank,
        role: user.role.clone(),
        email: user.email.clone(),
        account_status: user.account_status.clone(),
        created_at: user.created_at,
        last_active_at: user.last_active_at,
    };

    // User statistics
    let statistics = user_statistics::Entity::find()
        .filter(user_statistics::Column::UserId.eq(user.id))
        .one(&*state.db)
        .await?
        .map(|stats| UserStatisticsExport {
            total_games: stats.total_games,
            wins: stats.wins,
            second_place: stats.second_place,
            third_place: stats.third_place,
            fourth_place: stats.fourth_place,
            average_score: stats
                .average_score
                .map(|d| d.to_string().parse::<f64>().unwrap_or(0.0)),
            peak_mmr: stats.peak_mmr,
            current_streak: stats.current_streak,
            last_game_at: stats.last_game_at,
        });

    // Friends (accepted friendships only)
    let friendships = friendship::Entity::find()
        .filter(
            friendship::Column::Status.eq("Accepted").and(
                friendship::Column::RequesterId
                    .eq(user.id)
                    .or(friendship::Column::AddresseeId.eq(user.id)),
            ),
        )
        .all(&*state.db)
        .await?;

    let mut friends = Vec::new();
    for friendship_model in friendships {
        let friend_id = if friendship_model.requester_id == user.id {
            friendship_model.addressee_id
        } else {
            friendship_model.requester_id
        };

        if let Some(friend) = user::Entity::find_by_id(friend_id).one(&*state.db).await? {
            friends.push(FriendExport {
                friend_username: friend.username,
                friendship_status: friendship_model.status,
                created_at: friendship_model.created_at,
            });
        }
    }

    // Recent notifications (last 100)
    let notifications_data = notifications::Entity::find()
        .filter(notifications::Column::UserId.eq(user.id))
        .order_by_desc(notifications::Column::CreatedAt)
        .limit(100)
        .all(&*state.db)
        .await?;

    let notifications: Vec<NotificationExport> = notifications_data
        .into_iter()
        .map(|n| NotificationExport {
            r#type: n.r#type,
            title: n.title,
            message: n.message,
            read: n.read,
            created_at: n.created_at,
        })
        .collect();

    // User's bots
    let bots_data = bot::Entity::find()
        .filter(bot::Column::OwnerId.eq(user.id))
        .all(&*state.db)
        .await?;

    let bots: Vec<BotExport> = bots_data
        .into_iter()
        .map(|b| BotExport {
            name: b.name,
            source_code: b.source_code,
            matchmaking_rank: b.matchmaking_rank,
            live: b.live,
            description: b.description,
            version: b.version,
            created_at: b.created_at,
        })
        .collect();

    // Recent matches (last 50) where the user participated
    let recent_matches_data = r#match::Entity::find()
        .filter(
            r#match::Column::Participant1Id
                .eq(user.id)
                .or(r#match::Column::Participant2Id.eq(user.id))
                .or(r#match::Column::Participant3Id.eq(user.id))
                .or(r#match::Column::Participant4Id.eq(user.id)),
        )
        .find_also_related(lobby_pool::Entity)
        .order_by_desc(r#match::Column::StartedAt)
        .limit(50)
        .all(&*state.db)
        .await?;

    let mut recent_matches = Vec::new();
    for (match_data, lobby_option) in recent_matches_data {
        // Find which participant slot this user was in
        let (participant_score, participant_mmr_delta) = if match_data.participant1_id == user.id {
            (
                match_data.participant1_score,
                match_data.participant1_mmr_delta,
            )
        } else if match_data.participant2_id == user.id {
            (
                match_data.participant2_score,
                match_data.participant2_mmr_delta,
            )
        } else if match_data.participant3_id == user.id {
            (
                match_data.participant3_score,
                match_data.participant3_mmr_delta,
            )
        } else if match_data.participant4_id == Some(user.id) {
            (
                match_data.participant4_score,
                match_data.participant4_mmr_delta,
            )
        } else {
            (None, None) // This shouldn't happen given our filter
        };

        let lobby_name = lobby_option
            .map(|lobby| lobby.name)
            .unwrap_or_else(|| "Unknown Lobby".to_string());

        recent_matches.push(MatchExport {
            match_id: match_data.id,
            lobby_name,
            started_at: match_data.started_at,
            completed_at: match_data.completed_at,
            participant_score,
            participant_mmr_delta,
        });
    }

    let export_metadata = ExportMetadata {
        export_date: Utc::now(),
        export_type: export_type.to_string(),
        data_retention_policy: "Export files are retained for 7 days after generation".to_string(),
        contact_info: "For questions about your data, contact: privacy@smallturtlehouse.com"
            .to_string(),
    };

    Ok(UserDataExport {
        user_profile,
        statistics,
        friends,
        notifications,
        bots,
        recent_matches,
        export_metadata,
    })
}

#[utoipa::path(
    post,
    path = "/v1/admin/exports/{id}/complete",
    tag = "Admin",
    params(
        ("id" = Uuid, Path, description = "Export ID")
    ),
    responses(
        (status = 200, description = "Export marked as completed", body = DataExportResponse),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Export request not found"),
        (status = 500, description = "Internal server error")
    )
)]
// Admin endpoint to mark export as completed (would typically be called by worker)
pub async fn complete_export(
    State(state): State<AppState>,
    auth_session: AuthSession,
    Path(export_id): Path<Uuid>,
) -> Result<(StatusCode, HeaderMap, Json<DataExportResponse>)> {
    let mut headers = HeaderMap::new();
    add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or(AppError::Auth("Not authenticated".to_string()))?;

    // Check if user has admin privileges (this would typically be called by a worker)
    if current_user.role != "Admin" {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let export_request = data_export_requests::Entity::find_by_id(export_id)
        .one(&*state.db)
        .await?
        .ok_or(AppError::NotFound("Export request not found".to_string()))?;

    let mut active_model: data_export_requests::ActiveModel = export_request.into();
    active_model.status = Set("Completed".to_string());
    active_model.completed_at = Set(Some(Utc::now().into()));
    active_model.expires_at = Set(Some((Utc::now() + Duration::days(7)).into())); // 7 days from now

    let updated_export = active_model.update(&*state.db).await?;

    Ok((
        StatusCode::OK,
        headers,
        Json(DataExportResponse::from(updated_export)),
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
    use entity::data_export_requests;

    #[tokio::test]
    async fn test_export_endpoints_require_auth() {
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server.method(Method::GET, "/v1/exports").await;
        assert!(
            response.status_code() == StatusCode::UNAUTHORIZED
                || response.status_code() == StatusCode::FORBIDDEN,
            "Export endpoints should require authentication"
        );
    }

    #[tokio::test]
    async fn test_export_request_requires_auth() {
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::POST, "/v1/exports")
            .json(&json!({"export_type": "UserData"}))
            .await;
        assert!(
            response.status_code() == StatusCode::UNAUTHORIZED
                || response.status_code() == StatusCode::FORBIDDEN,
            "Export request endpoint should require auth"
        );
    }

    #[tokio::test]
    async fn test_export_request_validation() {
        let user_id = Uuid::new_v4();
        let user = sample_user(Some(user_id));

        let db = create_mock_db()
            .append_query_results([vec![user]])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::POST, "/v1/exports")
            .json(&json!({"export_type": "InvalidType"}))
            .await;

        assert!(
            response.status_code() == StatusCode::BAD_REQUEST
                || response.status_code() == StatusCode::UNAUTHORIZED,
            "Should reject invalid export type"
        );
    }

    #[tokio::test]
    async fn test_valid_export_types() {
        let user_id = Uuid::new_v4();
        let user = sample_user(Some(user_id));

        let valid_types = ["UserData", "MatchHistory", "BotStatistics", "FullExport"];

        for export_type in &valid_types {
            let db = create_mock_db()
                .append_query_results([vec![user.clone()]])
                .append_query_results([Vec::<data_export_requests::Model>::new()])
                .into_connection();

            let app = create_test_app(Arc::new(db));
            let server = TestServer::new(app).unwrap();

            let response = server
                .method(Method::POST, "/v1/exports")
                .json(&json!({"export_type": export_type}))
                .await;

            assert_ne!(
                response.status_code(),
                StatusCode::NOT_FOUND,
                "Export type {export_type} should be valid"
            );
        }
    }
}
