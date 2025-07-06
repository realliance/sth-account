use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(AuditLog::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(AuditLog::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(AuditLog::UserId).uuid().not_null())
                    .col(ColumnDef::new(AuditLog::ActionType).string().not_null())
                    .col(ColumnDef::new(AuditLog::Details).json().null())
                    .col(ColumnDef::new(AuditLog::IpAddress).string().not_null())
                    .col(ColumnDef::new(AuditLog::ModeratorId).uuid().null())
                    .col(
                        ColumnDef::new(AuditLog::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(AuditLog::DeletedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_audit_log_user_id")
                            .from(AuditLog::Table, AuditLog::UserId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_audit_log_moderator_id")
                            .from(AuditLog::Table, AuditLog::ModeratorId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_audit_log_user_id")
                    .table(AuditLog::Table)
                    .col(AuditLog::UserId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_audit_log_action_type")
                    .table(AuditLog::Table)
                    .col(AuditLog::ActionType)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_audit_log_created_at")
                    .table(AuditLog::Table)
                    .col(AuditLog::CreatedAt)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_audit_log_deleted_at")
                    .table(AuditLog::Table)
                    .col(AuditLog::DeletedAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(AuditLog::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum AuditLog {
    Table,
    Id,
    UserId,
    ActionType,
    Details,
    IpAddress,
    ModeratorId,
    CreatedAt,
    DeletedAt,
}

#[derive(DeriveIden)]
enum User {
    Table,
    Id,
}
