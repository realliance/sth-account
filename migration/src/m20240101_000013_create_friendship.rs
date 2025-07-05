use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Friendship::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Friendship::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Friendship::RequesterId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Friendship::AddresseeId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Friendship::Status)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Friendship::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(Friendship::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_friendship_requester_id")
                            .from(Friendship::Table, Friendship::RequesterId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_friendship_addressee_id")
                            .from(Friendship::Table, Friendship::AddresseeId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_friendship_requester_id")
                    .table(Friendship::Table)
                    .col(Friendship::RequesterId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_friendship_addressee_id")
                    .table(Friendship::Table)
                    .col(Friendship::AddresseeId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_friendship_status")
                    .table(Friendship::Table)
                    .col(Friendship::Status)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_friendship_unique_pair")
                    .table(Friendship::Table)
                    .col(Friendship::RequesterId)
                    .col(Friendship::AddresseeId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Friendship::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Friendship {
    Table,
    Id,
    RequesterId,
    AddresseeId,
    Status,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum User {
    Table,
    Id,
}