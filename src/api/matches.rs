use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, Set,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    auth::AuthSession,
    error::{AppError, Result},
};
use entity::{bot, game_history, lobby_pool, r#match, user};

#[derive(Debug, Serialize)]
pub struct MatchResponse {
    pub id: Uuid,
    pub lobby_id: Uuid,
    pub started_at: chrono::DateTime<chrono::FixedOffset>,
    pub completed_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub participants: Vec<MatchParticipant>,
}

#[derive(Debug, Serialize)]
pub struct MatchParticipant {
    pub participant_type: String, // "Human" or "Bot"
    pub participant_id: Uuid,
    pub score: Option<i32>,
    pub mmr_delta: Option<i32>,
    pub placement: Option<i32>, // 1st, 2nd, 3rd, 4th
}

impl From<r#match::Model> for MatchResponse {
    fn from(match_model: r#match::Model) -> Self {
        let mut participants = vec![
            MatchParticipant {
                participant_type: match_model.participant1_type,
                participant_id: match_model.participant1_id,
                score: match_model.participant1_score,
                mmr_delta: match_model.participant1_mmr_delta,
                placement: calculate_placement(match_model.participant1_score),
            },
            MatchParticipant {
                participant_type: match_model.participant2_type,
                participant_id: match_model.participant2_id,
                score: match_model.participant2_score,
                mmr_delta: match_model.participant2_mmr_delta,
                placement: calculate_placement(match_model.participant2_score),
            },
            MatchParticipant {
                participant_type: match_model.participant3_type,
                participant_id: match_model.participant3_id,
                score: match_model.participant3_score,
                mmr_delta: match_model.participant3_mmr_delta,
                placement: calculate_placement(match_model.participant3_score),
            },
        ];

        // Add 4th participant if present
        if let (Some(p4_type), Some(p4_id)) =
            (match_model.participant4_type, match_model.participant4_id)
        {
            participants.push(MatchParticipant {
                participant_type: p4_type,
                participant_id: p4_id,
                score: match_model.participant4_score,
                mmr_delta: match_model.participant4_mmr_delta,
                placement: calculate_placement(match_model.participant4_score),
            });
        }

        // Sort participants by score (highest first) to determine proper placement
        participants.sort_by(|a, b| b.score.unwrap_or(0).cmp(&a.score.unwrap_or(0)));
        for (index, participant) in participants.iter_mut().enumerate() {
            participant.placement = Some((index + 1) as i32);
        }

        Self {
            id: match_model.id,
            lobby_id: match_model.lobby_id,
            started_at: match_model.started_at,
            completed_at: match_model.completed_at,
            participants,
        }
    }
}

fn calculate_placement(score: Option<i32>) -> Option<i32> {
    // Placeholder - actual placement calculation happens after all scores are known
    score.map(|_| 0)
}

