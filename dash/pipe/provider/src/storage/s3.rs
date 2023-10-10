use anyhow::{anyhow, bail, Error, Result};
use async_trait::async_trait;
use bytes::Bytes;
use futures::TryFutureExt;
use minio::s3::{
    args::{GetObjectArgs, PutObjectApiArgs, RemoveObjectArgs},
    client::Client,
    creds::StaticProvider,
    http::BaseUrl,
};

use crate::message::ModelRef;

#[derive(Clone)]
pub struct Storage {
    base_url: BaseUrl,
    bind: Option<ModelRef>,
    provider: StaticProvider,
}

impl Storage {
    pub async fn try_new(
        super::StorageS3Args {
            access_key,
            region: _,
            s3_endpoint,
            secret_key,
        }: &super::StorageS3Args,
        bind: Option<&ModelRef>,
    ) -> Result<Self> {
        Ok(Self {
            base_url: BaseUrl::from_string(s3_endpoint.as_str().into())
                .map_err(|error| anyhow!("failed to parse s3 storage endpoint: {error}"))?,
            bind: bind.cloned(),
            provider: StaticProvider::new(access_key, secret_key, None),
        })
    }
}

impl Storage {
    fn bucket_name(&self) -> Result<&str> {
        match self.bind.as_deref() {
            Some(bind) => Ok(bind),
            None => bail!("s3 storage is not inited"),
        }
    }
}

#[async_trait]
impl super::Storage for Storage {
    fn model(&self) -> Option<&ModelRef> {
        self.bind.as_ref()
    }

    fn storage_type(&self) -> super::StorageType {
        super::StorageType::S3
    }

    async fn get(&self, bind: &ModelRef, path: &str) -> Result<Bytes> {
        let bucket_name = bind.as_str();
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

    async fn put(&self, path: &str, bytes: Bytes) -> Result<()> {
        let bucket_name = self.bucket_name()?;
        let args = PutObjectApiArgs::new(bucket_name, path, &bytes)?;

        Client::new(self.base_url.clone(), Some(&self.provider))
            .put_object_api(&args)
            .await
            .map(|_| ())
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
