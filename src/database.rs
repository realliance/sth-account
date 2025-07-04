use crate::{config::Config, error::Result};
use sea_orm::{Database, DatabaseConnection};

pub async fn establish_connection(config: &Config) -> Result<DatabaseConnection> {
    let db = Database::connect(&config.database_url).await?;
    Ok(db)
}
