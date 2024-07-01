use anyhow::{anyhow, Result};
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::Parser;
use kubegraph_api::{
    component::NetworkComponent,
    market::{
        price::{PriceHistogram, PriceItem},
        product::ProductSpec,
        r#pub::PubSpec,
        sub::SubSpec,
        BaseModel, Page,
    },
};
use kubegraph_market_migration::MigratorTrait;
use sea_orm::{ColumnTrait, DeleteResult, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use serde::{Deserialize, Serialize};
use tracing::{instrument, Level};

#[derive(Clone)]
pub struct Database {
    connection: ::sea_orm::DatabaseConnection,
    pub(crate) signal: FunctionSignal,
}

#[async_trait]
impl NetworkComponent for Database {
    type Args = DatabaseArgs;

    #[instrument(level = Level::INFO, skip(args, signal))]
    async fn try_new(
        args: <Self as NetworkComponent>::Args,
        signal: &FunctionSignal,
    ) -> Result<Self> {
        let DatabaseArgs { db_endpoint } = args;

        let opt = ::sea_orm::ConnectOptions::new(db_endpoint);
        let connection = ::sea_orm::Database::connect(opt)
            .await
            .map_err(|error| anyhow!("failed to connect to a market db: {error}"))?;

        let steps = None;
        ::migration::Migrator::up(&connection, steps)
            .await
            .map_err(|error| anyhow!("failed to upgrade the market db: {error}"))?;

        Ok(Self {
            connection,
            signal: signal.clone(),
        })
    }
}

impl Database {
    #[instrument(level = Level::INFO, skip(self))]
    pub async fn close(&self) -> Result<()> {
        Ok(())
    }
}

impl Database {
    #[instrument(level = Level::INFO, skip(self))]
    pub async fn get_product(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
    ) -> Result<Option<ProductSpec>> {
        let dsl = entity::product::Entity::find_by_id(prod_id);

        dsl.one(&self.connection)
            .await
            .map_err(Into::into)
            .and_then(|model| model.map(TryInto::try_into).transpose())
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn list_product_ids(
        &self,
        page: Page,
    ) -> Result<Vec<<ProductSpec as BaseModel>::Id>> {
        let Page { start, limit } = page;

        let col_id = entity::product::Column::Id;
        let dsl = entity::product::Entity::find()
            .select_only()
            .column(col_id)
            .order_by_asc(col_id)
            .limit(limit);
        let dsl = match start {
            Some(start) => dsl.filter(col_id.gt(start)),
            None => dsl,
        };

        dsl.into_tuple()
            .all(&self.connection)
            .await
            .map_err(Into::into)
    }

    #[instrument(level = Level::INFO, skip(self, spec))]
    pub async fn insert_product(
        &self,
        spec: ProductSpec,
    ) -> Result<<ProductSpec as BaseModel>::Id> {
        let prod_id = <ProductSpec as BaseModel>::Id::new_v4();
        let model = entity::product::ActiveModel::from_spec(spec, prod_id)?;
        let dsl = entity::product::Entity::insert(model);

        dsl.exec_without_returning(&self.connection).await?;
        Ok(prod_id)
    }

    // #[instrument(level = Level::INFO, skip(self, spec))]
    // pub async fn update_product(
    //     &self,
    //     prod_id: <ProductSpec as BaseModel>::Id,
    //     spec: ProductSpec,
    // ) -> Result<()> {
    //     let col_id = entity::product::Column::Id;
    //     let model = entity::product::ActiveModel::from_spec(spec, prod_id)?;
    //     let dsl = entity::product::Entity::update(model).filter(col_id.eq(prod_id));

    //     dsl.exec(&self.connection)
    //         .await
    //         .map(|_| ())
    //         .map_err(Into::into)
    // }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn remove_product(&self, prod_id: <ProductSpec as BaseModel>::Id) -> Result<()> {
        let model = entity::product::ActiveModel::from_id(prod_id);
        let dsl = entity::product::Entity::delete(model);

        let DeleteResult { rows_affected: _ } = dsl.exec(&self.connection).await?;
        Ok(())
    }
}

impl Database {
    // #[instrument(level = Level::INFO, skip(self))]
    // pub async fn list_price_ids(
    //     &self,
    //     prod_id: <ProductSpec as BaseModel>::Id,
    //     page: Page,
    // ) -> Result<Vec<<ProductSpec as BaseModel>::Id>> {
    //     let Page { start, limit } = page;

    //     let col_id = entity::price::Column::Id;
    //     let dsl = entity::price::Entity::find()
    //         .select_only()
    //         .column(col_id)
    //         .order_by_asc(col_id)
    //         .limit(limit);
    //     let dsl = match start {
    //         Some(start) => dsl.filter(col_id.gt(start)),
    //         None => dsl,
    //     };

    //     dsl.into_tuple()
    //         .all(&self.connection)
    //         .await
    //         .map_err(Into::into)
    // }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn list_price_histogram(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
        page: Page,
    ) -> Result<PriceHistogram> {
        let Page { start, limit } = page;

        let col_id = entity::price::Column::Id;
        let col_direction = entity::price::Column::Direction;
        let col_cost = entity::price::Column::Cost;
        let dsl = entity::price::Entity::find()
            .select_only()
            .columns([col_direction, col_cost])
            .order_by_asc(col_id)
            .limit(limit);
        let dsl = match start {
            Some(start) => dsl.filter(col_id.gt(start)),
            None => dsl,
        };

        dsl.into_tuple()
            .all(&self.connection)
            .await
            .map(|values| {
                values
                    .into_iter()
                    .map(|(direction, cost)| PriceItem {
                        direction: entity::price::Direction::into(direction),
                        cost,
                    })
                    .collect()
            })
            .map(PriceHistogram)
            .map_err(Into::into)
    }
}

