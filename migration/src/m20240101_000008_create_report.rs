use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Report::Table)
                    .if_not_exists()
                    .col(uuid(Report::Id).primary_key())
                    .col(uuid(Report::AuthorId))
                    .col(string(Report::AccusedIds))
                    .col(uuid_null(Report::MatchId))
                    .col(string(Report::OffenseType))
                    .col(text_null(Report::Description))
                    .col(string(Report::Status).default("Active"))
                    .col(text_null(Report::ReportWriteUp))
                    .col(timestamp_with_time_zone(Report::CreatedAt))
                    .col(string(Report::Severity).default("Medium"))
                    .col(uuid_null(Report::ModReportAuthor))
                    .col(timestamp_with_time_zone_null(Report::ConcludedAt))
                    .col(timestamp_with_time_zone_null(Report::DeletedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_report_author")
                            .from(Report::Table, Report::AuthorId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_report_match")
                            .from(Report::Table, Report::MatchId)
                            .to(Match::Table, Match::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_report_moderator")
                            .from(Report::Table, Report::ModReportAuthor)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        // Create indexes
        manager
            .create_index(
                Index::create()
                    .name("idx_report_author_id")
                    .table(Report::Table)
                    .col(Report::AuthorId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_report_status")
                    .table(Report::Table)
                    .col(Report::Status)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_report_severity")
                    .table(Report::Table)
                    .col(Report::Severity)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_report_created_at")
                    .table(Report::Table)
                    .col(Report::CreatedAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Report::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Report {
    Table,
    Id,
    AuthorId,
    AccusedIds,
    MatchId,
    OffenseType,
    Description,
    Status,
    ReportWriteUp,
    CreatedAt,
    Severity,
    ModReportAuthor,
    ConcludedAt,
    DeletedAt,
}

#[derive(DeriveIden)]
enum User {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Match {
    Table,
    Id,
}
