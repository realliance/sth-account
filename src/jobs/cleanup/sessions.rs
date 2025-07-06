use crate::error::Result;
use chrono::{Duration, Utc};
use sea_orm::{ColumnTrait, DeleteResult, EntityTrait, QueryFilter};
use tracing::info;

pub async fn run(db: &sea_orm::DatabaseConnection) -> Result<()> {
    info!("Running cleanup-sessions job");

    let cutoff_date = Utc::now() - Duration::days(30);

    // Delete expired sessions older than 30 days
    let result: DeleteResult = entity::user_session::Entity::delete_many()
        .filter(
            entity::user_session::Column::Status
                .eq("Expired")
                .and(entity::user_session::Column::CreatedAt.lt(cutoff_date)),
        )
        .exec(db)
        .await?;

    info!(
        "Cleaned up {} expired session entries older than 30 days",
        result.rows_affected
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use entity::{user, user_session};
    use sea_orm::{
        ActiveModelTrait, ColumnTrait, Database, DatabaseConnection, EntityTrait, QueryFilter, Set,
    };
    use uuid::Uuid;

    async fn create_test_db_connection()
    -> std::result::Result<DatabaseConnection, Box<dyn std::error::Error>> {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://localhost/sth_account_test".to_string());

        let db = Database::connect(&database_url).await?;
        Ok(db)
    }

    async fn setup_test_user(
        db: &DatabaseConnection,
    ) -> std::result::Result<user::Model, Box<dyn std::error::Error>> {
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
    async fn test_cleanup_sessions_removes_expired_sessions() {
        let db = create_test_db_connection()
            .await
            .expect("Failed to connect to test database");
        let user = setup_test_user(&db)
            .await
            .expect("Failed to create test user");

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
            data: Set(None),
        };
        expired_session
            .insert(&db)
            .await
            .expect("Failed to insert test session");

        // Run the cleanup job
        let result = run(&db).await;
        assert!(result.is_ok(), "Cleanup job should succeed");

        // Verify the expired session was deleted
        let remaining_sessions = user_session::Entity::find()
            .filter(user_session::Column::UserId.eq(user.id))
            .all(&db)
            .await
            .expect("Failed to query sessions");

        assert_eq!(
            remaining_sessions.len(),
            0,
            "Expired sessions should be deleted"
        );
    }
}
