use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(PrivateRoom::Table)
                    .if_not_exists()
                    .col(uuid(PrivateRoom::Id).primary_key())
                    .col(uuid(PrivateRoom::HostId))
                    .col(string(PrivateRoom::RoomName))
                    .col(string(PrivateRoom::RoomCode).unique_key())
                    .col(string_null(PrivateRoom::Password))
                    .col(integer(PrivateRoom::MaxPlayers).default(4))
                    .col(boolean(PrivateRoom::AllowBots).default(false))
                    .col(boolean(PrivateRoom::InviteOnly).default(false))
                    .col(string(PrivateRoom::Status).default("Waiting"))
                    .col(json_null(PrivateRoom::RoomSettings))
                    .col(timestamp_with_time_zone(PrivateRoom::CreatedAt))
                    .col(timestamp_with_time_zone_null(PrivateRoom::StartedAt))
                    .col(timestamp_with_time_zone_null(PrivateRoom::CompletedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_private_room_host")
                            .from(PrivateRoom::Table, PrivateRoom::HostId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(PrivateRoom::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum PrivateRoom {
    Table,
    Id,
    HostId,
    RoomName,
    RoomCode,
    Password,
    MaxPlayers,
    AllowBots,
    InviteOnly,
    Status,
    RoomSettings,
    CreatedAt,
    StartedAt,
    CompletedAt,
}

#[derive(DeriveIden)]
enum User {
    Table,
    Id,
}
