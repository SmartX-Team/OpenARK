use anyhow::{anyhow, bail, Error, Result};
use ark_core_k8s::data::Name;
use async_trait::async_trait;
use bytes::Bytes;
use chrono::{SecondsFormat, Utc};
use dash_pipe_api::storage::StorageS3Args;
use futures::TryFutureExt;
use minio::s3::{
    args::{GetObjectArgs, PutObjectApiArgs, RemoveObjectArgs},
    client::Client,
    creds::StaticProvider,
    http::BaseUrl,
};
use tracing::debug;

#[derive(Clone)]
pub struct Storage {
    base_url: BaseUrl,
    model: Option<Name>,
    pipe_name: Name,
    pipe_timestamp: String,
    provider: StaticProvider,
}

impl Storage {
    pub async fn try_new(
        StorageS3Args {
            access_key,
            region: _,
            s3_endpoint,
            secret_key,
        }: &StorageS3Args,
        model: Option<&Name>,
        pipe_name: &Name,
    ) -> Result<Self> {
        debug!("Initializing Storage Set ({model:?}) - S3");
        Ok(Self {
            base_url: BaseUrl::from_string(s3_endpoint.as_str().into())
                .map_err(|error| anyhow!("failed to parse s3 storage endpoint: {error}"))?,
            model: model.cloned(),
            pipe_name: pipe_name.clone(),
            pipe_timestamp: Utc::now()
                .to_rfc3339_opts(SecondsFormat::Nanos, true)
                .replace(':', "-"),
            provider: StaticProvider::new(access_key, secret_key, None),
        })
    }
}

impl Storage {
    fn bucket_name(&self) -> Result<&str> {
        match self.model.as_ref().map(|model| model.storage()) {
            Some(model) => Ok(model),
            None => bail!("s3 storage is not inited"),
        }
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
        let bucket_name = model.storage();
        let args = GetObjectArgs::new(bucket_name, path)?;

        Client::new(self.base_url.clone(), Some(&self.provider))
            .get_object(&args)
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
        let bucket_name = self.bucket_name()?;
        let path = format!(
            "{kind}/{prefix}/{timestamp}/{path}",
            kind = super::name::KIND_STORAGE,
            prefix = &self.pipe_name,
            timestamp = &self.pipe_timestamp,
        );
        let args = PutObjectApiArgs::new(bucket_name, &path, &bytes)?;

        Client::new(self.base_url.clone(), Some(&self.provider))
            .put_object_api(&args)
            .await
            .map(|_| path)
            .map_err(|error| anyhow!("failed to put object into S3 object store: {error}"))
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let bucket_name = self.bucket_name()?;
        let args = RemoveObjectArgs::new(bucket_name, path)?;

        Client::new(self.base_url.clone(), Some(&self.provider))
            .remove_object(&args)
            .await
            .map(|_| ())
            .map_err(|error| anyhow!("failed to delete object from S3 object store: {error}"))
    }
}