#[derive(Debug, Deserialize)]
pub struct MatchQuery {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub lobby_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct MatchHistoryResponse {
    pub matches: Vec<MatchResponse>,
    pub total_count: u64,
    pub limit: u64,
    pub offset: u64,
}

#[derive(Debug, Serialize)]
pub struct UserStatsResponse {
    pub user_id: Uuid,
    pub total_games: u64,
    pub wins: u64,
    pub second_place: u64,
    pub third_place: u64,
    pub fourth_place: u64,
    pub average_score: f64,
    pub current_mmr: i32,
    pub peak_mmr: i32,
}

#[derive(Debug, Serialize)]
pub struct BotStatsResponse {
    pub bot_id: Uuid,
    pub total_games: u64,
    pub wins: u64,
    pub second_place: u64,
    pub third_place: u64,
    pub fourth_place: u64,
    pub average_score: f64,
    pub current_mmr: i32,
    pub peak_mmr: i32,
}

/// Get match history for a user
pub async fn get_user_match_history(
    State(db): State<Arc<DatabaseConnection>>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Path(user_id): Path<Uuid>,
    Query(query): Query<MatchQuery>,
) -> Result<(StatusCode, HeaderMap, Json<MatchHistoryResponse>)> {
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Users can only view their own match history unless they're admin
    if current_user.id != user_id && current_user.role != "Admin" {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let limit = query.limit.unwrap_or(50).min(100); // Cap at 100
    let offset = query.offset.unwrap_or(0);

    // Build query to find matches where this user participated
    let mut match_query = r#match::Entity::find()
        .filter(
            r#match::Column::Participant1Id
                .eq(user_id)
                .or(r#match::Column::Participant2Id.eq(user_id))
                .or(r#match::Column::Participant3Id.eq(user_id))
                .or(r#match::Column::Participant4Id.eq(user_id)),
        )
        .order_by_desc(r#match::Column::StartedAt);

    // Filter by lobby if specified
    if let Some(lobby_id) = query.lobby_id {
        match_query = match_query.filter(r#match::Column::LobbyId.eq(lobby_id));
    }

    // Get total count
    let total_count = match_query.clone().count(db.as_ref()).await?;

    // Get paginated results
    let matches = match_query
        .limit(limit)
        .offset(offset)
        .all(db.as_ref())
        .await?;

    let match_responses: Vec<MatchResponse> =
        matches.into_iter().map(MatchResponse::from).collect();

    let response = MatchHistoryResponse {
        matches: match_responses,
        total_count,
        limit,
        offset,
    };

    Ok((StatusCode::OK, headers, Json(response)))
}

/// Get match history for a bot
pub async fn get_bot_match_history(
    State(db): State<Arc<DatabaseConnection>>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Path(bot_id): Path<Uuid>,
    Query(query): Query<MatchQuery>,
) -> Result<(StatusCode, HeaderMap, Json<MatchHistoryResponse>)> {
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Verify bot exists and user has access to view its history
    let bot = bot::Entity::find_by_id(bot_id)
        .one(db.as_ref())
        .await?
        .ok_or_else(|| AppError::Service("Bot not found".to_string()))?;

    if bot.owner_id != current_user.id && current_user.role != "Admin" {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let limit = query.limit.unwrap_or(50).min(100); // Cap at 100
    let offset = query.offset.unwrap_or(0);

    // Build query to find matches where this bot participated
    let mut match_query = r#match::Entity::find()
        .filter(
            r#match::Column::Participant1Id
                .eq(bot_id)
                .or(r#match::Column::Participant2Id.eq(bot_id))
                .or(r#match::Column::Participant3Id.eq(bot_id))
                .or(r#match::Column::Participant4Id.eq(bot_id)),
        )
        .order_by_desc(r#match::Column::StartedAt);

    // Filter by lobby if specified
    if let Some(lobby_id) = query.lobby_id {
        match_query = match_query.filter(r#match::Column::LobbyId.eq(lobby_id));
    }

    // Get total count
    let total_count = match_query.clone().count(db.as_ref()).await?;

    // Get paginated results
    let matches = match_query
        .limit(limit)
        .offset(offset)
        .all(db.as_ref())
        .await?;

    let match_responses: Vec<MatchResponse> =
        matches.into_iter().map(MatchResponse::from).collect();

    let response = MatchHistoryResponse {
        matches: match_responses,
        total_count,
        limit,
        offset,
    };

    Ok((StatusCode::OK, headers, Json(response)))
}

/// Get a specific match details
pub async fn get_match(
    State(db): State<Arc<DatabaseConnection>>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Path(match_id): Path<Uuid>,
) -> Result<(StatusCode, HeaderMap, Json<MatchResponse>)> {
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    let match_model = r#match::Entity::find_by_id(match_id)
        .one(db.as_ref())
        .await?
        .ok_or_else(|| AppError::Service("Match not found".to_string()))?;

    // Check if user participated in this match or is admin
    let user_participated = match_model.participant1_id == current_user.id
        || match_model.participant2_id == current_user.id
        || match_model.participant3_id == current_user.id
        || match_model.participant4_id == Some(current_user.id);

    // Check if user owns any bots that participated
    let mut bot_participated = false;
    if !user_participated && current_user.role != "Admin" {
        let user_bots = bot::Entity::find()
            .filter(bot::Column::OwnerId.eq(current_user.id))
            .all(db.as_ref())
            .await?;

        let user_bot_ids: Vec<Uuid> = user_bots.into_iter().map(|bot| bot.id).collect();

        bot_participated = user_bot_ids.contains(&match_model.participant1_id)
            || user_bot_ids.contains(&match_model.participant2_id)
            || user_bot_ids.contains(&match_model.participant3_id)
            || match_model
                .participant4_id
                .map_or(false, |id| user_bot_ids.contains(&id));
    }

    if !user_participated && !bot_participated && current_user.role != "Admin" {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let match_response = MatchResponse::from(match_model);
    Ok((StatusCode::OK, headers, Json(match_response)))
}

/// Get user statistics
pub async fn get_user_stats(
    State(db): State<Arc<DatabaseConnection>>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Path(user_id): Path<Uuid>,
) -> Result<(StatusCode, HeaderMap, Json<UserStatsResponse>)> {
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Users can only view their own stats unless they're admin
    if current_user.id != user_id && current_user.role != "Admin" {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    // Verify user exists
    let target_user = user::Entity::find_by_id(user_id)
        .one(db.as_ref())
        .await?
        .ok_or_else(|| AppError::Service("User not found".to_string()))?;

    // Get all completed matches for this user
    let user_matches = r#match::Entity::find()
        .filter(
            r#match::Column::Participant1Id
                .eq(user_id)
                .or(r#match::Column::Participant2Id.eq(user_id))
                .or(r#match::Column::Participant3Id.eq(user_id))
                .or(r#match::Column::Participant4Id.eq(user_id)),
        )
        .filter(r#match::Column::CompletedAt.is_not_null())
        .all(db.as_ref())
        .await?;

    // Calculate statistics
    let total_games = user_matches.len() as u64;
    let mut wins = 0u64;
    let mut second_place = 0u64;
    let mut third_place = 0u64;
    let mut fourth_place = 0u64;
    let mut total_score = 0i64;
    let mut score_count = 0u64;

    for match_model in user_matches {
        let match_response = MatchResponse::from(match_model);
        if let Some(user_participant) = match_response
            .participants
            .iter()
            .find(|p| p.participant_id == user_id)
        {
            if let Some(score) = user_participant.score {
                total_score += score as i64;
                score_count += 1;
            }

            match user_participant.placement {
                Some(1) => wins += 1,
                Some(2) => second_place += 1,
                Some(3) => third_place += 1,
                Some(4) => fourth_place += 1,
                _ => {} // Shouldn't happen in completed matches
            }
        }
    }

    let average_score = if score_count > 0 {
        total_score as f64 / score_count as f64
    } else {
        0.0
    };

    let stats = UserStatsResponse {
        user_id,
        total_games,
        wins,
        second_place,
        third_place,
        fourth_place,
        average_score,
        current_mmr: target_user.matchmaking_rank,
        peak_mmr: target_user.matchmaking_rank, // TODO: Track peak MMR separately
    };

    Ok((StatusCode::OK, headers, Json(stats)))
}

/// Get bot statistics
pub async fn get_bot_stats(
    State(db): State<Arc<DatabaseConnection>>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Path(bot_id): Path<Uuid>,
) -> Result<(StatusCode, HeaderMap, Json<BotStatsResponse>)> {
    super::add_rate_limit_headers(&mut headers);

    let current_user = auth_session
        .user
        .ok_or_else(|| AppError::Auth("Authentication required".to_string()))?;

    // Verify bot exists and user has access
    let bot = bot::Entity::find_by_id(bot_id)
        .one(db.as_ref())
        .await?
        .ok_or_else(|| AppError::Service("Bot not found".to_string()))?;

    if bot.owner_id != current_user.id && current_user.role != "Admin" {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    // Get all completed matches for this bot
    let bot_matches = r#match::Entity::find()
        .filter(
            r#match::Column::Participant1Id
                .eq(bot_id)
                .or(r#match::Column::Participant2Id.eq(bot_id))
                .or(r#match::Column::Participant3Id.eq(bot_id))
                .or(r#match::Column::Participant4Id.eq(bot_id)),
        )
        .filter(r#match::Column::CompletedAt.is_not_null())
        .all(db.as_ref())
        .await?;

    // Calculate statistics (similar to user stats)
    let total_games = bot_matches.len() as u64;
    let mut wins = 0u64;
    let mut second_place = 0u64;
    let mut third_place = 0u64;
    let mut fourth_place = 0u64;
    let mut total_score = 0i64;
    let mut score_count = 0u64;

    for match_model in bot_matches {
        let match_response = MatchResponse::from(match_model);
        if let Some(bot_participant) = match_response
            .participants
            .iter()
            .find(|p| p.participant_id == bot_id)
        {
            if let Some(score) = bot_participant.score {
                total_score += score as i64;
                score_count += 1;
            }

            match bot_participant.placement {
                Some(1) => wins += 1,
                Some(2) => second_place += 1,
                Some(3) => third_place += 1,
                Some(4) => fourth_place += 1,
                _ => {} // Shouldn't happen in completed matches
            }
        }
    }

    let average_score = if score_count > 0 {
        total_score as f64 / score_count as f64
    } else {
        0.0
    };

    let stats = BotStatsResponse {
        bot_id,
        total_games,
        wins,
        second_place,
        third_place,
        fourth_place,
        average_score,
        current_mmr: bot.matchmaking_rank,
        peak_mmr: bot.matchmaking_rank, // TODO: Track peak MMR separately
    };

    Ok((StatusCode::OK, headers, Json(stats)))
}

#[cfg(test)]
mod tests {
    use axum::http::{Method, StatusCode};
    use axum_test::TestServer;
    use std::sync::Arc;
    use uuid::Uuid;

    use crate::test_utils::test_utils::*;

    #[tokio::test]
    async fn test_get_user_match_history_unauthorized() {
        let user_id = Uuid::new_v4();
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::GET, &format!("/matches/user/{}", user_id))
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
    async fn test_get_bot_match_history_unauthorized() {
        let bot_id = Uuid::new_v4();
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::GET, &format!("/matches/bot/{}", bot_id))
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
    async fn test_get_match_unauthorized() {
        let match_id = Uuid::new_v4();
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::GET, &format!("/matches/{}", match_id))
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
    async fn test_get_user_stats_unauthorized() {
        let user_id = Uuid::new_v4();
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::GET, &format!("/stats/user/{}", user_id))
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
    async fn test_get_bot_stats_unauthorized() {
        let bot_id = Uuid::new_v4();
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::GET, &format!("/stats/bot/{}", bot_id))
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
