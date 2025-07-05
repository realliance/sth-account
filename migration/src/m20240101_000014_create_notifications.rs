use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Notifications::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Notifications::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Notifications::UserId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Notifications::Type)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Notifications::Title)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Notifications::Message)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Notifications::RelatedId)
                            .uuid()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(Notifications::Read)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Notifications::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(Notifications::ExpiresAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_notifications_user_id")
                            .from(Notifications::Table, Notifications::UserId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_notifications_user_id")
                    .table(Notifications::Table)
                    .col(Notifications::UserId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_notifications_read")
                    .table(Notifications::Table)
                    .col(Notifications::Read)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_notifications_created_at")
                    .table(Notifications::Table)
                    .col(Notifications::CreatedAt)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_notifications_type")
                    .table(Notifications::Table)
                    .col(Notifications::Type)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Notifications::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Notifications {
    Table,
    Id,
    UserId,
    Type,
    Title,
    Message,
    RelatedId,
    Read,
    CreatedAt,
    ExpiresAt,
}

#[derive(DeriveIden)]
enum User {
    Table,
    Id,
}