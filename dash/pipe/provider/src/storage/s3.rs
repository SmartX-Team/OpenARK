use std::sync::Arc;

use anyhow::{anyhow, bail, Error, Result};
use async_trait::async_trait;
use bytes::Bytes;
use chrono::{SecondsFormat, Utc};
use deltalake::{storage::DeltaObjectStore, ObjectStore, Path};
use futures::TryFutureExt;
use tracing::debug;

use crate::message::Name;

use super::lakehouse::{parse_table_uri, StorageBackend};

#[derive(Clone)]
pub struct Storage {
    backend: Arc<DeltaObjectStore>,
    model: Option<Name>,
    pipe_name: Name,
    pipe_timestamp: String,
}

impl Storage {
    pub async fn try_new(
        args: &super::StorageS3Args,
        model: Option<&Name>,
        pipe_name: &Name,
    ) -> Result<Self> {
        debug!("Initializing Storage Set ({model:?}) - S3");

        Ok(Self {
            backend: StorageBackend::try_new_table(
                args,
                Some(
                    &model
                        .cloned()
                        .unwrap_or_else(|| "placeholder".parse().unwrap()),
                ),
            )
            .await?
            .unwrap()
            .object_store(),
            model: model.cloned(),
            pipe_name: pipe_name.clone(),
            pipe_timestamp: Utc::now()
                .to_rfc3339_opts(SecondsFormat::Nanos, true)
                .replace(':', "-"),
        })
    }
}

#[async_trait]
impl super::Storage for Storage {
    fn model(&self) -> Option<&Name> {
        self.model.as_ref()
    }

    fn storage_type(&self) -> super::StorageType {
        super::StorageType::S3
    }

    async fn get(&self, model: &Name, path: &str) -> Result<Bytes> {
        let path = parse_path(path)?;
        let table_uri = parse_table_uri(model, super::name::KIND_STORAGE)?;

        let backend = DeltaObjectStore::new(self.backend.storage_backend(), table_uri);
        backend
            .get(&path)
            .map_err(Error::from)
            .and_then(|object| async move {
                match object.bytes().await {
                    Ok(bytes) => Ok(bytes),
                    Err(error) => {
                        bail!("failed to get object data from S3 object store: {error}")
                    }
                }
            })
            .await
            .map_err(|error| anyhow!("failed to get object from S3 object store: {error}"))
    }

    async fn put(&self, path: &str, bytes: Bytes) -> Result<String> {
        let path = format!(
            "{kind}/{prefix}/{timestamp}/{path}",
            kind = super::name::KIND_STORAGE,
            prefix = &self.pipe_name,
            timestamp = &self.pipe_timestamp,
        );

        self.backend
            .put(&parse_path(&path)?, bytes)
            .await
            .map(|_| path)
            .map_err(|error| anyhow!("failed to put object into S3 object store: {error}"))
    }
}

fn parse_path(path: impl AsRef<str>) -> Result<Path> {
    Path::parse(path).map_err(|error| anyhow!("failed to parse path as S3 style: {error}"))
}
