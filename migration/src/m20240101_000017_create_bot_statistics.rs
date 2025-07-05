use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(BotStatistics::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(BotStatistics::BotId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(BotStatistics::TotalGames)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(BotStatistics::Wins)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(BotStatistics::SecondPlace)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(BotStatistics::ThirdPlace)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(BotStatistics::FourthPlace)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(BotStatistics::AverageScore)
                            .decimal_len(10, 2)
                            .null(),
                    )
                    .col(
                        ColumnDef::new(BotStatistics::PeakMmr)
                            .integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(BotStatistics::CurrentStreak)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(BotStatistics::LastGameAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(BotStatistics::UptimePercentage)
                            .decimal_len(5, 2)
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_bot_statistics_bot_id")
                            .from(BotStatistics::Table, BotStatistics::BotId)
                            .to(Bot::Table, Bot::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_bot_statistics_total_games")
                    .table(BotStatistics::Table)
                    .col(BotStatistics::TotalGames)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_bot_statistics_peak_mmr")
                    .table(BotStatistics::Table)
                    .col(BotStatistics::PeakMmr)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(BotStatistics::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum BotStatistics {
    Table,
    BotId,
    TotalGames,
    Wins,
    SecondPlace,
    ThirdPlace,
    FourthPlace,
    AverageScore,
    PeakMmr,
    CurrentStreak,
    LastGameAt,
    UptimePercentage,
}

#[derive(DeriveIden)]
enum Bot {
    Table,
    Id,
}