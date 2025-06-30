use sea_orm::{Database, DatabaseConnection};
use crate::{config::Config, error::Result};

pub async fn establish_connection(config: &Config) -> Result<DatabaseConnection> {
    let db = Database::connect(&config.database_url).await?;
    Ok(db)
}