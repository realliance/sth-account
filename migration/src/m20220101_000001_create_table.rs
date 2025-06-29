use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(User::Table)
                    .if_not_exists()
                    .col(uuid(User::Id).primary_key())
                    .col(string_uniq(User::Username))
                    .col(string(User::Country))
                    .col(string_null(User::FavoriteTile))
                    .col(string_null(User::Pronouns))
                    .col(integer(User::MatchmakingRank).default(1500))
                    .col(string(User::Password))
                    .col(timestamp_with_time_zone(User::CreatedAt))
                    .col(json_null(User::Passkey))
                    .col(string(User::Role).default("Active"))
                    .col(string_null(User::Email))
                    .col(timestamp_with_time_zone_null(User::LastActiveAt))
                    .col(string(User::AccountStatus).default("Active"))
                    .col(json_null(User::Settings))
                    .col(timestamp_with_time_zone_null(User::DeletedAt))
                    .to_owned(),
            )
            .await?;

        // Create indexes
        manager
            .create_index(
                Index::create()
                    .name("idx_user_username")
                    .table(User::Table)
                    .col(User::Username)
                    .to_owned(),
            )
            .await?;
            
        manager
            .create_index(
                Index::create()
                    .name("idx_user_email")
                    .table(User::Table)
                    .col(User::Email)
                    .to_owned(),
            )
            .await?;
            
        manager
            .create_index(
                Index::create()
                    .name("idx_user_role")
                    .table(User::Table)
                    .col(User::Role)
                    .to_owned(),
            )
            .await?;
            
        manager
            .create_index(
                Index::create()
                    .name("idx_user_account_status")
                    .table(User::Table)
                    .col(User::AccountStatus)
                    .to_owned(),
            )
            .await?;
            
        manager
            .create_index(
                Index::create()
                    .name("idx_user_last_active_at")
                    .table(User::Table)
                    .col(User::LastActiveAt)
                    .to_owned(),
            )
            .await

    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(User::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum User {
    Table,
    Id,
    Username,
    Country,
    FavoriteTile,
    Pronouns,
    MatchmakingRank,
    Password,
    CreatedAt,
    Passkey,
    Role,
    Email,
    LastActiveAt,
    AccountStatus,
    Settings,
    DeletedAt,
}
