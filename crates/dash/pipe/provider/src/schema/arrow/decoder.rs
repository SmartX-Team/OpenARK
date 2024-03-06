use anyhow::{anyhow, Result};
use arrow::{json::ArrayWriter, record_batch::RecordBatch};
use async_stream::try_stream;
use async_trait::async_trait;
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

pub(crate) struct DatasetRecordBatchStream<S>(pub(crate) S);

#[async_trait]
impl<S, E> TryIntoTableDecoder for DatasetRecordBatchStream<S>
where
    S: 'static + Send + Unpin + Stream<Item = Result<RecordBatch, E>>,
    E: 'static + Send + Sync + ::std::error::Error,
{
    type Output<T> = crate::storage::Stream<T>;

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn try_into_decoder<T>(mut self) -> Result<<Self as TryIntoTableDecoder>::Output<T>>
    where
        T: 'static + Send + DeserializeOwned,
    {
        Ok(try_stream! {
            while let Some(batch) = self.0.try_next().await? {
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
