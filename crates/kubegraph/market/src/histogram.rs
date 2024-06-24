use std::{collections::BTreeMap, marker::PhantomData};

use anyhow::Result;
use kubegraph_api::market::{product::PriceHistogram, BaseModel};
use tracing::{instrument, Level};
use uuid::Uuid;

use crate::db::{Database, DatabaseSession};

#[derive(Clone)]
pub struct HistogramClient<Prod, Pub, Sub> {
    db: Database,
    model: PhantomData<(Prod, Pub, Sub)>,
}

impl<Prod, Pub, Sub> HistogramClient<Prod, Pub, Sub>
where
    Prod: BaseModel<Id = Uuid>,
    Pub: BaseModel<Id = Uuid, Cost = <Prod as BaseModel>::Cost, Count = <Prod as BaseModel>::Count>,
    Sub: BaseModel<Id = Uuid, Cost = <Prod as BaseModel>::Cost, Count = <Prod as BaseModel>::Count>,
{
    pub const fn new(db: Database) -> Self {
        Self {
            db,
            model: PhantomData,
        }
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn get(&self, prod_id: <Prod as BaseModel>::Id) -> Result<PriceHistogram<Pub, Sub>> {
        self.checkout(prod_id)
            .get(None)
            .await
            .map(Option::unwrap_or_default)
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn insert_pub(
        &self,
        prod_id: <Prod as BaseModel>::Id,
        pub_id: <Pub as BaseModel>::Id,
        cost: <Pub as BaseModel>::Cost,
    ) -> Result<()> {
        self.insert(prod_id, pub_id, cost, |hist| &mut hist.r#pub)
            .await
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn insert_sub(
        &self,
        prod_id: <Prod as BaseModel>::Id,
        sub_id: <Sub as BaseModel>::Id,
        cost: <Sub as BaseModel>::Cost,
    ) -> Result<()> {
        self.insert(prod_id, sub_id, cost, |hist| &mut hist.sub)
            .await
    }

    #[instrument(level = Level::INFO, skip(self, map))]
    async fn insert(
        &self,
        prod_id: <Prod as BaseModel>::Id,
        item_id: <Prod as BaseModel>::Id,
        cost: <Prod as BaseModel>::Cost,
        map: impl FnOnce(
            &mut PriceHistogram<Pub, Sub>,
        ) -> &mut BTreeMap<<Prod as BaseModel>::Id, <Prod as BaseModel>::Cost>,
    ) -> Result<()> {
        let session = self.checkout(prod_id);
        let mut histogram = session.get(None).await?.unwrap_or_default();
        map(&mut histogram).insert(item_id, cost);
        session.update(None, histogram).await
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn remove_all(&self, prod_id: <Prod as BaseModel>::Id) -> Result<()> {
        self.checkout(prod_id).remove(None).await
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn remove_pub(
        &self,
        prod_id: <Prod as BaseModel>::Id,
        pub_id: <Sub as BaseModel>::Id,
    ) -> Result<()> {
        self.remove(prod_id, pub_id, |hist| &mut hist.r#pub).await
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn remove_sub(
        &self,
        prod_id: <Prod as BaseModel>::Id,
        sub_id: <Sub as BaseModel>::Id,
    ) -> Result<()> {
        self.remove(prod_id, sub_id, |hist| &mut hist.sub).await
    }

    #[instrument(level = Level::INFO, skip(self, map))]
    async fn remove(
        &self,
        prod_id: <Prod as BaseModel>::Id,
        item_id: <Prod as BaseModel>::Id,
        map: impl FnOnce(
            &mut PriceHistogram<Pub, Sub>,
        ) -> &mut BTreeMap<<Prod as BaseModel>::Id, <Prod as BaseModel>::Cost>,
    ) -> Result<()> {
        let session = self.checkout(prod_id);
        match session.get(None).await? {
            Some(mut histogram) => {
                map(&mut histogram).remove(&item_id);
                session.update(None, histogram).await
            }
            None => Ok(()),
        }
    }

    fn checkout(
        &self,
        prod_id: <Prod as BaseModel>::Id,
    ) -> DatabaseSession<'_, PriceHistogram<Pub, Sub>> {
        self.db.checkout::<Prod>().enter(prod_id)
    }
}
