pub use sea_orm_migration::prelude::*;

mod m20220101_000001_create_table;
mod m20240101_000002_create_bot;
mod m20240101_000003_create_game_history;
mod m20240101_000004_create_lobby_pool;
mod m20240101_000005_create_match;
mod m20240101_000006_create_queue;
mod m20240101_000007_create_user_session;
mod m20240101_000008_create_report;
mod m20240101_000009_create_private_room;
mod m20240101_000010_create_room_invitation;
mod m20240101_000011_create_room_participants;
mod m20240101_000012_create_user_statistics;
mod m20240101_000013_create_friendship;
mod m20240101_000014_create_notifications;
mod m20240101_000015_create_audit_log;
mod m20240101_000016_create_data_export_requests;
mod m20240101_000017_create_bot_statistics;
mod m20240101_000018_create_system_configuration;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_create_table::Migration),
            Box::new(m20240101_000002_create_bot::Migration),
            Box::new(m20240101_000003_create_game_history::Migration),
            Box::new(m20240101_000004_create_lobby_pool::Migration),
            Box::new(m20240101_000005_create_match::Migration),
            Box::new(m20240101_000006_create_queue::Migration),
            Box::new(m20240101_000007_create_user_session::Migration),
            Box::new(m20240101_000008_create_report::Migration),
            Box::new(m20240101_000009_create_private_room::Migration),
            Box::new(m20240101_000010_create_room_invitation::Migration),
            Box::new(m20240101_000011_create_room_participants::Migration),
            Box::new(m20240101_000012_create_user_statistics::Migration),
            Box::new(m20240101_000013_create_friendship::Migration),
            Box::new(m20240101_000014_create_notifications::Migration),
            Box::new(m20240101_000015_create_audit_log::Migration),
            Box::new(m20240101_000016_create_data_export_requests::Migration),
            Box::new(m20240101_000017_create_bot_statistics::Migration),
            Box::new(m20240101_000018_create_system_configuration::Migration),
        ]
    }
}

use sea_orm::DatabaseConnection;

#[derive(Debug)]
pub enum MigrationCommand {
    Up,
    Down,
    Status,
    Fresh,
    Generate { name: String },
}

pub async fn run_migration(command: MigrationCommand) -> Result<(), sea_orm::DbErr> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://localhost/sth_account".to_string());

    let db: DatabaseConnection = sea_orm::Database::connect(&database_url).await?;

    match command {
        MigrationCommand::Up => {
            Migrator::up(&db, None).await?;
            println!("All migrations applied successfully");
        }
        MigrationCommand::Down => {
            Migrator::down(&db, None).await?;
            println!("Last migration rolled back successfully");
        }
        MigrationCommand::Status => {
            println!("Migration status checking...");
            // For now, just check if we can connect to the database
            println!("Database connection: OK");
            println!("Use 'up' to apply all pending migrations");
        }
        MigrationCommand::Fresh => {
            Migrator::fresh(&db).await?;
            println!("Database reset and all migrations applied successfully");
        }
        MigrationCommand::Generate { name } => {
            println!("Migration generation should be done using:");
            println!("  sea-orm-cli migrate generate {}", name);
            println!("Run this command in the migration/ directory");
        }
    }

    Ok(())
}
