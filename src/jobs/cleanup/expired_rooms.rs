use crate::error::Result;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, ActiveModelTrait, Set, PaginatorTrait};
use tracing::info;

pub async fn run(db: &sea_orm::DatabaseConnection) -> Result<()> {
    info!("Running cleanup-expired-rooms job");
    
    // Find rooms that are open but have no active participants
    let empty_rooms = entity::private_room::Entity::find()
        .filter(entity::private_room::Column::Status.eq("Open"))
        .all(db)
        .await?;
    
    let mut closed_count = 0;
    
    for room in empty_rooms {
        // Check if room has any active participants
        let active_participants = entity::room_participants::Entity::find()
            .filter(
                entity::room_participants::Column::RoomId.eq(room.id)
                    .and(entity::room_participants::Column::LeftAt.is_null())
            )
            .count(db)
            .await?;
        
        // If no active participants, close the room
        if active_participants == 0 {
            let mut room_active: entity::private_room::ActiveModel = room.into();
            room_active.status = Set("Closed".to_string());
            room_active.update(db).await?;
            closed_count += 1;
        }
    }
    
    info!("Closed {} empty private rooms", closed_count);
    Ok(())
}