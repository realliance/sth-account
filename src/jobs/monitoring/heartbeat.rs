use crate::error::Result;
use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use tracing::info;

pub async fn run(db: &sea_orm::DatabaseConnection) -> Result<()> {
    info!("Running heartbeat-check job");

    let heartbeat_cutoff = Utc::now() - Duration::minutes(10);

    // Find bots that haven't sent a heartbeat in 10 minutes and are still marked as live
    let stale_bots = entity::bot::Entity::find()
        .filter(
            entity::bot::Column::Live.eq(true).and(
                entity::bot::Column::LastHeartbeat
                    .is_null()
                    .or(entity::bot::Column::LastHeartbeat.lt(heartbeat_cutoff)),
            ),
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

    info!(
        "Marked {} bots as offline due to missing heartbeat",
        updated_count
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use entity::{bot, user};
    use sea_orm::{ActiveModelTrait, Database, DatabaseConnection, EntityTrait, Set};
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
    async fn test_heartbeat_check_marks_bots_offline() {
        let db = create_test_db_connection()
            .await
            .expect("Failed to connect to test database");
        let user = setup_test_user(&db)
            .await
            .expect("Failed to create test user");

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
        let bot_model = stale_bot
            .insert(&db)
            .await
            .expect("Failed to insert test bot");

        // Run the heartbeat check job
        let result = run(&db).await;
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
    async fn test_heartbeat_check_leaves_recent_bots_online() {
        let db = create_test_db_connection()
            .await
            .expect("Failed to connect to test database");
        let user = setup_test_user(&db)
            .await
            .expect("Failed to create test user");

        // Create a bot with recent heartbeat
        let recent_heartbeat = Utc::now() - Duration::minutes(5);
        let active_bot = bot::ActiveModel {
            id: Set(Uuid::new_v4()),
            name: Set(format!("activebot_{}", Uuid::new_v4())),
            owner_id: Set(user.id),
            source_code: Set(None),
            matchmaking_rank: Set(1000),
            api_key: Set("test_api_key".to_string()),
            live: Set(true),
            icon: Set(None),
            description: Set(None),
            version: Set(None),
            last_heartbeat: Set(Some(recent_heartbeat.into())),
            created_at: Set(Utc::now().into()),
        };
        let bot_model = active_bot
            .insert(&db)
            .await
            .expect("Failed to insert test bot");

        // Run the heartbeat check job
        let result = run(&db).await;
        assert!(result.is_ok(), "Heartbeat check job should succeed");

        // Verify the bot is still online
        let updated_bot = bot::Entity::find_by_id(bot_model.id)
            .one(&db)
            .await
            .expect("Failed to query bot")
            .expect("Bot should exist");

        assert!(updated_bot.live, "Bot should still be marked as online");
    }
}
