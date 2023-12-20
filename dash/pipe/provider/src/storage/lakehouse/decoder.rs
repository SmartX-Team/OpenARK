use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_stream::try_stream;
use async_trait::async_trait;
use deltalake::{
    arrow::json::ArrayWriter,
    datafusion::{physical_plan::execute_stream, prelude::DataFrame},
};
use futures::{Stream, StreamExt, TryStreamExt};
use serde::de::DeserializeOwned;
use tracing::{instrument, Level};

#[async_trait]
pub trait TryIntoTableDecoder {
    type Output<T>: Send + Stream<Item = Result<T>>;

    async fn try_into_decoder<T>(self) -> Result<<Self as TryIntoTableDecoder>::Output<T>>
    where
        T: 'static + Send + DeserializeOwned;
}

#[async_trait]
impl TryIntoTableDecoder for DataFrame {
    type Output<T> = super::super::Stream<T>;

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn try_into_decoder<T>(self) -> Result<<Self as TryIntoTableDecoder>::Output<T>>
    where
        T: 'static + Send + DeserializeOwned,
    {
        let task_ctx = Arc::new(self.task_ctx());
        let plan = self.create_physical_plan().await?;

        let mut stream = execute_stream(plan, task_ctx)?;
        Ok(try_stream! {
            while let Some(batch) = stream.try_next().await? {
                let mut writer = ArrayWriter::new(vec![]);
                writer.write(&batch)?;
                writer.finish()?;

                let data = writer.into_inner();
                if !data.is_empty() {
                    for value in ::serde_json::from_reader::<_, Vec<T>>(&*data)
                        .map_err(|error| anyhow!("failed to convert data: {error}"))?
                    {
                        yield value;
                    }
                }
            }
        }
        .boxed())
    }
}
