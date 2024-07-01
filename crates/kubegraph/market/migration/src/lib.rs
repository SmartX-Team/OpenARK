mod m20240701_000001_create_table_products;
mod m20240701_000002_create_table_prices;

use async_trait::async_trait;

pub use sea_orm_migration::prelude::*;

pub struct Migrator;

#[async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(self::m20240701_000001_create_table_products::Migration),
            Box::new(self::m20240701_000002_create_table_prices::Migration),
        ]
    }
}
