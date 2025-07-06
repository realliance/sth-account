use crate::error::Result;
use chrono::{Duration, Utc};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use tracing::{info, warn, error};
use std::fs;
use std::path::Path;

pub async fn run(db: &sea_orm::DatabaseConnection) -> Result<()> {
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
        // Delete the actual file from storage
        if let Some(file_path) = &export.file_path {
            if let Err(e) = delete_export_file(file_path) {
                error!("Failed to delete export file {}: {}", file_path, e);
                // Continue processing other files even if one fails
            }
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

fn delete_export_file(file_path: &str) -> std::io::Result<()> {
    let path = Path::new(file_path);
    
    if path.exists() {
        if path.is_file() {
            fs::remove_file(path)?;
            info!("Deleted export file: {}", file_path);
        } else {
            warn!("Export file path is not a file: {}", file_path);
        }
    } else {
        warn!("Export file does not exist: {}", file_path);
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use sea_orm::{Database, DatabaseConnection, EntityTrait, ActiveModelTrait, Set, ColumnTrait, QueryFilter};
    use uuid::Uuid;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;
    use entity::{data_export_requests, user};

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
    async fn test_cleanup_export_files_removes_expired_exports() {
        let db = create_test_db_connection().await.expect("Failed to connect to test database");
        let user = setup_test_user(&db).await.expect("Failed to create test user");
        
        // Create a temporary file for testing
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let temp_file_path = temp_dir.path().join("test_export.json");
        let mut temp_file = File::create(&temp_file_path).expect("Failed to create temp file");
        writeln!(temp_file, r#"{{"test": "data"}}"#).expect("Failed to write to temp file");
        
        // Create an expired export request
        let old_date = Utc::now() - Duration::days(10);
        let expired_export = data_export_requests::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(user.id),
            export_type: Set("UserData".to_string()),
            status: Set("Completed".to_string()),
            file_path: Set(Some(temp_file_path.to_string_lossy().to_string())),
            requested_at: Set(old_date.into()),
            completed_at: Set(Some(old_date.into())),
            expires_at: Set(Some(old_date.into())),
        };
        expired_export.insert(&db).await.expect("Failed to insert test export");
        
        // Verify the file exists before cleanup
        assert!(temp_file_path.exists(), "Test file should exist before cleanup");
        
        // Run the cleanup job
        let result = run(&db).await;
        assert!(result.is_ok(), "Cleanup job should succeed");
        
        // Verify the expired export was deleted from database
        let remaining_exports = data_export_requests::Entity::find()
            .filter(data_export_requests::Column::UserId.eq(user.id))
            .all(&db)
            .await
            .expect("Failed to query exports");
        
        assert_eq!(remaining_exports.len(), 0, "Expired exports should be deleted");
        
        // Verify the file was deleted from filesystem
        assert!(!temp_file_path.exists(), "Export file should be deleted from filesystem");
    }

    #[test]
    fn test_delete_export_file_success() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let temp_file_path = temp_dir.path().join("test_file.txt");
        
        // Create a test file
        let mut file = File::create(&temp_file_path).expect("Failed to create test file");
        writeln!(file, "test content").expect("Failed to write to temp file");
        
        // Verify file exists
        assert!(temp_file_path.exists());
        
        // Delete the file
        let result = delete_export_file(&temp_file_path.to_string_lossy());
        assert!(result.is_ok(), "File deletion should succeed");
        
        // Verify file is deleted
        assert!(!temp_file_path.exists());
    }

    #[test]
    fn test_delete_export_file_nonexistent() {
        let nonexistent_path = "/tmp/nonexistent_file_12345.txt";
        
        // This should not panic or return an error
        let result = delete_export_file(nonexistent_path);
        assert!(result.is_ok(), "Deleting nonexistent file should not error");
    }
}