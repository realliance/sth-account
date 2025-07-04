use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(RoomInvitation::Table)
                    .if_not_exists()
                    .col(uuid(RoomInvitation::Id).primary_key())
                    .col(uuid(RoomInvitation::RoomId))
                    .col(uuid(RoomInvitation::InviterId))
                    .col(uuid(RoomInvitation::InviteeId))
                    .col(string(RoomInvitation::Status).default("Pending"))
                    .col(timestamp_with_time_zone(RoomInvitation::CreatedAt))
                    .col(timestamp_with_time_zone_null(RoomInvitation::RespondedAt))
                    .col(timestamp_with_time_zone_null(RoomInvitation::ExpiresAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_room_invitation_room")
                            .from(RoomInvitation::Table, RoomInvitation::RoomId)
                            .to(PrivateRoom::Table, PrivateRoom::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_room_invitation_inviter")
                            .from(RoomInvitation::Table, RoomInvitation::InviterId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_room_invitation_invitee")
                            .from(RoomInvitation::Table, RoomInvitation::InviteeId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(RoomInvitation::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum RoomInvitation {
    Table,
    Id,
    RoomId,
    InviterId,
    InviteeId,
    Status,
    CreatedAt,
    RespondedAt,
    ExpiresAt,
}

#[derive(DeriveIden)]
enum PrivateRoom {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum User {
    Table,
    Id,
}
