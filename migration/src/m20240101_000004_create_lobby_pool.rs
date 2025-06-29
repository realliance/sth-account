use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(LobbyPool::Table)
                    .if_not_exists()
                    .col(uuid(LobbyPool::Id).primary_key())
                    .col(string(LobbyPool::Name))
                    .col(text_null(LobbyPool::Description))
                    .col(string(LobbyPool::Preset).default("GeneralFourPlayer"))
                    .col(boolean(LobbyPool::Active).default(true))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(LobbyPool::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum LobbyPool {
    Table,
    Id,
    Name,
    Description,
    Preset,
    Active,
}