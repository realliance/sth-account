use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(DataExportRequests::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(DataExportRequests::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(DataExportRequests::UserId).uuid().not_null())
                    .col(
                        ColumnDef::new(DataExportRequests::ExportType)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(DataExportRequests::Status)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(DataExportRequests::FilePath).string().null())
                    .col(
                        ColumnDef::new(DataExportRequests::RequestedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(DataExportRequests::CompletedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(DataExportRequests::ExpiresAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_data_export_requests_user_id")
                            .from(DataExportRequests::Table, DataExportRequests::UserId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_data_export_requests_user_id")
                    .table(DataExportRequests::Table)
                    .col(DataExportRequests::UserId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_data_export_requests_status")
                    .table(DataExportRequests::Table)
                    .col(DataExportRequests::Status)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_data_export_requests_expires_at")
                    .table(DataExportRequests::Table)
                    .col(DataExportRequests::ExpiresAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(DataExportRequests::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum DataExportRequests {
    Table,
    Id,
    UserId,
    ExportType,
    Status,
    FilePath,
    RequestedAt,
    CompletedAt,
    ExpiresAt,
}

#[derive(DeriveIden)]
enum User {
    Table,
    Id,
}
