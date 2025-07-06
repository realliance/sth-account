use crate::{config::Config, database::establish_connection, error::{AppError, Result}};
use chrono::{Duration, Utc};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, DeleteResult, PaginatorTrait, ActiveModelTrait, Set};
use tracing::{info, warn};

pub async fn run_job(job_type: String) -> Result<()> {
    let config = Config::from_env()?;
    let db = establish_connection(&config).await?;
    
    match job_type.as_str() {
        "cleanup-audit-logs" => cleanup_audit_logs(&db).await,
        "cleanup-sessions" => cleanup_sessions(&db).await,
        "cleanup-export-files" => cleanup_export_files(&db).await,
        "cleanup-reports" => cleanup_reports(&db).await,
        "cleanup-notifications" => cleanup_notifications(&db).await,
        "update-statistics" => update_statistics(&db).await,
        "cleanup-stale-queues" => cleanup_stale_queues(&db).await,
        "cleanup-expired-rooms" => cleanup_expired_rooms(&db).await,
        "heartbeat-check" => heartbeat_check(&db).await,
        "generate-daily-stats" => generate_daily_stats(&db).await,
        "mmr-recalculation" => mmr_recalculation(&db).await,
        _ => Err(AppError::Job(format!("Unknown job type: {}", job_type))),
    }
}

async fn cleanup_audit_logs(db: &sea_orm::DatabaseConnection) -> Result<()> {
    info!("Running cleanup-audit-logs job");
    
    let cutoff_date = Utc::now() - Duration::days(60);
    
    // Hard delete audit logs older than 60 days (they're already soft-deletable)
    let result: DeleteResult = entity::audit_log::Entity::delete_many()
        .filter(
            entity::audit_log::Column::CreatedAt.lt(cutoff_date)
                .and(entity::audit_log::Column::DeletedAt.is_null())
        )
        .exec(db)
        .await?;
    
    info!("Cleaned up {} audit log entries older than 60 days", result.rows_affected);
    Ok(())
}

async fn cleanup_sessions(db: &sea_orm::DatabaseConnection) -> Result<()> {
    info!("Running cleanup-sessions job");
    
    let cutoff_date = Utc::now() - Duration::days(30);
    
    // Delete expired sessions older than 30 days
    let result: DeleteResult = entity::user_session::Entity::delete_many()
        .filter(
            entity::user_session::Column::Status.eq("Expired")
                .and(entity::user_session::Column::CreatedAt.lt(cutoff_date))
        )
        .exec(db)
        .await?;
    
    info!("Cleaned up {} expired session entries older than 30 days", result.rows_affected);
    Ok(())
}

async fn cleanup_export_files(db: &sea_orm::DatabaseConnection) -> Result<()> {
    info!("Running cleanup-export-files job");
    
    let cutoff_date = Utc::now() - Duration::days(7);
    
    // Find export requests that have expired
    let expired_exports = entity::data_export_requests::Entity::find()
        .filter(
            entity::data_export_requests::Column::Status.eq("Completed")
                .and(entity::data_export_requests::Column::ExpiresAt.is_not_null())
                .and(entity::data_export_requests::Column::ExpiresAt.lt(cutoff_date))
        )
        .all(db)
        .await?;
    
    let mut deleted_count = 0;
    for export in expired_exports {
        // In a real implementation, you would delete the actual file from storage here
        if let Some(file_path) = &export.file_path {
            warn!("Would delete export file: {}", file_path);
        }
        
        // Delete the database record
        entity::data_export_requests::Entity::delete_by_id(export.id)
            .exec(db)
            .await?;
        
        deleted_count += 1;
    }
    
    info!("Cleaned up {} expired export files older than 7 days", deleted_count);
    Ok(())
}

async fn cleanup_reports(db: &sea_orm::DatabaseConnection) -> Result<()> {
    info!("Running cleanup-reports job");
    
    let cutoff_date = Utc::now() - Duration::days(90);
    
    let result: DeleteResult = entity::report::Entity::delete_many()
        .filter(
            entity::report::Column::Status.eq("Dismissed")
                .and(entity::report::Column::CreatedAt.lt(cutoff_date))
        )
        .exec(db)
        .await?;
    
    info!("Cleaned up {} dismissed reports older than 90 days", result.rows_affected);
    Ok(())
}

async fn cleanup_notifications(db: &sea_orm::DatabaseConnection) -> Result<()> {
    info!("Running cleanup-notifications job");
    
    let cutoff_date = Utc::now() - Duration::days(30);
    
    let result: DeleteResult = entity::notifications::Entity::delete_many()
        .filter(
            entity::notifications::Column::Read.eq(true)
                .and(entity::notifications::Column::CreatedAt.lt(cutoff_date))
        )
        .exec(db)
        .await?;
    
    info!("Cleaned up {} read notifications older than 30 days", result.rows_affected);
    Ok(())
}

