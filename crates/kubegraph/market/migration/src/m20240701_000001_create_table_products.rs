use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(self::Products::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(self::Products::Id)
                            .uuid() // Uuid
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(self::Products::CreatedAt)
                            .timestamp() // NaiveDateTime
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(self::Products::Spec)
                            .json() // JSON Value
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(self::Products::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub(super) enum Products {
    Table,
    Id,
    CreatedAt,
    Spec,
}
