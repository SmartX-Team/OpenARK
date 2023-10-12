use std::marker::PhantomData;

use anyhow::{anyhow, Result};
use dash_pipe_provider::storage::{
    lakehouse::{decoder::TryIntoTableDecoder, StorageBackend},
    Stream,
};
pub use dash_pipe_provider::{storage::StorageS3Args as QueryClientArgs, Name};
use datafusion::prelude::{DataFrame, SessionContext};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;

pub struct QueryClient<Value> {
    ctx: SessionContext,
    _value: PhantomData<Value>,
}

impl<Value> QueryClient<Value> {
    pub const TABLE_NAME: &'static str = StorageBackend::TABLE_NAME;

    pub async fn try_new(args: &QueryClientArgs, model: Option<&Name>) -> Result<Self>
    where
        Value: Default + JsonSchema,
    {
        StorageBackend::try_new::<Value>(args, model)
            .await
            .and_then(|mut backend| backend.get_session_context().cloned())
            .map(|ctx| Self {
                ctx,
                _value: PhantomData,
            })
    }

    pub async fn sql(&self, sql: &str) -> Result<DataFrame> {
        self.ctx
            .sql(sql)
            .await
            .map_err(|error| anyhow!("failed to query object metadata: {error}"))
    }

    pub async fn sql_and_decode(&self, sql: &str) -> Result<Stream<Value>>
    where
        Value: 'static + Send + DeserializeOwned,
    {
        self.sql(sql)
            .await?
            .try_into_decoder()
            .await
            .map_err(|error| anyhow!("failed to decode object metadata: {error}"))
    }
}
