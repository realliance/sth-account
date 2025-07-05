use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set, PaginatorTrait,
    QuerySelect,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    api::add_rate_limit_headers,
    auth::AuthSession,
    error::{AppError, Result},
    service::AppState,
};
use entity::notifications;


#[derive(Debug, Serialize)]
pub struct NotificationResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub r#type: String,
    pub title: String,
    pub message: String,
    pub related_id: Option<Uuid>,
    pub read: bool,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub expires_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}

impl From<notifications::Model> for NotificationResponse {
    fn from(notification: notifications::Model) -> Self {
        Self {
            id: notification.id,
            user_id: notification.user_id,
            r#type: notification.r#type,
            title: notification.title,
            message: notification.message,
            related_id: notification.related_id,
            read: notification.read,
            created_at: notification.created_at,
            expires_at: notification.expires_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct GetNotificationsQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
    pub unread_only: Option<bool>,
    pub notification_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NotificationsResponse {
    pub notifications: Vec<NotificationResponse>,
    pub total_count: u64,
    pub unread_count: u64,
    pub page: u64,
    pub per_page: u64,
    pub total_pages: u64,
}

#[derive(Debug, Deserialize)]
pub struct MarkAsReadRequest {
    pub notification_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct CreateNotificationRequest {
    pub user_id: Uuid,
    pub r#type: String,
    pub title: String,
    pub message: String,
    pub related_id: Option<Uuid>,
    pub expires_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}

pub async fn get_notifications(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Query(query): Query<GetNotificationsQuery>,
) -> Result<(StatusCode, HeaderMap, Json<NotificationsResponse>)> {
    add_rate_limit_headers(&mut headers);

    let current_user = auth_session.user.ok_or(AppError::Auth("Not authenticated".to_string()))?;

    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).min(100).max(1);

    let mut query_builder = notifications::Entity::find()
        .filter(notifications::Column::UserId.eq(current_user.id))
        .order_by_desc(notifications::Column::CreatedAt);

    // Filter by read status if requested
    if query.unread_only.unwrap_or(false) {
        query_builder = query_builder.filter(notifications::Column::Read.eq(false));
    }

    // Filter by notification type if specified
    if let Some(notification_type) = &query.notification_type {
        query_builder = query_builder.filter(notifications::Column::Type.eq(notification_type));
    }

    // Get total count
    let total_count = query_builder.clone().count(&*state.db).await?;

    // Get unread count separately
    let unread_count = notifications::Entity::find()
        .filter(
            notifications::Column::UserId.eq(current_user.id)
                .and(notifications::Column::Read.eq(false))
        )
        .count(&*state.db)
        .await?;

    // Apply pagination
    let notifications = query_builder
        .offset(Some((page - 1) * per_page))
        .limit(Some(per_page))
        .all(&*state.db)
        .await?;

    let total_pages = (total_count + per_page - 1) / per_page;

    let response = NotificationsResponse {
        notifications: notifications.into_iter().map(NotificationResponse::from).collect(),
        total_count,
        unread_count,
        page,
        per_page,
        total_pages,
    };

    Ok((StatusCode::OK, headers, Json(response)))
}

pub async fn mark_notifications_as_read(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Json(request): Json<MarkAsReadRequest>,
) -> Result<(StatusCode, HeaderMap, Json<serde_json::Value>)> {
    add_rate_limit_headers(&mut headers);

    let current_user = auth_session.user.ok_or(AppError::Auth("Not authenticated".to_string()))?;

    // Find notifications belonging to the current user
    let notifications = notifications::Entity::find()
        .filter(
            notifications::Column::UserId.eq(current_user.id)
                .and(notifications::Column::Id.is_in(request.notification_ids.clone()))
        )
        .all(&*state.db)
        .await?;

    let mut updated_count = 0;

    for notification in notifications {
        if !notification.read {
            let mut active_model: notifications::ActiveModel = notification.into();
            active_model.read = Set(true);
            active_model.update(&*state.db).await?;
            updated_count += 1;
        }
    }

    Ok((
        StatusCode::OK,
        headers,
        Json(serde_json::json!({
            "updated_count": updated_count,
            "message": format!("{} notifications marked as read", updated_count)
        })),
    ))
}

pub async fn mark_all_notifications_as_read(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
) -> Result<(StatusCode, HeaderMap, Json<serde_json::Value>)> {
    add_rate_limit_headers(&mut headers);

    let current_user = auth_session.user.ok_or(AppError::Auth("Not authenticated".to_string()))?;

    // Update all unread notifications for the current user
    let update_result = notifications::Entity::update_many()
        .col_expr(notifications::Column::Read, true.into())
        .filter(
            notifications::Column::UserId.eq(current_user.id)
                .and(notifications::Column::Read.eq(false))
        )
        .exec(&*state.db)
        .await?;

    Ok((
        StatusCode::OK,
        headers,
        Json(serde_json::json!({
            "updated_count": update_result.rows_affected,
            "message": format!("{} notifications marked as read", update_result.rows_affected)
        })),
    ))
}

pub async fn delete_notification(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Path(notification_id): Path<Uuid>,
) -> Result<(StatusCode, HeaderMap)> {
    add_rate_limit_headers(&mut headers);

    let current_user = auth_session.user.ok_or(AppError::Auth("Not authenticated".to_string()))?;

    // Find the notification and verify ownership
    let notification = notifications::Entity::find_by_id(notification_id)
        .one(&*state.db)
        .await?
        .ok_or(AppError::NotFound("Notification not found".to_string()))?;

    if notification.user_id != current_user.id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    // Delete the notification
    notifications::Entity::delete_by_id(notification_id)
        .exec(&*state.db)
        .await?;

    Ok((StatusCode::NO_CONTENT, headers))
}

pub async fn get_notification_summary(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
) -> Result<(StatusCode, HeaderMap, Json<serde_json::Value>)> {
    add_rate_limit_headers(&mut headers);

    let current_user = auth_session.user.ok_or(AppError::Auth("Not authenticated".to_string()))?;

    // Get counts by type and read status
    let total_count = notifications::Entity::find()
        .filter(notifications::Column::UserId.eq(current_user.id))
        .count(&*state.db)
        .await?;

    let unread_count = notifications::Entity::find()
        .filter(
            notifications::Column::UserId.eq(current_user.id)
                .and(notifications::Column::Read.eq(false))
        )
        .count(&*state.db)
        .await?;

    let friend_request_count = notifications::Entity::find()
        .filter(
            notifications::Column::UserId.eq(current_user.id)
                .and(notifications::Column::Type.eq("FriendRequest"))
                .and(notifications::Column::Read.eq(false))
        )
        .count(&*state.db)
        .await?;

    let system_message_count = notifications::Entity::find()
        .filter(
            notifications::Column::UserId.eq(current_user.id)
                .and(notifications::Column::Type.eq("SystemMessage"))
                .and(notifications::Column::Read.eq(false))
        )
        .count(&*state.db)
        .await?;

    Ok((
        StatusCode::OK,
        headers,
        Json(serde_json::json!({
            "total_count": total_count,
            "unread_count": unread_count,
            "by_type": {
                "friend_requests": friend_request_count,
                "system_messages": system_message_count
            }
        })),
    ))
}

// Admin endpoint to create notifications (for system announcements, etc.)
pub async fn create_notification(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Json(request): Json<CreateNotificationRequest>,
) -> Result<(StatusCode, HeaderMap, Json<NotificationResponse>)> {
    add_rate_limit_headers(&mut headers);

    let current_user = auth_session.user.ok_or(AppError::Auth("Not authenticated".to_string()))?;

    // Check if user has admin privileges
    if current_user.role != "Admin" && current_user.role != "Moderator" {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let notification = notifications::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(request.user_id),
        r#type: Set(request.r#type),
        title: Set(request.title),
        message: Set(request.message),
        related_id: Set(request.related_id),
        read: Set(false),
        created_at: Set(Utc::now().into()),
        expires_at: Set(request.expires_at),
    };

    let created_notification = notification.insert(&*state.db).await?;

    Ok((
        StatusCode::CREATED,
        headers,
        Json(NotificationResponse::from(created_notification)),
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
    async fn test_notification_endpoints_require_auth() {
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let endpoints = [
            ("/api/v1/notifications", Method::GET),
            ("/api/v1/notifications/summary", Method::GET),
        ];

        for (endpoint, method) in &endpoints {
            let response = server.method(method.clone(), endpoint).await;
            
            assert!(
                response.status_code() == StatusCode::UNAUTHORIZED || response.status_code() == StatusCode::FORBIDDEN,
                "Endpoint {} {} should require authentication, got {}",
                method,
                endpoint,
                response.status_code()
            );
        }
    }

    #[tokio::test]
    async fn test_notification_mark_read_validation() {
        let user_id = Uuid::new_v4();
        let user = sample_user(Some(user_id));

        let db = create_mock_db()
            .append_query_results([vec![user]])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::POST, "/api/v1/notifications/mark-read")
            .json(&json!({"notification_ids": ["not-a-uuid"]}))
            .await;

        assert!(
            response.status_code() == StatusCode::BAD_REQUEST ||
            response.status_code() == StatusCode::UNPROCESSABLE_ENTITY ||
            response.status_code() == StatusCode::UNAUTHORIZED,
            "Should reject invalid notification IDs"
        );
    }
}