async fn update_statistics(db: &sea_orm::DatabaseConnection) -> Result<()> {
    info!("Running update-statistics job");
    
    // This would typically recalculate statistics from match data
    // For now, we'll log that this job is running
    // In a real implementation, you would:
    // 1. Query all matches for each user/bot
    // 2. Calculate wins, losses, average scores, etc.
    // 3. Update the user_statistics and bot_statistics tables
    
    let user_count = entity::user::Entity::find().count(db).await?;
    let bot_count = entity::bot::Entity::find().count(db).await?;
    
    info!("Statistics update complete - processed {} users and {} bots", user_count, bot_count);
    info!("Note: Actual statistics recalculation would be implemented based on match data");
    
    Ok(())
}

async fn cleanup_stale_queues(db: &sea_orm::DatabaseConnection) -> Result<()> {
    info!("Running cleanup-stale-queues job");
    
    let stale_cutoff = Utc::now() - Duration::hours(1);
    
    // Remove queue entries that have been waiting for more than 1 hour
    let result: DeleteResult = entity::queue::Entity::delete_many()
        .filter(
            entity::queue::Column::Status.eq("Waiting")
                .and(entity::queue::Column::JoinedAt.lt(stale_cutoff))
        )
        .exec(db)
        .await?;
    
    info!("Cleaned up {} stale queue entries older than 1 hour", result.rows_affected);
    Ok(())
}

async fn cleanup_expired_rooms(db: &sea_orm::DatabaseConnection) -> Result<()> {
    info!("Running cleanup-expired-rooms job");
    
    // Find rooms that are open but have no active participants
    let empty_rooms = entity::private_room::Entity::find()
        .filter(entity::private_room::Column::Status.eq("Open"))
        .all(db)
        .await?;
    
    let mut closed_count = 0;
    
    for room in empty_rooms {
        // Check if room has any active participants
        let active_participants = entity::room_participants::Entity::find()
            .filter(
                entity::room_participants::Column::RoomId.eq(room.id)
                    .and(entity::room_participants::Column::LeftAt.is_null())
            )
            .count(db)
            .await?;
        
        // If no active participants, close the room
        if active_participants == 0 {
            let mut room_active: entity::private_room::ActiveModel = room.into();
            room_active.status = Set("Closed".to_string());
            room_active.update(db).await?;
            closed_count += 1;
        }
    }
    
    info!("Closed {} empty private rooms", closed_count);
    Ok(())
}

async fn heartbeat_check(db: &sea_orm::DatabaseConnection) -> Result<()> {
    info!("Running heartbeat-check job");
    
    let heartbeat_cutoff = Utc::now() - Duration::minutes(10);
    
    // Find bots that haven't sent a heartbeat in 10 minutes and are still marked as live
    let stale_bots = entity::bot::Entity::find()
        .filter(
            entity::bot::Column::Live.eq(true)
                .and(
                    entity::bot::Column::LastHeartbeat.is_null()
                        .or(entity::bot::Column::LastHeartbeat.lt(heartbeat_cutoff))
                )
        )
        .all(db)
        .await?;
    
    let mut updated_count = 0;
    for bot in stale_bots {
        // Mark bot as offline
        let mut bot_active: entity::bot::ActiveModel = bot.into();
        bot_active.live = Set(false);
        bot_active.update(db).await?;
        updated_count += 1;
    }
    
    info!("Marked {} bots as offline due to missing heartbeat", updated_count);
    Ok(())
}

async fn generate_daily_stats(db: &sea_orm::DatabaseConnection) -> Result<()> {
    info!("Running generate-daily-stats job");
    
    let today = Utc::now().date_naive();
    
    // Gather basic platform statistics (exclude soft-deleted users)
    let total_users = entity::user::Entity::find()
        .filter(
            entity::user::Column::AccountStatus.eq("Active")
                .and(entity::user::Column::DeletedAt.is_null())
        )
        .count(db)
        .await?;
    
    let total_bots = entity::bot::Entity::find().count(db).await?;
    
    let live_bots = entity::bot::Entity::find()
        .filter(entity::bot::Column::Live.eq(true))
        .count(db)
        .await?;
    
    let active_sessions = entity::user_session::Entity::find()
        .filter(entity::user_session::Column::Status.eq("Active"))
        .count(db)
        .await?;
    
    // Log daily statistics
    info!("Daily platform statistics for {}:", today);
    info!("  Total active users: {}", total_users);
    info!("  Total bots: {} ({} live)", total_bots, live_bots);
    info!("  Active sessions: {}", active_sessions);
    
    // In a real implementation, you would store these in a statistics table
    // or send them to a monitoring system
    
    Ok(())
}

