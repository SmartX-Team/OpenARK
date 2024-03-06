use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use deltalake::datafusion::{physical_plan::execute_stream, prelude::DataFrame};
use serde::de::DeserializeOwned;
use tracing::{instrument, Level};

use crate::schema::arrow::decoder::{DatasetRecordBatchStream, TryIntoTableDecoder};

#[async_trait]
impl TryIntoTableDecoder for DataFrame {
    type Output<T> = crate::storage::Stream<T>;

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn try_into_decoder<T>(self) -> Result<<Self as TryIntoTableDecoder>::Output<T>>
    where
        T: 'static + Send + DeserializeOwned,
    {
        let task_ctx = Arc::new(self.task_ctx());
        let plan = self.create_physical_plan().await?;

        DatasetRecordBatchStream(execute_stream(plan, task_ctx)?)
            .try_into_decoder()
            .await
    }
}
