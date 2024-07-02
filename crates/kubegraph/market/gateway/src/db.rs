use std::convert::identity;

use anyhow::{anyhow, Result};
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use chrono::NaiveDateTime;
use clap::Parser;
use futures::TryFutureExt;
use kubegraph_api::{
    component::NetworkComponent,
    market::{
        function::MarketFunctionContext,
        price::{PriceHistogram, PriceItem},
        product::ProductSpec,
        r#pub::PubSpec,
        sub::SubSpec,
        transaction::{TransactionError, TransactionSpec, TransactionTemplate},
        BaseModel, Page,
    },
};
use kubegraph_market_function::{MarketFunction, MarketFunctionClient, MarketFunctionClientArgs};
use kubegraph_market_migration::MigratorTrait;
use sea_orm::{
    ActiveValue, ColumnTrait, DbErr, DeleteResult, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use tokio::try_join;
use tracing::{error, instrument, Level};

#[derive(Clone)]
pub struct Database {
    connection: ::sea_orm::DatabaseConnection,
    function: MarketFunctionClient,
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
        let DatabaseArgs {
            db_endpoint,
            function,
        } = args;

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
            function: MarketFunctionClient::try_new(function, signal).await?,
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
        let col_timestamp = entity::price::Column::CreatedAt;
        let col_direction = entity::price::Column::Direction;
        let col_cost = entity::price::Column::Cost;
        let col_count = entity::price::Column::Count;
        let dsl = entity::price::Entity::find()
            .select_only()
            .columns([col_id, col_timestamp, col_direction, col_cost, col_count])
            .order_by_asc(col_id)
            .limit(limit);

        let filter = self::filter::default_price(None);
        let dsl = match start {
            Some(start) => dsl.filter(col_id.gt(start).and(filter)),
            None => dsl.filter(filter),
        };

        dsl.into_tuple()
            .all(&self.connection)
            .await
            .map(|values| {
                values
                    .into_iter()
                    .map(|(id, timestamp, direction, cost, count)| PriceItem {
                        id,
                        timestamp: NaiveDateTime::and_utc(&timestamp),
                        direction: entity::price::Direction::into(direction),
                        cost,
                        count,
                    })
                    .collect()
            })
            .map_err(Into::into)
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn trade(
        &self,
        template: TransactionTemplate,
    ) -> Result<<TransactionSpec as BaseModel>::Id, TransactionError> {
        let (
            txn_id,
            TransactionTemplate {
                r#pub,
                sub,
                cost: _,
                count: _,
            },
        ) = self.trade_on_db(template).await?;

        let ctx = MarketFunctionContext { template };

        let task_pub = self
            .function
            .spawn(ctx.clone(), r#pub)
            .map_err(TransactionError::FunctionFailedPub);
        let task_sub = self
            .function
            .spawn(ctx, sub)
            .map_err(TransactionError::FunctionFailedSub);

        try_join!(task_pub, task_sub).map(|((), ())| txn_id)
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn trade_on_db(
        &self,
        template: TransactionTemplate,
    ) -> Result<
        (
            <TransactionSpec as BaseModel>::Id,
            TransactionTemplate<PubSpec, SubSpec>,
        ),
        TransactionError,
    > {
        self.connection
            .transaction::<_, _, DbErr>(|txn| {
                Box::pin(async move {
                    let TransactionTemplate {
                        r#pub: pub_id,
                        sub: sub_id,
                        cost,
                        count,
                    } = template;

                    if count <= 0 {
                        return Ok(Err(TransactionError::EmptyCount));
                    }

                    let r#pub = match entity::price::Entity::find_by_id(pub_id).one(txn).await? {
                        Some(item) => {
                            if item.direction != entity::price::Direction::Pub
                                || item.cost > cost
                                || item.count >= count
                            {
                                item
                            } else {
                                return Ok(Err(TransactionError::OutOfPub));
                            }
                        }
                        None => return Ok(Err(TransactionError::OutOfPub)),
                    };
                    let sub = match entity::price::Entity::find_by_id(sub_id).one(txn).await? {
                        Some(item) => {
                            if item.direction != entity::price::Direction::Sub
                                || item.cost < cost
                                || item.count >= count
                            {
                                item
                            } else {
                                return Ok(Err(TransactionError::OutOfSub));
                            }
                        }
                        None => return Ok(Err(TransactionError::OutOfSub)),
                    };

                    let pub_spec = match r#pub.clone().try_into() {
                        Ok(spec) => spec,
                        Err(error) => {
                            error!("{error}");
                            return Ok(Err(TransactionError::OutOfPub));
                        }
                    };
                    let sub_spec = match sub.clone().try_into() {
                        Ok(spec) => spec,
                        Err(error) => {
                            error!("{error}");
                            return Ok(Err(TransactionError::OutOfPub));
                        }
                    };

                    let withdraw = |price: entity::price::Model| async move {
                        let col_id = entity::price::Column::Id;
                        let model = entity::price::ActiveModel {
                            id: ActiveValue::Unchanged(price.id),
                            product_id: ActiveValue::Unchanged(price.product_id),
                            created_at: ActiveValue::Unchanged(price.created_at),
                            direction: ActiveValue::Unchanged(price.direction),
                            cost: ActiveValue::Unchanged(price.cost),
                            count: ActiveValue::Set(price.count - count),
                            spec: ActiveValue::Unchanged(price.spec),
                        };
                        let dsl = entity::price::Entity::update(model).filter(col_id.eq(price.id));

                        dsl.exec(txn).await.map(|_| ())
                    };

                    withdraw(r#pub).await?;
                    withdraw(sub).await?;

                    let txn_id = {
                        let txn_id = <TransactionSpec as BaseModel>::Id::new_v4();
                        let model =
                            entity::transaction::ActiveModel::from_template(txn_id, template);
                        let dsl = entity::transaction::Entity::insert(model);

                        dsl.exec_without_returning(txn).await?;
                        txn_id
                    };

                    let template = TransactionTemplate {
                        r#pub: pub_spec,
                        sub: sub_spec,
                        cost,
                        count,
                    };
                    Ok(Ok((txn_id, template)))
                })
            })
            .await
            .map_err(|error| match error {
                ::sea_orm::TransactionError::Connection(error) => {
                    error!("failed to connect to DB while trading: {error}");
                    TransactionError::TransactionFailed
                }
                ::sea_orm::TransactionError::Transaction(error) => {
                    error!("failed to execute transaction on DB while trading: {error}");
                    TransactionError::TransactionFailed
                }
            })
            .and_then(identity)
    }
}

impl Database {
    #[instrument(level = Level::INFO, skip(self))]
    pub async fn get_pub(&self, pub_id: <PubSpec as BaseModel>::Id) -> Result<Option<PubSpec>> {
        let filter = self::filter::default_price(Some(entity::price::Direction::Pub));
        let dsl = entity::price::Entity::find_by_id(pub_id).filter(filter);

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

        let col_id = entity::price::Column::Id;
        let filter = self::filter::default_price(Some(entity::price::Direction::Pub));
        let dsl = entity::price::Entity::find()
            .select_only()
            .column(col_id)
            .order_by_asc(col_id)
            .limit(limit);
        let dsl = match start {
            Some(start) => dsl.filter(col_id.gt(start).and(filter)),
            None => dsl.filter(filter),
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
        let pub_id = <PubSpec as BaseModel>::Id::new_v4();
        let model = entity::price::ActiveModel::from_pub_spec(spec, Some(prod_id), pub_id)?;
        let dsl = entity::price::Entity::insert(model);

        dsl.exec_without_returning(&self.connection).await?;
        Ok(pub_id)
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn remove_pub(&self, pub_id: <PubSpec as BaseModel>::Id) -> Result<()> {
        let model = entity::price::ActiveModel::from_id(pub_id);
        let filter = self::filter::default_price(Some(entity::price::Direction::Pub));
        let dsl = entity::price::Entity::delete(model).filter(filter);

        let DeleteResult { rows_affected: _ } = dsl.exec(&self.connection).await?;
        Ok(())
    }
}

impl Database {
    #[instrument(level = Level::INFO, skip(self))]
    pub async fn get_sub(&self, sub_id: <SubSpec as BaseModel>::Id) -> Result<Option<SubSpec>> {
        let filter = self::filter::default_price(Some(entity::price::Direction::Sub));
        let dsl = entity::price::Entity::find_by_id(sub_id).filter(filter);

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

        let col_id = entity::price::Column::Id;
        let filter = self::filter::default_price(Some(entity::price::Direction::Sub));
        let dsl = entity::price::Entity::find()
            .select_only()
            .column(col_id)
            .order_by_asc(col_id)
            .limit(limit);
        let dsl = match start {
            Some(start) => dsl.filter(col_id.gt(start).and(filter)),
            None => dsl.filter(filter),
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
        let sub_id = <SubSpec as BaseModel>::Id::new_v4();
        let model = entity::price::ActiveModel::from_sub_spec(spec, Some(prod_id), sub_id)?;
        let dsl = entity::price::Entity::insert(model);

        dsl.exec_without_returning(&self.connection).await?;
        Ok(sub_id)
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn remove_sub(&self, sub_id: <SubSpec as BaseModel>::Id) -> Result<()> {
        let model = entity::price::ActiveModel::from_id(sub_id);
        let filter = self::filter::default_price(Some(entity::price::Direction::Sub));
        let dsl = entity::price::Entity::delete(model).filter(filter);

        let DeleteResult { rows_affected: _ } = dsl.exec(&self.connection).await?;
        Ok(())
    }
}

impl Database {
    #[instrument(level = Level::INFO, skip(self))]
    pub async fn get_transaction(
        &self,
        txn_id: <TransactionSpec as BaseModel>::Id,
    ) -> Result<Option<TransactionSpec>> {
        let dsl = entity::transaction::Entity::find_by_id(txn_id);

        dsl.one(&self.connection)
            .await
            .map_err(Into::into)
            .map(|model| model.map(Into::into))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
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

    #[command(flatten)]
    pub function: MarketFunctionClientArgs,
}

impl DatabaseArgs {
    fn default_db_endpoint() -> String {
        "sqlite::memory:".into()
    }
}

mod filter {
    use migration::SimpleExpr;
    use sea_orm::ColumnTrait;

    pub(crate) fn default_price(direction: Option<entity::price::Direction>) -> SimpleExpr {
        let col_cost = entity::price::Column::Cost;
        let col_count = entity::price::Column::Count;
        let filter = col_cost.gte(0).and(col_count.gt(0));

        match direction {
            Some(direction) => {
                let col_direction = entity::price::Column::Direction;
                filter.and(col_direction.eq(direction))
            }
            None => filter,
        }
    }
}
