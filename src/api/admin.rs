use axum::{
    extract::{Path, Query, State, ConnectInfo},
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
use std::net::SocketAddr;

use crate::{
    api::add_rate_limit_headers,
    auth::AuthSession,
    error::{AppError, Result},
    service::AppState,
};
use entity::{user, report, audit_log, system_configuration, notifications};

fn extract_ip_address(headers: &HeaderMap, connect_info: Option<ConnectInfo<SocketAddr>>) -> String {
    // Check for X-Forwarded-For header (common in reverse proxy setups)
    if let Some(forwarded) = headers.get("X-Forwarded-For") {
        if let Ok(forwarded_str) = forwarded.to_str() {
            if let Some(first_ip) = forwarded_str.split(',').next() {
                return first_ip.trim().to_string();
            }
        }
    }
    
    // Check for X-Real-IP header (nginx)
    if let Some(real_ip) = headers.get("X-Real-IP") {
        if let Ok(ip_str) = real_ip.to_str() {
            return ip_str.to_string();
        }
    }
    
    // Fall back to connection info
    if let Some(ConnectInfo(socket_addr)) = connect_info {
        return socket_addr.ip().to_string();
    }
    
    // Default fallback
    "unknown".to_string()
}


#[derive(Debug, Serialize)]
pub struct ReportResponse {
    pub id: Uuid,
    pub author_id: Uuid,
    pub accused_ids: String,
    pub match_id: Option<Uuid>,
    pub offense_type: String,
    pub description: Option<String>,
    pub status: String,
    pub write_up: Option<String>,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub severity: String,
    pub mod_report_author: Option<Uuid>,
    pub concluded_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub author_info: Option<UserInfo>,
}

#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub id: Uuid,
    pub username: String,
    pub role: String,
    pub account_status: String,
}

#[derive(Debug, Serialize)]
pub struct AuditLogResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub action_type: String,
    pub details: Option<serde_json::Value>,
    pub ip_address: String,
    pub moderator_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub user_info: Option<UserInfo>,
    pub moderator_info: Option<UserInfo>,
}

