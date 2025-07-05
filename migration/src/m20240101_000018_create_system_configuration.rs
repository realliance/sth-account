use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(SystemConfiguration::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SystemConfiguration::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(SystemConfiguration::Key)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(SystemConfiguration::Value)
                            .json()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SystemConfiguration::Description)
                            .text()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(SystemConfiguration::UpdatedBy)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SystemConfiguration::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_system_configuration_updated_by")
                            .from(SystemConfiguration::Table, SystemConfiguration::UpdatedBy)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_system_configuration_key")
                    .table(SystemConfiguration::Table)
                    .col(SystemConfiguration::Key)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_system_configuration_updated_at")
                    .table(SystemConfiguration::Table)
                    .col(SystemConfiguration::UpdatedAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(SystemConfiguration::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum SystemConfiguration {
    Table,
    Id,
    Key,
    Value,
    Description,
    UpdatedBy,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum User {
    Table,
    Id,
}