use crate::error::Result;
use chrono::{Duration, Utc};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, DeleteResult};
use tracing::info;

pub async fn run(db: &sea_orm::DatabaseConnection) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use sea_orm::{Database, DatabaseConnection, EntityTrait, ActiveModelTrait, Set, ColumnTrait, QueryFilter};
    use uuid::Uuid;
    use entity::{report, user};

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
            write_up: Set(Some("No action needed".to_string())),
            created_at: Set(old_date.into()),
            severity: Set("Low".to_string()),
            mod_report_author: Set(Some(user.id)),
            concluded_at: Set(Some(old_date.into())),
            deleted_at: Set(None),
        };
        dismissed_report.insert(&db).await.expect("Failed to insert test report");
        
        // Run the cleanup job
        let result = run(&db).await;
        assert!(result.is_ok(), "Cleanup reports job should succeed");
        
        // Verify the old report was deleted
        let remaining_reports = report::Entity::find()
            .filter(report::Column::AuthorId.eq(user.id))
            .all(&db)
            .await
            .expect("Failed to query reports");
        
        assert_eq!(remaining_reports.len(), 0, "Old dismissed reports should be deleted");
    }
}