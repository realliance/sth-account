use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(RoomParticipants::Table)
                    .if_not_exists()
                    .col(uuid(RoomParticipants::Id).primary_key())
                    .col(uuid(RoomParticipants::RoomId))
                    .col(string(RoomParticipants::ParticipantType))
                    .col(uuid(RoomParticipants::ParticipantId))
                    .col(string(RoomParticipants::Status).default("Joined"))
                    .col(timestamp_with_time_zone(RoomParticipants::JoinedAt))
                    .col(timestamp_with_time_zone_null(RoomParticipants::LeftAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_room_participants_room")
                            .from(RoomParticipants::Table, RoomParticipants::RoomId)
                            .to(PrivateRoom::Table, PrivateRoom::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(RoomParticipants::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum RoomParticipants {
    Table,
    Id,
    RoomId,
    ParticipantType,
    ParticipantId,
    Status,
    JoinedAt,
    LeftAt,
}

#[derive(DeriveIden)]
enum PrivateRoom {
    Table,
    Id,
}