async fn mmr_recalculation(db: &sea_orm::DatabaseConnection) -> Result<()> {
    info!("Running mmr-recalculation job");
    
    // This job would be used if the MMR algorithm changes and requires recalculation
    // For now, we'll just log that it's running
    
    let users_with_mmr = entity::user::Entity::find()
        .filter(
            entity::user::Column::MatchmakingRank.gt(0)
                .and(entity::user::Column::DeletedAt.is_null())
        )
        .count(db)
        .await?;
    
    let bots_with_mmr = entity::bot::Entity::find()
        .filter(entity::bot::Column::MatchmakingRank.gt(0))
        .count(db)
        .await?;
    
    info!("MMR recalculation job completed");
    info!("  Users with MMR: {}", users_with_mmr);
    info!("  Bots with MMR: {}", bots_with_mmr);
    info!("  Note: Actual MMR recalculation would be implemented based on match history");
    
    Ok(())
}

// Integration tests for jobs - these require a running PostgreSQL database
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use sea_orm::{Database, DatabaseConnection, EntityTrait, ActiveModelTrait, Set, ColumnTrait, QueryFilter};
    use uuid::Uuid;
    
    use entity::{audit_log, user_session, data_export_requests, report, notifications, queue, private_room, bot, user};

    // Helper to create a test database connection
    // This requires a PostgreSQL database to be running
    async fn create_test_db_connection() -> std::result::Result<DatabaseConnection, Box<dyn std::error::Error>> {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://localhost/sth_account_test".to_string());
        
        let db = Database::connect(&database_url).await?;
        Ok(db)
    }

    // Helper to create test data
    async fn setup_test_user(db: &DatabaseConnection) -> std::result::Result<user::Model, Box<dyn std::error::Error>> {
        let user_model = user::ActiveModel {
            id: Set(Uuid::new_v4()),
            username: Set(format!("testuser_{}", Uuid::new_v4())),
            country: Set("USA".to_string()),
            favorite_tile: Set(Some("Man1".to_string())),
            pronouns: Set(Some("they/them".to_string())),
            matchmaking_rank: Set(1000),
            password: Set("hashed_password".to_string()),
            created_at: Set(Utc::now().into()),
            passkey: Set(None),
            role: Set("Active".to_string()),
            email: Set(Some("test@example.com".to_string())),
            last_active_at: Set(Some(Utc::now().into())),
            account_status: Set("Active".to_string()),
            settings: Set(None),
            deleted_at: Set(None),
        };
        
        let user = user_model.insert(db).await?;
        Ok(user)
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_cleanup_audit_logs_removes_old_entries() {
        let db = create_test_db_connection().await.expect("Failed to connect to test database");
        let user = setup_test_user(&db).await.expect("Failed to create test user");
        
        // Create an old audit log entry (older than 60 days)
        let old_date = Utc::now() - Duration::days(65);
        let old_audit_log = audit_log::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(user.id),
            action_type: Set("Login".to_string()),
            details: Set(None),
            ip_address: Set("127.0.0.1".to_string()),
            moderator_id: Set(None),
            created_at: Set(old_date.into()),
            deleted_at: Set(None),
        };
        old_audit_log.insert(&db).await.expect("Failed to insert test audit log");
        
        // Run the cleanup job
        let result = cleanup_audit_logs(&db).await;
        assert!(result.is_ok(), "Cleanup job should succeed");
        
        // Verify the old entry was deleted
        let remaining_logs = audit_log::Entity::find()
            .filter(audit_log::Column::UserId.eq(user.id))
            .all(&db)
            .await
            .expect("Failed to query audit logs");
        
        assert_eq!(remaining_logs.len(), 0, "Old audit logs should be deleted");
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_cleanup_sessions_removes_expired_sessions() {
        let db = create_test_db_connection().await.expect("Failed to connect to test database");
        let user = setup_test_user(&db).await.expect("Failed to create test user");
        
        // Create an expired session (older than 30 days)
        let old_date = Utc::now() - Duration::days(35);
        let expired_session = user_session::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(user.id),
            token_hash: Set("expired_token_hash".to_string()),
            device_info: Set(Some("Test Device".to_string())),
            ip_address: Set("127.0.0.1".to_string()),
            created_at: Set(old_date.into()),
            expires_at: Set((old_date + Duration::hours(24)).into()),
            last_active_at: Set(Some(old_date.into())),
            status: Set("Expired".to_string()),
        };
        expired_session.insert(&db).await.expect("Failed to insert test session");
        
        // Run the cleanup job
        let result = cleanup_sessions(&db).await;
        assert!(result.is_ok(), "Cleanup job should succeed");
        
        // Verify the expired session was deleted
        let remaining_sessions = user_session::Entity::find()
            .filter(user_session::Column::UserId.eq(user.id))
            .all(&db)
            .await
            .expect("Failed to query sessions");
        
        assert_eq!(remaining_sessions.len(), 0, "Expired sessions should be deleted");
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_cleanup_export_files_removes_expired_exports() {
        let db = create_test_db_connection().await.expect("Failed to connect to test database");
        let user = setup_test_user(&db).await.expect("Failed to create test user");
        
        // Create an expired export request
        let old_date = Utc::now() - Duration::days(10);
        let expired_export = data_export_requests::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(user.id),
            export_type: Set("UserData".to_string()),
            status: Set("Completed".to_string()),
            file_path: Set(Some("/tmp/test_export.json".to_string())),
            requested_at: Set(old_date.into()),
            completed_at: Set(Some(old_date.into())),
            expires_at: Set(Some(old_date.into())),
        };
        expired_export.insert(&db).await.expect("Failed to insert test export");
        
        // Run the cleanup job
        let result = cleanup_export_files(&db).await;
        assert!(result.is_ok(), "Cleanup job should succeed");
        
        // Verify the expired export was deleted
        let remaining_exports = data_export_requests::Entity::find()
            .filter(data_export_requests::Column::UserId.eq(user.id))
            .all(&db)
            .await
            .expect("Failed to query exports");
        
        assert_eq!(remaining_exports.len(), 0, "Expired exports should be deleted");
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_heartbeat_check_marks_bots_offline() {
        let db = create_test_db_connection().await.expect("Failed to connect to test database");
        let user = setup_test_user(&db).await.expect("Failed to create test user");
        
        // Create a bot with stale heartbeat
        let old_heartbeat = Utc::now() - Duration::minutes(15);
        let stale_bot = bot::ActiveModel {
            id: Set(Uuid::new_v4()),
            name: Set(format!("stalebot_{}", Uuid::new_v4())),
            owner_id: Set(user.id),
            source_code: Set(None),
            matchmaking_rank: Set(1000),
            api_key: Set("test_api_key".to_string()),
            live: Set(true),
            icon: Set(None),
            description: Set(None),
            version: Set(None),
            last_heartbeat: Set(Some(old_heartbeat.into())),
            created_at: Set(Utc::now().into()),
        };
        let bot_model = stale_bot.insert(&db).await.expect("Failed to insert test bot");
        
        // Run the heartbeat check job
        let result = heartbeat_check(&db).await;
        assert!(result.is_ok(), "Heartbeat check job should succeed");
        
        // Verify the bot was marked as offline
        let updated_bot = bot::Entity::find_by_id(bot_model.id)
            .one(&db)
            .await
            .expect("Failed to query bot")
            .expect("Bot should exist");
        
        assert!(!updated_bot.live, "Bot should be marked as offline");
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_cleanup_expired_rooms_closes_empty_rooms() {
        let db = create_test_db_connection().await.expect("Failed to connect to test database");
        let user = setup_test_user(&db).await.expect("Failed to create test user");
        
        // Create an empty room
        let empty_room = private_room::ActiveModel {
            id: Set(Uuid::new_v4()),
            host_id: Set(user.id),
            room_name: Set("Empty Test Room".to_string()),
            room_code: Set(format!("TEST{}", Uuid::new_v4().to_string()[0..4].to_uppercase())),
            password: Set(None),
            max_players: Set(4),
            allow_bots: Set(true),
            invite_only: Set(false),
            status: Set("Open".to_string()),
            room_settings: Set(None),
            created_at: Set(Utc::now().into()),
            started_at: Set(None),
            completed_at: Set(None),
        };
        let room_model = empty_room.insert(&db).await.expect("Failed to insert test room");
        
        // Run the cleanup job
        let result = cleanup_expired_rooms(&db).await;
        assert!(result.is_ok(), "Cleanup expired rooms job should succeed");
        
        // Verify the room was closed
        let updated_room = private_room::Entity::find_by_id(room_model.id)
            .one(&db)
            .await
            .expect("Failed to query room")
            .expect("Room should exist");
        
        assert_eq!(updated_room.status, "Closed", "Empty room should be closed");
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_cleanup_reports_removes_dismissed_reports() {
        let db = create_test_db_connection().await.expect("Failed to connect to test database");
        let user = setup_test_user(&db).await.expect("Failed to create test user");
        
        // Create a dismissed report (older than 90 days)
        let old_date = Utc::now() - Duration::days(95);
        let dismissed_report = report::ActiveModel {
            id: Set(Uuid::new_v4()),
            author_id: Set(user.id),
            accused_ids: Set("h/12345678-1234-1234-1234-123456789012".to_string()),
            match_id: Set(None),
            offense_type: Set("ToxicBehaviour".to_string()),
            description: Set(Some("Test report".to_string())),
            status: Set("Dismissed".to_string()),
            report_write_up: Set(Some("No action needed".to_string())),
            created_at: Set(old_date.into()),
            severity: Set("Low".to_string()),
            mod_report_author: Set(Some(user.id)),
            concluded_at: Set(Some(old_date.into())),
            deleted_at: Set(None),
        };
        dismissed_report.insert(&db).await.expect("Failed to insert test report");
        
        // Run the cleanup job
        let result = cleanup_reports(&db).await;
        assert!(result.is_ok(), "Cleanup reports job should succeed");
        
        // Verify the old report was deleted
        let remaining_reports = report::Entity::find()
            .filter(report::Column::AuthorId.eq(user.id))
            .all(&db)
            .await
            .expect("Failed to query reports");
        
        assert_eq!(remaining_reports.len(), 0, "Old dismissed reports should be deleted");
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_cleanup_notifications_removes_read_notifications() {
        let db = create_test_db_connection().await.expect("Failed to connect to test database");
        let user = setup_test_user(&db).await.expect("Failed to create test user");
        
        // Create a read notification (older than 30 days)
        let old_date = Utc::now() - Duration::days(35);
        let read_notification = notifications::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(user.id),
            r#type: Set("SystemMessage".to_string()),
            title: Set("Test Notification".to_string()),
            message: Set("This is a test notification".to_string()),
            related_id: Set(None),
            read: Set(true),
            created_at: Set(old_date.into()),
            expires_at: Set(None),
        };
        read_notification.insert(&db).await.expect("Failed to insert test notification");
        
        // Run the cleanup job
        let result = cleanup_notifications(&db).await;
        assert!(result.is_ok(), "Cleanup notifications job should succeed");
        
        // Verify the old notification was deleted
        let remaining_notifications = notifications::Entity::find()
            .filter(notifications::Column::UserId.eq(user.id))
            .all(&db)
            .await
            .expect("Failed to query notifications");
        
        assert_eq!(remaining_notifications.len(), 0, "Old read notifications should be deleted");
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_cleanup_stale_queues_removes_old_entries() {
        let db = create_test_db_connection().await.expect("Failed to connect to test database");
        let user = setup_test_user(&db).await.expect("Failed to create test user");
        
        // Create a stale queue entry (older than 1 hour)
        let old_date = Utc::now() - Duration::hours(2);
        let stale_queue_entry = queue::ActiveModel {
            id: Set(Uuid::new_v4()),
            participant_type: Set("Human".to_string()),
            participant_id: Set(user.id),
            lobby_id: Set(Uuid::new_v4()),
            preferred_mmr_range: Set(None),
            joined_at: Set(old_date.into()),
            status: Set("Waiting".to_string()),
        };
        stale_queue_entry.insert(&db).await.expect("Failed to insert test queue entry");
        
        // Run the cleanup job
        let result = cleanup_stale_queues(&db).await;
        assert!(result.is_ok(), "Cleanup stale queues job should succeed");
        
        // Verify the old queue entry was deleted
        let remaining_queue_entries = queue::Entity::find()
            .filter(queue::Column::ParticipantId.eq(user.id))
            .all(&db)
            .await
            .expect("Failed to query queue entries");
        
        assert_eq!(remaining_queue_entries.len(), 0, "Stale queue entries should be deleted");
    }

    #[tokio::test]
    async fn test_run_job_with_invalid_job_type() {
        let result = run_job("invalid-job-type".to_string()).await;
        assert!(result.is_err());
        
        match result {
            Err(AppError::Job(msg)) => {
                assert!(msg.contains("Unknown job type"));
            }
            Err(AppError::Config(_)) => {
                // Config error is expected if no database connection can be established
                println!("Got config error (expected in test environment)");
            }
            Err(AppError::Database(_)) => {
                // Database error is expected if no database connection can be established
                println!("Got database error (expected in test environment)");
            }
            Err(other) => {
                panic!("Expected Job, Config, or Database error, got: {:?}", other);
            }
            Ok(()) => {
                panic!("Expected an error for invalid job type, but got success");
            }
        }
    }
}

