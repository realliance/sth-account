use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(UserSession::Table)
                    .if_not_exists()
                    .col(uuid(UserSession::Id).primary_key())
                    .col(uuid(UserSession::UserId))
                    .col(string(UserSession::TokenHash))
                    .col(string_null(UserSession::DeviceInfo))
                    .col(string(UserSession::IpAddress))
                    .col(timestamp_with_time_zone(UserSession::CreatedAt))
                    .col(timestamp_with_time_zone(UserSession::ExpiresAt))
                    .col(timestamp_with_time_zone_null(UserSession::LastActiveAt))
                    .col(string(UserSession::Status).default("Active"))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_session_user")
                            .from(UserSession::Table, UserSession::UserId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create indexes
        manager
            .create_index(
                Index::create()
                    .name("idx_session_user_id")
                    .table(UserSession::Table)
                    .col(UserSession::UserId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_session_status")
                    .table(UserSession::Table)
                    .col(UserSession::Status)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_session_expires_at")
                    .table(UserSession::Table)
                    .col(UserSession::ExpiresAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UserSession::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum UserSession {
    Table,
    Id,
    UserId,
    TokenHash,
    DeviceInfo,
    IpAddress,
    CreatedAt,
    ExpiresAt,
    LastActiveAt,
    Status,
}

#[derive(DeriveIden)]
enum User {
    Table,
    Id,
}
