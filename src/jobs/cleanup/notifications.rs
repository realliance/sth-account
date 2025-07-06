use crate::error::Result;
use chrono::{Duration, Utc};
use sea_orm::{ColumnTrait, DeleteResult, EntityTrait, QueryFilter};
use tracing::info;

pub async fn run(db: &sea_orm::DatabaseConnection) -> Result<()> {
    info!("Running cleanup-notifications job");

    let cutoff_date = Utc::now() - Duration::days(30);

    let result: DeleteResult = entity::notifications::Entity::delete_many()
        .filter(
            entity::notifications::Column::Read
                .eq(true)
                .and(entity::notifications::Column::CreatedAt.lt(cutoff_date)),
        )
        .exec(db)
        .await?;

    info!(
        "Cleaned up {} read notifications older than 30 days",
        result.rows_affected
    );
    Ok(())
}