#[derive(Debug, Deserialize)]
pub struct GetReportsQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
    pub status: Option<String>,
    pub severity: Option<String>,
    pub offense_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateReportRequest {
    pub status: String,
    pub write_up: Option<String>,
    pub severity: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UserModerationRequest {
    pub action: String, // "suspend", "ban", "activate", "change_role"
    pub reason: String,
    pub duration_days: Option<i32>, // For suspensions
    pub new_role: Option<String>, // For role changes
}

#[derive(Debug, Deserialize)]
pub struct SystemConfigRequest {
    pub key: String,
    pub value: serde_json::Value,
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SystemConfigResponse {
    pub id: Uuid,
    pub key: String,
    pub value: serde_json::Value,
    pub description: Option<String>,
    pub updated_by: Uuid,
    pub updated_at: chrono::DateTime<chrono::FixedOffset>,
    pub updated_by_info: Option<UserInfo>,
}

// Report management endpoints

pub async fn get_reports(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Query(query): Query<GetReportsQuery>,
) -> Result<(StatusCode, HeaderMap, Json<serde_json::Value>)> {
    add_rate_limit_headers(&mut headers);

    let current_user = auth_session.user.ok_or(AppError::Auth("Not authenticated".to_string()))?;

    // Check if user has moderator/admin privileges
    if current_user.role != "Admin" && current_user.role != "Moderator" {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);

    let mut query_builder = report::Entity::find()
        .order_by_desc(report::Column::CreatedAt);

    // Apply filters
    if let Some(status) = &query.status {
        query_builder = query_builder.filter(report::Column::Status.eq(status));
    }
    if let Some(severity) = &query.severity {
        query_builder = query_builder.filter(report::Column::Severity.eq(severity));
    }
    if let Some(offense_type) = &query.offense_type {
        query_builder = query_builder.filter(report::Column::OffenseType.eq(offense_type));
    }

    let total_count = query_builder.clone().count(&*state.db).await?;

    let reports = query_builder
        .offset(Some((page - 1) * per_page))
        .limit(Some(per_page))
        .all(&*state.db)
        .await?;

    let mut responses = Vec::new();
    
    for report_model in reports {
        // Get author info
        let author = user::Entity::find_by_id(report_model.author_id)
            .one(&*state.db)
            .await?;

        let author_info = author.map(|u| UserInfo {
            id: u.id,
            username: u.username,
            role: u.role,
            account_status: u.account_status,
        });

        responses.push(ReportResponse {
            id: report_model.id,
            author_id: report_model.author_id,
            accused_ids: report_model.accused_ids,
            match_id: report_model.match_id,
            offense_type: report_model.offense_type,
            description: report_model.description,
            status: report_model.status,
            write_up: report_model.write_up,
            created_at: report_model.created_at,
            severity: report_model.severity,
            mod_report_author: report_model.mod_report_author,
            concluded_at: report_model.concluded_at,
            author_info,
        });
    }

    let total_pages = total_count.div_ceil(per_page);

    Ok((
        StatusCode::OK,
        headers,
        Json(serde_json::json!({
            "reports": responses,
            "total_count": total_count,
            "page": page,
            "per_page": per_page,
            "total_pages": total_pages
        })),
    ))
}

pub async fn update_report(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Path(report_id): Path<Uuid>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(request): Json<UpdateReportRequest>,
) -> Result<(StatusCode, HeaderMap, Json<ReportResponse>)> {
    add_rate_limit_headers(&mut headers);

    let current_user = auth_session.user.ok_or(AppError::Auth("Not authenticated".to_string()))?;

    // Check if user has moderator/admin privileges
    if current_user.role != "Admin" && current_user.role != "Moderator" {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let report = report::Entity::find_by_id(report_id)
        .one(&*state.db)
        .await?
        .ok_or(AppError::NotFound("Report not found".to_string()))?;

    let mut active_model: report::ActiveModel = report.clone().into();
    active_model.status = Set(request.status.clone());
    active_model.mod_report_author = Set(Some(current_user.id));
    
    if let Some(write_up) = request.write_up {
        active_model.write_up = Set(Some(write_up));
    }
    
    if let Some(severity) = request.severity {
        active_model.severity = Set(severity);
    }

    if request.status == "ActionTaken" || request.status == "Dismissed" {
        active_model.concluded_at = Set(Some(Utc::now().into()));
    }

    let updated_report = active_model.update(&*state.db).await?;

    // Create audit log entry
    let audit_entry = audit_log::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(current_user.id),
        action_type: Set("ReportUpdated".to_string()),
        details: Set(Some(serde_json::json!({
            "report_id": report_id,
            "new_status": request.status,
            "previous_status": report.status
        }))),
        ip_address: Set(extract_ip_address(&headers, Some(ConnectInfo(addr)))),
        moderator_id: Set(Some(current_user.id)),
        created_at: Set(Utc::now().into()),
        deleted_at: Set(None),
    };

    audit_entry.insert(&*state.db).await?;

    // Get author info for response
    let author = user::Entity::find_by_id(updated_report.author_id)
        .one(&*state.db)
        .await?;

    let author_info = author.map(|u| UserInfo {
        id: u.id,
        username: u.username,
        role: u.role,
        account_status: u.account_status,
    });

    let response = ReportResponse {
        id: updated_report.id,
        author_id: updated_report.author_id,
        accused_ids: updated_report.accused_ids,
        match_id: updated_report.match_id,
        offense_type: updated_report.offense_type,
        description: updated_report.description,
        status: updated_report.status,
        write_up: updated_report.write_up,
        created_at: updated_report.created_at,
        severity: updated_report.severity,
        mod_report_author: updated_report.mod_report_author,
        concluded_at: updated_report.concluded_at,
        author_info,
    };

    Ok((StatusCode::OK, headers, Json(response)))
}

// User moderation endpoints

pub async fn moderate_user(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Path(user_id): Path<Uuid>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(request): Json<UserModerationRequest>,
) -> Result<(StatusCode, HeaderMap, Json<serde_json::Value>)> {
    add_rate_limit_headers(&mut headers);

    let current_user = auth_session.user.ok_or(AppError::Auth("Not authenticated".to_string()))?;

    // Check if user has admin privileges (only admins can moderate)
    if current_user.role != "Admin" {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let target_user = user::Entity::find_by_id(user_id)
        .one(&*state.db)
        .await?
        .ok_or(AppError::NotFound("User not found".to_string()))?;

    let mut active_model: user::ActiveModel = target_user.clone().into();
    let mut action_details = serde_json::json!({
        "target_user_id": user_id,
        "action": request.action,
        "reason": request.reason
    });

    match request.action.as_str() {
        "suspend" => {
            active_model.account_status = Set("Suspended".to_string());
            if let Some(duration) = request.duration_days {
                action_details["duration_days"] = serde_json::Value::from(duration);
            }
        }
        "ban" => {
            active_model.account_status = Set("Banned".to_string());
        }
        "activate" => {
            active_model.account_status = Set("Active".to_string());
        }
        "change_role" => {
            if let Some(new_role) = &request.new_role {
                active_model.role = Set(new_role.clone());
                action_details["new_role"] = serde_json::Value::from(new_role.as_str());
                action_details["previous_role"] = serde_json::Value::from(target_user.role.as_str());
            } else {
                return Err(AppError::BadRequest("new_role required for change_role action".to_string()));
            }
        }
        _ => {
            return Err(AppError::BadRequest("Invalid action".to_string()));
        }
    }

    let updated_user = active_model.update(&*state.db).await?;

    // Create audit log entry
    let audit_entry = audit_log::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(target_user.id),
        action_type: Set(format!("UserModeration_{}", request.action)),
        details: Set(Some(action_details)),
        ip_address: Set(extract_ip_address(&headers, Some(ConnectInfo(addr)))),
        moderator_id: Set(Some(current_user.id)),
        created_at: Set(Utc::now().into()),
        deleted_at: Set(None),
    };

    audit_entry.insert(&*state.db).await?;

    // Create notification for the user
    let notification = notifications::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(target_user.id),
        r#type: Set("ModeratorAction".to_string()),
        title: Set("Account Status Update".to_string()),
        message: Set(format!("Your account has been updated: {}", request.reason)),
        related_id: Set(None),
        read: Set(false),
        created_at: Set(Utc::now().into()),
        expires_at: Set(None),
    };

    notification.insert(&*state.db).await?;

    Ok((
        StatusCode::OK,
        headers,
        Json(serde_json::json!({
            "message": format!("User {} successfully", request.action),
            "user_id": user_id,
            "new_status": updated_user.account_status,
            "new_role": updated_user.role
        })),
    ))
}

// System configuration endpoints

pub async fn get_system_config(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
) -> Result<(StatusCode, HeaderMap, Json<Vec<SystemConfigResponse>>)> {
    add_rate_limit_headers(&mut headers);

    let current_user = auth_session.user.ok_or(AppError::Auth("Not authenticated".to_string()))?;

    // Check if user has admin privileges
    if current_user.role != "Admin" {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let configs = system_configuration::Entity::find()
        .order_by_asc(system_configuration::Column::Key)
        .all(&*state.db)
        .await?;

    let mut responses = Vec::new();

    for config in configs {
        let updated_by_user = user::Entity::find_by_id(config.updated_by)
            .one(&*state.db)
            .await?;

        let updated_by_info = updated_by_user.map(|u| UserInfo {
            id: u.id,
            username: u.username,
            role: u.role,
            account_status: u.account_status,
        });

        responses.push(SystemConfigResponse {
            id: config.id,
            key: config.key,
            value: config.value,
            description: config.description,
            updated_by: config.updated_by,
            updated_at: config.updated_at,
            updated_by_info,
        });
    }

    Ok((StatusCode::OK, headers, Json(responses)))
}

pub async fn update_system_config(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(request): Json<SystemConfigRequest>,
) -> Result<(StatusCode, HeaderMap, Json<SystemConfigResponse>)> {
    add_rate_limit_headers(&mut headers);

    let current_user = auth_session.user.ok_or(AppError::Auth("Not authenticated".to_string()))?;

    // Check if user has admin privileges
    if current_user.role != "Admin" {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    // Check if config exists
    let existing_config = system_configuration::Entity::find()
        .filter(system_configuration::Column::Key.eq(&request.key))
        .one(&*state.db)
        .await?;

    let config = if let Some(existing) = existing_config {
        // Update existing config
        let mut active_model: system_configuration::ActiveModel = existing.into();
        active_model.value = Set(request.value);
        active_model.updated_by = Set(current_user.id);
        active_model.updated_at = Set(Utc::now().into());
        
        if let Some(description) = request.description {
            active_model.description = Set(Some(description));
        }

        active_model.update(&*state.db).await?
    } else {
        // Create new config
        let new_config = system_configuration::ActiveModel {
            id: Set(Uuid::new_v4()),
            key: Set(request.key.clone()),
            value: Set(request.value),
            description: Set(request.description),
            updated_by: Set(current_user.id),
            updated_at: Set(Utc::now().into()),
        };

        new_config.insert(&*state.db).await?
    };

    // Create audit log entry
    let audit_entry = audit_log::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(current_user.id),
        action_type: Set("SystemConfigUpdated".to_string()),
        details: Set(Some(serde_json::json!({
            "config_key": request.key,
            "config_id": config.id
        }))),
        ip_address: Set(extract_ip_address(&headers, Some(ConnectInfo(addr)))),
        moderator_id: Set(Some(current_user.id)),
        created_at: Set(Utc::now().into()),
        deleted_at: Set(None),
    };

    audit_entry.insert(&*state.db).await?;

    let updated_by_info = Some(UserInfo {
        id: current_user.id,
        username: current_user.username,
        role: current_user.role,
        account_status: current_user.account_status,
    });

    let response = SystemConfigResponse {
        id: config.id,
        key: config.key,
        value: config.value,
        description: config.description,
        updated_by: config.updated_by,
        updated_at: config.updated_at,
        updated_by_info,
    };

    Ok((StatusCode::OK, headers, Json(response)))
}

pub async fn get_audit_logs(
    State(state): State<AppState>,
    auth_session: AuthSession,
    mut headers: HeaderMap,
    Query(query): Query<GetReportsQuery>, // Reuse the same query structure
) -> Result<(StatusCode, HeaderMap, Json<serde_json::Value>)> {
    add_rate_limit_headers(&mut headers);

    let current_user = auth_session.user.ok_or(AppError::Auth("Not authenticated".to_string()))?;

    // Check if user has admin privileges
    if current_user.role != "Admin" {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);

    let query_builder = audit_log::Entity::find()
        .filter(audit_log::Column::DeletedAt.is_null()) // Only non-deleted entries
        .order_by_desc(audit_log::Column::CreatedAt);

    let total_count = query_builder.clone().count(&*state.db).await?;

    let audit_logs = query_builder
        .offset(Some((page - 1) * per_page))
        .limit(Some(per_page))
        .all(&*state.db)
        .await?;

    let mut responses = Vec::new();

    for log in audit_logs {
        // Get user info
        let user_info = user::Entity::find_by_id(log.user_id)
            .one(&*state.db)
            .await?
            .map(|u| UserInfo {
                id: u.id,
                username: u.username,
                role: u.role,
                account_status: u.account_status,
            });

        // Get moderator info if present
        let moderator_info = if let Some(mod_id) = log.moderator_id {
            user::Entity::find_by_id(mod_id)
                .one(&*state.db)
                .await?
                .map(|u| UserInfo {
                    id: u.id,
                    username: u.username,
                    role: u.role,
                    account_status: u.account_status,
                })
        } else {
            None
        };

        responses.push(AuditLogResponse {
            id: log.id,
            user_id: log.user_id,
            action_type: log.action_type,
            details: log.details,
            ip_address: log.ip_address,
            moderator_id: log.moderator_id,
            created_at: log.created_at,
            user_info,
            moderator_info,
        });
    }

    let total_pages = total_count.div_ceil(per_page);

    Ok((
        StatusCode::OK,
        headers,
        Json(serde_json::json!({
            "audit_logs": responses,
            "total_count": total_count,
            "page": page,
            "per_page": per_page,
            "total_pages": total_pages
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
    async fn test_admin_endpoints_require_auth() {
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let endpoints = [
            ("/api/v1/admin/reports", Method::GET),
            ("/api/v1/admin/config", Method::GET),
            ("/api/v1/admin/audit-logs", Method::GET),
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
    async fn test_admin_notification_creation_requires_auth() {
        let db = create_mock_db().into_connection();
        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::POST, "/api/v1/admin/notifications")
            .json(&json!({
                "user_id": Uuid::new_v4(),
                "type": "SystemMessage",
                "title": "Test",
                "message": "Test message"
            }))
            .await;
        assert!(
            response.status_code() == StatusCode::UNAUTHORIZED || response.status_code() == StatusCode::FORBIDDEN,
            "Admin notification endpoint should require auth"
        );
    }

    #[tokio::test]
    async fn test_admin_endpoints_reject_regular_users() {
        let user_id = Uuid::new_v4();
        let regular_user = sample_user(Some(user_id));

        let db = create_mock_db()
            .append_query_results([vec![regular_user.clone()]])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::GET, "/api/v1/admin/config")
            .await;

        assert!(
            response.status_code() == StatusCode::FORBIDDEN ||
            response.status_code() == StatusCode::UNAUTHORIZED,
            "Admin config endpoint should reject regular users"
        );
    }

    #[tokio::test]
    async fn test_moderator_endpoints_accept_moderators() {
        let moderator_id = Uuid::new_v4();
        let moderator = sample_moderator_user(Some(moderator_id));

        let db = create_mock_db()
            .append_query_results([vec![moderator]])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::GET, "/api/v1/admin/reports")
            .await;

        assert_ne!(
            response.status_code(),
            StatusCode::FORBIDDEN,
            "Reports endpoint should not reject moderators"
        );
    }

    #[tokio::test]
    async fn test_admin_endpoints_accept_admins() {
        let admin_id = Uuid::new_v4();
        let admin = sample_admin_user(Some(admin_id));

        let db = create_mock_db()
            .append_query_results([vec![admin]])
            .into_connection();

        let app = create_test_app(Arc::new(db));
        let server = TestServer::new(app).unwrap();

        let response = server
            .method(Method::GET, "/api/v1/admin/config")
            .await;

        assert_ne!(
            response.status_code(),
            StatusCode::FORBIDDEN,
            "Admin config endpoint should not reject admins"
        );
    }
}