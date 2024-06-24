use anyhow::Result;
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::Parser;
use kubegraph_api::{
    component::NetworkComponent,
    market::{
        product::{PriceHistogram, ProductSpec},
        r#pub::PubSpec,
        sub::SubSpec,
        BaseModel, BaseModelItem, Page,
    },
};
use serde::{Deserialize, Serialize};
use tokio::{spawn, task::JoinHandle};
use tracing::{instrument, Level};

use crate::{
    db::{Database, DatabaseArgs, DatabaseSession},
    histogram::HistogramClient,
};

#[derive(Clone)]
pub struct Agent {
    db: Database,
    histogram: HistogramClient<ProductSpec, PubSpec, SubSpec>,
    pub(crate) signal: FunctionSignal,
}

#[async_trait]
impl NetworkComponent for Agent {
    type Args = AgentArgs;

    #[instrument(level = Level::INFO, skip(args, signal))]
    async fn try_new(
        args: <Self as NetworkComponent>::Args,
        signal: &FunctionSignal,
    ) -> Result<Self> {
        let AgentArgs { db } = args;

        let db = Database::try_new(db).await?;
        let histogram = HistogramClient::new(db.clone());

        Ok(Self {
            db,
            histogram,
            signal: signal.clone(),
        })
    }
}

impl Agent {
    pub(crate) fn spawn_workers(&self) -> Vec<JoinHandle<()>> {
        vec![spawn(crate::actix::loop_forever(self.clone()))]
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub(crate) async fn close(self) -> Result<()> {
        self.db.close().await
    }
}

impl Agent {
    pub(crate) async fn list_product(
        &self,
        page: Page,
    ) -> Result<Vec<<ProductSpec as BaseModel>::Id>> {
        self.checkout_product().list(page).await
    }

    pub(crate) async fn get_product(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
    ) -> Result<Option<ProductSpec>> {
        self.checkout_product().get(Some(prod_id)).await
    }

    pub(crate) async fn put_product(
        &self,
        spec: ProductSpec,
    ) -> Result<<ProductSpec as BaseModel>::Id> {
        self.checkout_product().insert(spec).await
    }

    pub(crate) async fn delete_product(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
    ) -> Result<()> {
        self.checkout_product().remove(Some(prod_id)).await
    }

    fn checkout_product(&self) -> DatabaseSession<'_, ProductSpec> {
        self.db.checkout()
    }
}

impl Agent {
    pub(crate) async fn list_price(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
    ) -> Result<PriceHistogram> {
        self.histogram.get(prod_id).await
    }
}

impl Agent {
    pub(crate) async fn list_pub(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
        page: Page,
    ) -> Result<Vec<<PubSpec as BaseModel>::Id>> {
        self.checkout_pub(prod_id).list(page).await
    }

    pub(crate) async fn get_pub(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
        pub_id: <PubSpec as BaseModel>::Id,
    ) -> Result<Option<PubSpec>> {
        self.checkout_pub(prod_id).get(Some(pub_id)).await
    }

    pub(crate) async fn put_pub(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
        spec: PubSpec,
    ) -> Result<<PubSpec as BaseModel>::Id> {
        let cost = spec.cost();
        let pub_id = self.checkout_pub(prod_id).insert(spec).await?;
        self.histogram.insert_pub(prod_id, pub_id, cost).await?;
        Ok(pub_id)
    }

    pub(crate) async fn delete_pub(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
        pub_id: <PubSpec as BaseModel>::Id,
    ) -> Result<()> {
        self.checkout_pub(prod_id).remove(Some(pub_id)).await?;
        self.histogram.remove_pub(prod_id, pub_id).await
    }

    fn checkout_pub(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
    ) -> DatabaseSession<'_, PubSpec> {
        self.checkout_product().enter(prod_id)
    }
}

impl Agent {
    pub(crate) async fn list_sub(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
        page: Page,
    ) -> Result<Vec<<SubSpec as BaseModel>::Id>> {
        self.checkout_sub(prod_id).list(page).await
    }

    pub(crate) async fn get_sub(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
        sub_id: <SubSpec as BaseModel>::Id,
    ) -> Result<Option<SubSpec>> {
        self.checkout_sub(prod_id).get(Some(sub_id)).await
    }

    pub(crate) async fn put_sub(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
        spec: SubSpec,
    ) -> Result<<SubSpec as BaseModel>::Id> {
        let cost = spec.cost();
        let sub_id = self.checkout_sub(prod_id).insert(spec).await?;
        self.histogram.insert_sub(prod_id, sub_id, cost).await?;
        Ok(sub_id)
    }

    pub(crate) async fn delete_sub(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
        sub_id: <SubSpec as BaseModel>::Id,
    ) -> Result<()> {
        self.checkout_pub(prod_id).remove(Some(sub_id)).await?;
        self.histogram.remove_sub(prod_id, sub_id).await
    }

    fn checkout_sub(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
    ) -> DatabaseSession<'_, SubSpec> {
        self.checkout_product().enter(prod_id)
    }
}

#[derive(Clone, Debug, Parser, Serialize, Deserialize)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct AgentArgs {
    #[command(flatten)]
    pub db: DatabaseArgs,
}
