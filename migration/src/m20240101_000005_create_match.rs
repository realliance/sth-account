use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Match::Table)
                    .if_not_exists()
                    .col(uuid(Match::Id).primary_key())
                    .col(uuid(Match::LobbyId))
                    .col(uuid(Match::GameHistoryId))
                    .col(timestamp_with_time_zone(Match::StartedAt))
                    .col(timestamp_with_time_zone_null(Match::CompletedAt))
                    // Participant 1
                    .col(string(Match::Participant1Type))
                    .col(uuid(Match::Participant1Id))
                    .col(integer_null(Match::Participant1Score))
                    .col(integer_null(Match::Participant1MmrDelta))
                    // Participant 2
                    .col(string(Match::Participant2Type))
                    .col(uuid(Match::Participant2Id))
                    .col(integer_null(Match::Participant2Score))
                    .col(integer_null(Match::Participant2MmrDelta))
                    // Participant 3
                    .col(string(Match::Participant3Type))
                    .col(uuid(Match::Participant3Id))
                    .col(integer_null(Match::Participant3Score))
                    .col(integer_null(Match::Participant3MmrDelta))
                    // Participant 4 (optional)
                    .col(string_null(Match::Participant4Type))
                    .col(uuid_null(Match::Participant4Id))
                    .col(integer_null(Match::Participant4Score))
                    .col(integer_null(Match::Participant4MmrDelta))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_match_lobby")
                            .from(Match::Table, Match::LobbyId)
                            .to(LobbyPool::Table, LobbyPool::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_match_game_history")
                            .from(Match::Table, Match::GameHistoryId)
                            .to(GameHistory::Table, GameHistory::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;

        // Create indexes
        manager
            .create_index(
                Index::create()
                    .name("idx_match_lobby_id")
                    .table(Match::Table)
                    .col(Match::LobbyId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_match_started_at")
                    .table(Match::Table)
                    .col(Match::StartedAt)
                    .to_owned(),
            )
            .await?;

        // Indexes for participants
        manager
            .create_index(
                Index::create()
                    .name("idx_match_participant1_id")
                    .table(Match::Table)
                    .col(Match::Participant1Id)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_match_participant2_id")
                    .table(Match::Table)
                    .col(Match::Participant2Id)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_match_participant3_id")
                    .table(Match::Table)
                    .col(Match::Participant3Id)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_match_participant4_id")
                    .table(Match::Table)
                    .col(Match::Participant4Id)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Match::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Match {
    Table,
    Id,
    LobbyId,
    GameHistoryId,
    StartedAt,
    CompletedAt,
    Participant1Type,
    Participant1Id,
    Participant1Score,
    Participant1MmrDelta,
    Participant2Type,
    Participant2Id,
    Participant2Score,
    Participant2MmrDelta,
    Participant3Type,
    Participant3Id,
    Participant3Score,
    Participant3MmrDelta,
    Participant4Type,
    Participant4Id,
    Participant4Score,
    Participant4MmrDelta,
}

#[derive(DeriveIden)]
enum LobbyPool {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum GameHistory {
    Table,
    Id,
}
