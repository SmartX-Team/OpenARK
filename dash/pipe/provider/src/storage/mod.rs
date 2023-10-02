mod lakehouse;
mod nats;
mod s3;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use bytes::Bytes;
use clap::Parser;
use deltalake::Path;
use serde::{Deserialize, Serialize};

pub struct StorageSet {
    default_output: StorageType,
    lakehouse: self::lakehouse::Storage,
    nats: self::nats::Storage,
    s3: self::s3::Storage,
}

impl StorageSet {
    pub async fn try_new(
        args: &StorageArgs,
        client: &::nats::Client,
        default_output: StorageType,
    ) -> Result<Self> {
        Ok(Self {
            default_output,
            lakehouse: self::lakehouse::Storage::try_new(&args.s3, &args.bucket_name).await?,
            nats: self::nats::Storage::try_new(&args.nats, client, &args.bucket_name).await?,
            s3: self::s3::Storage::try_new(&args.s3, &args.bucket_name).await?,
        })
    }

    pub const fn get(&self, type_: StorageType) -> &(dyn Send + Sync + Storage) {
        match type_ {
            StorageType::LakeHouse => &self.lakehouse,
            StorageType::Nats => &self.nats,
            StorageType::S3 => &self.s3,
        }
    }

    pub const fn get_default_output(&self) -> &(dyn Send + Sync + Storage) {
        self.get(self.default_output)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum StorageType {
    LakeHouse,
    Nats,
    S3,
}

#[async_trait]
pub trait Storage {
    fn storage_type(&self) -> StorageType;

    async fn get(&self, path: &Path) -> Result<Bytes>;

    async fn get_with_str(&self, path: &str) -> Result<Bytes> {
        self.get(&parse_path(path)?).await
    }

    async fn put(&self, path: &Path, bytes: Bytes) -> Result<()>;

    async fn put_with_str(&self, path: &str, bytes: Bytes) -> Result<()> {
        self.put(&parse_path(path)?, bytes).await
    }

    async fn delete(&self, path: &Path) -> Result<()>;
}

#[derive(Clone, Debug, Parser, Serialize, Deserialize)]
pub struct StorageArgs {
    #[arg(long, env = "BUCKET", value_name = "NAME")]
    bucket_name: String,

    #[command(flatten)]
    nats: self::nats::StorageNatsArgs,

    #[command(flatten)]
    s3: self::s3::StorageS3Args,
}

fn parse_path(path: impl AsRef<str>) -> Result<Path> {
    Path::parse(path).map_err(|error| anyhow!("failed to parse storage path: {error}"))
}
