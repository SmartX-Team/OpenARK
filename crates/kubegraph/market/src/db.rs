use std::{marker::PhantomData, ops::Bound};

use anyhow::{anyhow, Result};
use clap::Parser;
use kubegraph_api::market::{BaseModel, Page};
use serde::{Deserialize, Serialize};
use tikv_client::{BoundRange, KvPair, TransactionClient};
use tracing::{instrument, Level};
use uuid::Uuid;

#[derive(Clone)]
pub struct Database {
    client: TransactionClient,
}

impl Database {
    #[instrument(level = Level::INFO, skip(args))]
    pub async fn try_new(args: DatabaseArgs) -> Result<Self> {
        let DatabaseArgs { pd_endpoints } = args;

        Ok(Self {
            client: TransactionClient::new(pd_endpoints.split(',').collect())
                .await
                .map_err(|error| anyhow!("failed to create a tikv client: {error}"))?,
        })
    }

    pub const fn checkout<M>(&self) -> DatabaseSession<'_, M>
    where
        M: BaseModel,
    {
        DatabaseSession {
            base_dir: None,
            db: self,
            model: PhantomData,
        }
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn close(&self) -> Result<()> {
        Ok(())
    }
}

#[derive(Copy, Clone)]
pub struct DatabaseSession<'a, M>
where
    M: BaseModel,
{
    base_dir: Option<(&'static str, <M as BaseModel>::Id)>,
    db: &'a Database,
    model: PhantomData<M>,
}

impl<'a, M> DatabaseSession<'a, M>
where
    M: BaseModel<Id = Uuid>,
{
    pub const fn enter<Dst>(self, id: <M as BaseModel>::Id) -> DatabaseSession<'a, Dst>
    where
        Dst: BaseModel<Id = <M as BaseModel>::Id>,
    {
        let Self {
            base_dir: _,
            db,
            model: PhantomData,
        } = self;
        DatabaseSession {
            base_dir: Some((<M as BaseModel>::KEY, id)),
            db,
            model: PhantomData,
        }
    }

    fn get_key(&self, id: Option<<M as BaseModel>::Id>) -> String {
        let key = <M as BaseModel>::KEY;
        match self.base_dir {
            Some((k, v)) => match id {
                Some(id) => format!("/{k}/{v}/{key}/{id}"),
                None => format!("/{k}/{v}/{key}/"),
            },
            None => match id {
                Some(id) => format!("/{key}/{id}"),
                None => format!("/{key}/"),
            },
        }
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn get(&self, id: Option<<M as BaseModel>::Id>) -> Result<Option<M>> {
        let value = {
            let key = self.get_key(id).into_bytes();

            let mut txn = self.db.client.begin_optimistic().await?;
            let value = txn.get(key).await?;
            txn.commit().await?;
            value
        };
        value
            .map(|ref value| ::serde_json::from_slice(value))
            .transpose()
            .map_err(Into::into)
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn list(&self, page: Page) -> Result<Vec<<M as BaseModel>::Id>> {
        let Page { start, limit } = page;
        let values = {
            let from = match start {
                Some(start) => Bound::Excluded(self.get_key(Some(start)).into_bytes().into()),
                None => Bound::Included(self.get_key(None).into_bytes().into()),
            };
            let to = {
                let mut key = self.get_key(start).into_bytes();
                match key.last_mut() {
                    Some(byte) => {
                        *byte += 1;
                        Bound::Excluded(key.into())
                    }
                    None => Bound::Unbounded,
                }
            };

            let range = BoundRange { from, to };

            let mut txn = self.db.client.begin_optimistic().await?;
            let values = txn
                .scan(range, limit)
                .await?
                .filter_map(|KvPair(key, _)| String::from_utf8(key.into()).ok())
                .filter_map(|key| key.split('/').last().and_then(|id| id.parse().ok()))
                .collect();
            txn.commit().await?;
            values
        };
        Ok(values)
    }

    #[instrument(level = Level::INFO, skip(self, model))]
    pub async fn insert(&self, model: M) -> Result<<M as BaseModel>::Id> {
        let id = Uuid::new_v4();
        self.update(Some(id), model).await?;
        Ok(id)
    }

    #[instrument(level = Level::INFO, skip(self, model))]
    pub async fn update(&self, id: Option<<M as BaseModel>::Id>, model: M) -> Result<()> {
        {
            let key = self.get_key(id).into_bytes();
            let value = ::serde_json::to_vec(&model)?;

            let mut txn = self.db.client.begin_optimistic().await?;
            txn.put(key, value).await?;
            txn.commit().await?;
        }
        Ok(())
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn remove(&self, id: Option<<M as BaseModel>::Id>) -> Result<()> {
        {
            let key = self.get_key(id).into_bytes();

            let mut txn = self.db.client.begin_optimistic().await?;
            txn.delete(key).await?;
            txn.commit().await?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Parser, Serialize, Deserialize)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct DatabaseArgs {
    #[arg(
        long,
        env = "KUBEGRAPH_MARKET_PD_ENDPOINTS",
        value_name = "DIR",
        default_value_t = DatabaseArgs::default_pd_endpoints(),
    )]
    pub pd_endpoints: String,
}

impl DatabaseArgs {
    fn default_pd_endpoints() -> String {
        "market-pd.kubegraph.svc:2379".into()
    }
}
