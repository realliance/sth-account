use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Queue::Table)
                    .if_not_exists()
                    .col(uuid(Queue::Id).primary_key())
                    .col(string(Queue::ParticipantType))
                    .col(uuid(Queue::ParticipantId))
                    .col(uuid(Queue::LobbyId))
                    .col(string_null(Queue::PreferredMmrRange))
                    .col(timestamp_with_time_zone(Queue::JoinedAt))
                    .col(string(Queue::Status).default("Waiting"))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_queue_lobby")
                            .from(Queue::Table, Queue::LobbyId)
                            .to(LobbyPool::Table, LobbyPool::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create indexes
        manager
            .create_index(
                Index::create()
                    .name("idx_queue_lobby_id")
                    .table(Queue::Table)
                    .col(Queue::LobbyId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_queue_status")
                    .table(Queue::Table)
                    .col(Queue::Status)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_queue_joined_at")
                    .table(Queue::Table)
                    .col(Queue::JoinedAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Queue::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Queue {
    Table,
    Id,
    ParticipantType,
    ParticipantId,
    LobbyId,
    PreferredMmrRange,
    JoinedAt,
    Status,
}

#[derive(DeriveIden)]
enum LobbyPool {
    Table,
    Id,
}
