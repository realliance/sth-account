use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(GameHistory::Table)
                    .if_not_exists()
                    .col(uuid(GameHistory::Id).primary_key())
                    .col(binary(GameHistory::HistoryBlob))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(GameHistory::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum GameHistory {
    Table,
    Id,
    HistoryBlob,
}