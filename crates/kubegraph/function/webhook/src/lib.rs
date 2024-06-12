#[cfg(feature = "df-polars")]
extern crate polars as pl;

#[cfg(feature = "df-polars")]
mod polars;

use anyhow::Result;
use async_trait::async_trait;
use kubegraph_api::{
    frame::LazyFrame, function::spawn::FunctionSpawnContext, graph::ScopedNetworkGraphDB,
};

#[async_trait]
pub trait NetworkFunctionWebhook<DB, T, M>
where
    DB: ScopedNetworkGraphDB<LazyFrame, M>,
{
    async fn spawn(&self, ctx: FunctionSpawnContext<'async_trait, DB, T, M>) -> Result<()>
    where
        DB: 'async_trait + Send,
        M: 'async_trait + Send;
}
