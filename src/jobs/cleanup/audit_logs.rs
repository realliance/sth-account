use crate::error::Result;
use chrono::{Duration, Utc};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, DeleteResult};
use tracing::info;

pub async fn run(db: &sea_orm::DatabaseConnection) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use sea_orm::{Database, DatabaseConnection, EntityTrait, ActiveModelTrait, Set};
    use uuid::Uuid;
    use entity::{audit_log, user};

    async fn create_test_db_connection() -> std::result::Result<DatabaseConnection, Box<dyn std::error::Error>> {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://localhost/sth_account_test".to_string());
        
        let db = Database::connect(&database_url).await?;
        Ok(db)
    }

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
        let result = run(&db).await;
        assert!(result.is_ok(), "Cleanup job should succeed");
        
        // Verify the old entry was deleted
        let remaining_logs = audit_log::Entity::find()
            .filter(audit_log::Column::UserId.eq(user.id))
            .all(&db)
            .await
            .expect("Failed to query audit logs");
        
        assert_eq!(remaining_logs.len(), 0, "Old audit logs should be deleted");
    }
}