impl Database {
    #[instrument(level = Level::INFO, skip(self))]
    pub async fn get_pub(&self, pub_id: <PubSpec as BaseModel>::Id) -> Result<Option<PubSpec>> {
        let dsl = entity::price::Entity::find_by_id(pub_id);

        dsl.one(&self.connection)
            .await
            .map_err(Into::into)
            .and_then(|model| model.map(TryInto::try_into).transpose())
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn list_pub_ids(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
        page: Page,
    ) -> Result<Vec<<PubSpec as BaseModel>::Id>> {
        let Page { start, limit } = page;

        let col_direction = entity::price::Column::Direction;
        let filter_direction = col_direction.eq(entity::price::Direction::Pub);

        let col_id = entity::price::Column::Id;
        let dsl = entity::price::Entity::find()
            .select_only()
            .column(col_id)
            .order_by_asc(col_id)
            .limit(limit);
        let dsl = match start {
            Some(start) => dsl.filter(col_id.gt(start).and(filter_direction)),
            None => dsl,
        };

        dsl.into_tuple()
            .all(&self.connection)
            .await
            .map_err(Into::into)
    }

    #[instrument(level = Level::INFO, skip(self, spec))]
    pub async fn insert_pub(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
        spec: PubSpec,
    ) -> Result<<PubSpec as BaseModel>::Id> {
        let pub_id = <ProductSpec as BaseModel>::Id::new_v4();
        let model = entity::price::ActiveModel::from_pub_spec(spec, Some(prod_id), pub_id)?;
        let dsl = entity::price::Entity::insert(model);

        dsl.exec_without_returning(&self.connection).await?;
        Ok(pub_id)
    }

    // #[instrument(level = Level::INFO, skip(self, spec))]
    // pub async fn update_pub(
    //     &self,
    //     pub_id: <PubSpec as BaseModel>::Id,
    //     spec: PubSpec,
    // ) -> Result<()> {
    //     let col_id = entity::price::Column::Id;
    //     let model = entity::price::ActiveModel::from_pub_spec(spec, None, pub_id)?;
    //     let dsl = entity::price::Entity::update(model).filter(col_id.eq(pub_id));

    //     dsl.exec(&self.connection)
    //         .await
    //         .map(|_| ())
    //         .map_err(Into::into)
    // }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn remove_pub(&self, pub_id: <PubSpec as BaseModel>::Id) -> Result<()> {
        let model = entity::price::ActiveModel::from_id(pub_id);
        let dsl = entity::price::Entity::delete(model);

        let DeleteResult { rows_affected: _ } = dsl.exec(&self.connection).await?;
        Ok(())
    }
}

impl Database {
    #[instrument(level = Level::INFO, skip(self))]
    pub async fn get_sub(&self, sub_id: <SubSpec as BaseModel>::Id) -> Result<Option<SubSpec>> {
        let dsl = entity::price::Entity::find_by_id(sub_id);

        dsl.one(&self.connection)
            .await
            .map_err(Into::into)
            .and_then(|model| model.map(TryInto::try_into).transpose())
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn list_sub_ids(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
        page: Page,
    ) -> Result<Vec<<SubSpec as BaseModel>::Id>> {
        let Page { start, limit } = page;

        let col_direction = entity::price::Column::Direction;
        let filter_direction = col_direction.eq(entity::price::Direction::Sub);

        let col_id = entity::price::Column::Id;
        let dsl = entity::price::Entity::find()
            .select_only()
            .column(col_id)
            .order_by_asc(col_id)
            .limit(limit);
        let dsl = match start {
            Some(start) => dsl.filter(col_id.gt(start).and(filter_direction)),
            None => dsl,
        };

        dsl.into_tuple()
            .all(&self.connection)
            .await
            .map_err(Into::into)
    }

    #[instrument(level = Level::INFO, skip(self, spec))]
    pub async fn insert_sub(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
        spec: SubSpec,
    ) -> Result<<SubSpec as BaseModel>::Id> {
        let sub_id = <ProductSpec as BaseModel>::Id::new_v4();
        let model = entity::price::ActiveModel::from_sub_spec(spec, Some(prod_id), sub_id)?;
        let dsl = entity::price::Entity::insert(model);

        dsl.exec_without_returning(&self.connection).await?;
        Ok(sub_id)
    }

    // #[instrument(level = Level::INFO, skip(self, spec))]
    // pub async fn update_sub(
    //     &self,
    //     sub_id: <SubSpec as BaseModel>::Id,
    //     spec: SubSpec,
    // ) -> Result<()> {
    //     let col_id = entity::price::Column::Id;
    //     let model = entity::price::ActiveModel::from_sub_spec(spec, None, sub_id)?;
    //     let dsl = entity::price::Entity::update(model).filter(col_id.eq(sub_id));

    //     dsl.exec(&self.connection)
    //         .await
    //         .map(|_| ())
    //         .map_err(Into::into)
    // }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn remove_sub(&self, sub_id: <SubSpec as BaseModel>::Id) -> Result<()> {
        let model = entity::price::ActiveModel::from_id(sub_id);
        let dsl = entity::price::Entity::delete(model);

        let DeleteResult { rows_affected: _ } = dsl.exec(&self.connection).await?;
        Ok(())
    }
}

#[derive(Clone, Debug, Parser, Serialize, Deserialize)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct DatabaseArgs {
    #[arg(
        long,
        env = "KUBEGRAPH_MARKET_DB_ENDPOINT",
        value_name = "DIR",
        default_value_t = DatabaseArgs::default_db_endpoint(),
    )]
    pub db_endpoint: String,
}

impl DatabaseArgs {
    fn default_db_endpoint() -> String {
        "sqlite::memory:".into()
    }
}
