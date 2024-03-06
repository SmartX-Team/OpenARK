use anyhow::Result;
use async_trait::async_trait;
use lancedb::query::Query;
use serde::de::DeserializeOwned;
use tracing::{instrument, Level};

use crate::schema::arrow::decoder::{DatasetRecordBatchStream, TryIntoTableDecoder};

#[async_trait]
impl TryIntoTableDecoder for Query {
    type Output<T> = crate::storage::Stream<T>;

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn try_into_decoder<T>(self) -> Result<<Self as TryIntoTableDecoder>::Output<T>>
    where
        T: 'static + Send + DeserializeOwned,
    {
        DatasetRecordBatchStream(self.execute_stream().await?)
            .try_into_decoder()
            .await
    }
}
