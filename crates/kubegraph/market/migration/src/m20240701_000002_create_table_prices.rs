use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(self::Prices::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(self::Prices::Id)
                            .uuid() // Uuid
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(self::Prices::ProductId)
                            .uuid() // Uuid
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-prices-product_id")
                            .from(self::Prices::Table, self::Prices::ProductId)
                            .to(
                                super::m20240701_000001_create_table_products::Products::Table,
                                super::m20240701_000001_create_table_products::Products::Id,
                            )
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .col(
                        ColumnDef::new(self::Prices::CreatedAt)
                            .timestamp() // NaiveDateTime
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(self::Prices::Direction)
                            .small_integer() // enum Direction -> i16
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(self::Prices::Cost)
                            .big_integer() // i64
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(self::Prices::Count)
                            .big_integer() // i64
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(self::Prices::Spec)
                            .json() // JSON Value
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(self::Prices::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub(super) enum Prices {
    Table,
    Id,
    ProductId,
    CreatedAt,
    Direction,
    Cost,
    Count,
    Spec,
}
