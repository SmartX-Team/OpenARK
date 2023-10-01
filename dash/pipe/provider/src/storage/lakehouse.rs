use std::collections::HashMap;

use anyhow::{anyhow, bail, Error, Result};
use async_trait::async_trait;
use bytes::Bytes;
use clap::Parser;
use deltalake::{DeltaTable, DeltaTableBuilder, ObjectStore, Path};
use futures::TryFutureExt;
use serde::{Deserialize, Serialize};
use url::Url;

pub struct Storage {
    table: DeltaTable,
}

impl Storage {
    pub async fn try_new(
        StorageLakeHouseArgs {
            access_key,
            deltalake_endpoint,
            region,
            secret_key,
        }: &StorageLakeHouseArgs,
        bucket_name: &str,
    ) -> Result<Self> {
        Ok(Self {
            table: {
                let allow_http = deltalake_endpoint.scheme() == "http";
                let table_uri = format!("s3a://{bucket_name}/");

                let mut backend_config: HashMap<String, String> = HashMap::new();
                backend_config.insert("AWS_ACCESS_KEY_ID".to_string(), access_key.clone());
                backend_config.insert(
                    "AWS_ENDPOINT_URL".to_string(),
                    deltalake_endpoint.to_string(),
                );
                backend_config.insert("AWS_REGION".to_string(), region.clone());
                backend_config.insert("AWS_SECRET_ACCESS_KEY".to_string(), secret_key.clone());
                backend_config.insert("AWS_S3_ALLOW_UNSAFE_RENAME".to_string(), "true".into());

                DeltaTableBuilder::from_uri(table_uri)
                    .with_allow_http(allow_http)
                    .with_storage_options(backend_config)
                    .build()
                    .unwrap()
            },
        })
    }
}

#[async_trait]
impl super::Storage for Storage {
    fn storage_type(&self) -> super::StorageType {
        super::StorageType::LakeHouse
    }

    async fn get(&self, path: &Path) -> Result<Bytes> {
        self.table
            .object_store()
            .get(path)
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
        self.table
            .object_store()
            .put(path, bytes)
            .await
            .map(|_| ())
            .map_err(|error| anyhow!("failed to put object into DeltaLake object store: {error}"))
    }

    async fn delete(&self, path: &Path) -> Result<()> {
        self.table
            .object_store()
            .delete(path)
            .await
            .map(|_| ())
            .map_err(|error| {
                anyhow!("failed to delete object from DeltaLake object store: {error}")
            })
    }
}

#[derive(Clone, Debug, Parser, Serialize, Deserialize)]
pub struct StorageLakeHouseArgs {
    #[arg(long, env = "AWS_ACCESS_KEY_ID", value_name = "VALUE")]
    access_key: String,

    #[arg(long, env = "AWS_ENDPOINT_URL", value_name = "URL")]
    deltalake_endpoint: Url,

    #[arg(
        long,
        env = "AWS_REGION",
        value_name = "REGION",
        default_value = "us-east-1"
    )]
    region: String,

    #[arg(long, env = "AWS_SECRET_ACCESS_KEY", value_name = "VALUE")]
    secret_key: String,
}
