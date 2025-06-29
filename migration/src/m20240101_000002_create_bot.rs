use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Bot::Table)
                    .if_not_exists()
                    .col(uuid(Bot::Id).primary_key())
                    .col(string(Bot::Name))
                    .col(uuid(Bot::OwnerId))
                    .col(string_null(Bot::SourceCode))
                    .col(integer(Bot::MatchmakingRank).default(1500))
                    .col(string(Bot::ApiKey))
                    .col(boolean(Bot::Live).default(false))
                    .col(string_null(Bot::Icon))
                    .col(text_null(Bot::Description))
                    .col(string_null(Bot::Version))
                    .col(timestamp_with_time_zone_null(Bot::LastHeartbeat))
                    .col(timestamp_with_time_zone(Bot::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_bot_owner")
                            .from(Bot::Table, Bot::OwnerId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                    )
                    .to_owned(),
            )
            .await?;

        // Create indexes
        manager
            .create_index(
                Index::create()
                    .name("idx_bot_owner_id")
                    .table(Bot::Table)
                    .col(Bot::OwnerId)
                    .to_owned(),
            )
            .await?;
            
        manager
            .create_index(
                Index::create()
                    .name("idx_bot_live")
                    .table(Bot::Table)
                    .col(Bot::Live)
                    .to_owned(),
            )
            .await?;
            
        manager
            .create_index(
                Index::create()
                    .name("idx_bot_mmr")
                    .table(Bot::Table)
                    .col(Bot::MatchmakingRank)
                    .to_owned(),
            )
            .await

    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Bot::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Bot {
    Table,
    Id,
    Name,
    OwnerId,
    SourceCode,
    MatchmakingRank,
    ApiKey,
    Live,
    Icon,
    Description,
    Version,
    LastHeartbeat,
    CreatedAt,
}

#[derive(DeriveIden)]
enum User {
    Table,
    Id,
}