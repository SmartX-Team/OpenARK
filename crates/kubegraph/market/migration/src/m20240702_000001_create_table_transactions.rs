use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(self::Transactions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(self::Transactions::Id)
                            .uuid() // Uuid
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(self::Transactions::ProdId)
                            .uuid() // Uuid
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-transactions-prod_id")
                            .from(self::Transactions::Table, self::Transactions::ProdId)
                            .to(
                                super::m20240701_000002_create_table_prices::Prices::Table,
                                super::m20240701_000002_create_table_prices::Prices::Id,
                            )
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .col(
                        ColumnDef::new(self::Transactions::PubId)
                            .uuid() // Uuid
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-transactions-pub_id")
                            .from(self::Transactions::Table, self::Transactions::PubId)
                            .to(
                                super::m20240701_000002_create_table_prices::Prices::Table,
                                super::m20240701_000002_create_table_prices::Prices::Id,
                            )
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .col(
                        ColumnDef::new(self::Transactions::SubId)
                            .uuid() // Uuid
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-transactions-sub_id")
                            .from(self::Transactions::Table, self::Transactions::SubId)
                            .to(
                                super::m20240701_000002_create_table_prices::Prices::Table,
                                super::m20240701_000002_create_table_prices::Prices::Id,
                            )
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .col(
                        ColumnDef::new(self::Transactions::CreatedAt)
                            .timestamp() // NaiveDateTime
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(self::Transactions::Cost)
                            .big_integer() // i64
                            .check(Expr::col(self::Transactions::Cost).gte(0)) // unsigned
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(self::Transactions::Count)
                            .big_integer() // i64
                            .check(Expr::col(self::Transactions::Count).gte(0)) // unsigned
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(self::Transactions::PubState)
                            .small_integer() // enum TaskState -> i16
                            .default(0) // TaskState::Running -> 0
                            .check(
                                Expr::col(self::Transactions::PubState)
                                    .gte(0) // unsigned
                                    .and(Expr::col(self::Transactions::PubState).lt(3)), // len(TaskState) -> 3
                            )
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(self::Transactions::PubUpdatedAt)
                            .timestamp() // NaiveDateTime
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(self::Transactions::SubState)
                            .small_integer() // enum TaskState -> i16
                            .check(
                                Expr::col(self::Transactions::SubState)
                                    .gte(0) // unsigned
                                    .and(Expr::col(self::Transactions::SubState).lt(3)), // len(TaskState) -> 3
                            )
                            .default(0) // TaskState::Running -> 0
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(self::Transactions::SubUpdatedAt)
                            .timestamp() // NaiveDateTime
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(self::Transactions::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub(super) enum Transactions {
    Table,
    Id,
    ProdId,
    PubId,
    SubId,
    CreatedAt,
    Cost,
    Count,
    PubState,
    PubUpdatedAt,
    SubState,
    SubUpdatedAt,
}
