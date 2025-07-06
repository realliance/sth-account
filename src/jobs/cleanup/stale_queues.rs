use crate::error::Result;
use chrono::{Duration, Utc};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, DeleteResult};
use tracing::info;

pub async fn run(db: &sea_orm::DatabaseConnection) -> Result<()> {
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