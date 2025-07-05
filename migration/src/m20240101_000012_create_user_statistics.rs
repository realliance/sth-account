use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(UserStatistics::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UserStatistics::UserId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(UserStatistics::TotalGames)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(UserStatistics::Wins)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(UserStatistics::SecondPlace)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(UserStatistics::ThirdPlace)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(UserStatistics::FourthPlace)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(UserStatistics::AverageScore)
                            .decimal_len(10, 2)
                            .null(),
                    )
                    .col(
                        ColumnDef::new(UserStatistics::PeakMmr)
                            .integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(UserStatistics::CurrentStreak)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(UserStatistics::LastGameAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_statistics_user_id")
                            .from(UserStatistics::Table, UserStatistics::UserId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_user_statistics_total_games")
                    .table(UserStatistics::Table)
                    .col(UserStatistics::TotalGames)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_user_statistics_peak_mmr")
                    .table(UserStatistics::Table)
                    .col(UserStatistics::PeakMmr)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UserStatistics::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum UserStatistics {
    Table,
    UserId,
    TotalGames,
    Wins,
    SecondPlace,
    ThirdPlace,
    FourthPlace,
    AverageScore,
    PeakMmr,
    CurrentStreak,
    LastGameAt,
}

#[derive(DeriveIden)]
enum User {
    Table,
    Id,
}