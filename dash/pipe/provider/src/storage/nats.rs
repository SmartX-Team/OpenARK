use std::io::Cursor;

use anyhow::{anyhow, bail, Error, Result};
use async_stream::try_stream;
use async_trait::async_trait;
use bytes::Bytes;
use clap::Parser;
use deltalake::Path;
use futures::{StreamExt, TryFutureExt};
use nats::jetstream::object_store::ObjectStore;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;

#[derive(Clone)]
pub struct Storage {
    store: ObjectStore,
}

impl Storage {
    pub async fn try_new(
        StorageNatsArgs {}: &StorageNatsArgs,
        client: &::nats::Client,
        bucket_name: &str,
    ) -> Result<Self> {
        Ok(Self {
            store: {
                let context = ::nats::jetstream::new(client.clone());
                context
                    .get_object_store(bucket_name)
                    .await
                    .map_err(|error| anyhow!("failed to init NATS object store: {error}"))?
            },
        })
    }
}

#[async_trait]
impl super::Storage for Storage {
    fn storage_type(&self) -> super::StorageType {
        super::StorageType::Nats
    }

    async fn list(&self) -> Result<super::Stream> {
        let storage = self.clone();
        Ok(try_stream! {
            let mut list = storage.store.list()
                .map_err(|error| anyhow!("failed to list objects from NATS object store: {error}"))
                .await?;
            while let Some(item) = list.next().await
            {
                if let Ok(path) = item
                    .map_err(Into::into)
                    .and_then(|item| super::parse_path(item.name))
                {
                    let value = storage.get(&path).await?;
                    yield (path, value);
                }
            }
        }
        .boxed())
    }

    async fn get(&self, path: &Path) -> Result<Bytes> {
        self.store
            .get(path.as_ref())
            .map_err(Error::from)
            .and_then(|mut object| async move {
                let mut buf = Vec::with_capacity(object.info().size);
                match object.read_to_end(&mut buf).await {
                    Ok(_) => Ok(buf.into()),
                    Err(error) => {
                        bail!("failed to get object data from NATS object store: {error}")
                    }
                }
            })
            .await
            .map_err(|error| anyhow!("failed to get object from NATS object store: {error}"))
    }

    async fn put(&self, path: &Path, bytes: Bytes) -> Result<()> {
        self.store
            .put(path.as_ref(), &mut Cursor::new(bytes))
            .await
            .map(|_| ())
            .map_err(|error| anyhow!("failed to put object into NATS object store: {error}"))
    }

    async fn delete(&self, path: &Path) -> Result<()> {
        self.store
            .delete(path.as_ref())
            .await
            .map(|_| ())
            .map_err(|error| anyhow!("failed to delete object from NATS object store: {error}"))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct StorageNatsArgs {}
