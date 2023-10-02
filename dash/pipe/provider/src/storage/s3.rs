use anyhow::{anyhow, bail, Error, Result};
use async_trait::async_trait;
use bytes::Bytes;
use clap::Parser;
use deltalake::Path;
use futures::TryFutureExt;
use minio::s3::{
    args::{GetObjectArgs, PutObjectApiArgs, RemoveObjectArgs},
    client::Client,
    creds::StaticProvider,
    http::BaseUrl,
};
use serde::{Deserialize, Serialize};
use url::Url;

pub struct Storage {
    base_url: BaseUrl,
    bucket_name: String,
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
        bucket_name: &str,
    ) -> Result<Self> {
        Ok(Self {
            base_url: BaseUrl::from_string(s3_endpoint.as_str().into())
                .map_err(|error| anyhow!("failed to parse s3 storage endpoint: {error}"))?,
            bucket_name: bucket_name.into(),
            provider: StaticProvider::new(access_key, secret_key, None),
        })
    }
}

#[async_trait]
impl super::Storage for Storage {
    fn storage_type(&self) -> super::StorageType {
        super::StorageType::S3
    }

    async fn get(&self, path: &Path) -> Result<Bytes> {
        let args = GetObjectArgs::new(&self.bucket_name, path.as_ref())?;

        Client::new(self.base_url.clone(), Some(&self.provider))
            .get_object(&args)
            .map_err(Error::from)
            .and_then(|object| async move {
                match object.bytes().await {
                    Ok(bytes) => Ok(bytes),
                    Err(error) => {
                        bail!("failed to get object data from DeltaLake object store: {error}")
                    }
                }
            })
            .await
            .map_err(|error| anyhow!("failed to get object from DeltaLake object store: {error}"))
    }

    async fn put(&self, path: &Path, bytes: Bytes) -> Result<()> {
        let args = PutObjectApiArgs::new(&self.bucket_name, path.as_ref(), &bytes)?;

        Client::new(self.base_url.clone(), Some(&self.provider))
            .put_object_api(&args)
            .await
            .map(|_| ())
            .map_err(|error| anyhow!("failed to put object into DeltaLake object store: {error}"))
    }

    async fn delete(&self, path: &Path) -> Result<()> {
        let args = RemoveObjectArgs::new(&self.bucket_name, path.as_ref())?;

        Client::new(self.base_url.clone(), Some(&self.provider))
            .remove_object(&args)
            .await
            .map(|_| ())
            .map_err(|error| {
                anyhow!("failed to delete object from DeltaLake object store: {error}")
            })
    }
}

#[derive(Clone, Debug, Parser, Serialize, Deserialize)]
pub struct StorageS3Args {
    #[arg(long, env = "AWS_ACCESS_KEY_ID", value_name = "VALUE")]
    pub(super) access_key: String,

    #[arg(
        long,
        env = "AWS_REGION",
        value_name = "REGION",
        default_value = "us-east-1"
    )]
    pub(super) region: String,

    #[arg(long, env = "AWS_ENDPOINT_URL", value_name = "URL")]
    pub(super) s3_endpoint: Url,

    #[arg(long, env = "AWS_SECRET_ACCESS_KEY", value_name = "VALUE")]
    pub(super) secret_key: String,
}
