#[cfg(feature = "lakehouse")]
mod lakehouse;
#[cfg(feature = "nats-storage")]
mod nats;
#[cfg(feature = "s3")]
mod s3;

use std::pin::Pin;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use bytes::Bytes;
use clap::Parser;
use deltalake::Path;
use serde::{Deserialize, Serialize};

pub struct StorageSet {
    default_output: StorageType,
    #[cfg(feature = "lakehouse")]
    lakehouse: self::lakehouse::Storage,
    #[cfg(feature = "nats-storage")]
    nats: self::nats::Storage,
    #[cfg(feature = "s3")]
    s3: self::s3::Storage,
}

impl StorageSet {
    pub async fn try_new(
        args: &StorageArgs,
        #[cfg_attr(not(feature = "nats-storage"), allow(unused_variables))] client: &::nats::Client,
        default_output: StorageType,
    ) -> Result<Self> {
        Ok(Self {
            default_output,
            #[cfg(feature = "lakehouse")]
            lakehouse: self::lakehouse::Storage::try_new(&args.s3, &args.bucket_name).await?,
            #[cfg(feature = "nats-storage")]
            nats: self::nats::Storage::try_new(&args.nats, client, &args.bucket_name).await?,
            #[cfg(feature = "s3")]
            s3: self::s3::Storage::try_new(&args.s3, &args.bucket_name).await?,
        })
    }

    pub const fn get(&self, type_: StorageType) -> &(dyn Send + Sync + Storage) {
        match type_ {
            #[cfg(feature = "lakehouse")]
            StorageType::LakeHouse => &self.lakehouse,
            #[cfg(feature = "nats-storage")]
            StorageType::Nats => &self.nats,
            #[cfg(feature = "s3")]
            StorageType::S3 => &self.s3,
        }
    }

    pub const fn get_default_output(&self) -> &(dyn Send + Sync + Storage) {
        self.get(self.default_output)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum StorageType {
    #[cfg(feature = "lakehouse")]
    LakeHouse,
    #[cfg(feature = "nats-storage")]
    Nats,
    #[cfg(feature = "s3")]
    S3,
}

impl StorageType {
    #[cfg(feature = "nats-storage")]
    pub const TEMPORARY: Self = Self::Nats;
    #[cfg(all(not(feature = "nats-storage"), feature = "s3"))]
    pub const TEMPORARY: Self = Self::S3;
    #[cfg(all(not(feature = "nats-storage"), not(feature = "s3")))]
    pub const TEMPORARY: Self = Self::LakeHouse;

    #[cfg(feature = "s3")]
    pub const PERSISTENT: Self = Self::S3;
    #[cfg(all(not(feature = "s3"), feature = "lakehouse"))]
    pub const PERSISTENT: Self = Self::LakeHouse;
    #[cfg(all(not(feature = "s3"), not(feature = "lakehouse")))]
    pub const PERSISTENT: Self = Self::Nats;
}

#[async_trait]
pub trait Storage {
    fn storage_type(&self) -> StorageType;

    async fn list(&self) -> Result<Stream>;

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

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct StorageArgs {
    #[arg(long, env = "BUCKET", value_name = "NAME")]
    bucket_name: String,

    #[cfg(feature = "nats-storage")]
    #[command(flatten)]
    nats: self::nats::StorageNatsArgs,

    #[cfg(any(feature = "lakehouse", feature = "s3"))]
    #[command(flatten)]
    s3: StorageS3Args,
}

#[cfg(any(feature = "lakehouse", feature = "s3"))]
#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
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
    pub(super) s3_endpoint: ::url::Url,

    #[arg(long, env = "AWS_SECRET_ACCESS_KEY", value_name = "VALUE")]
    pub(super) secret_key: String,
}

pub type Stream = Pin<Box<dyn ::futures::Stream<Item = Result<(Path, Bytes)>>>>;

fn parse_path(path: impl AsRef<str>) -> Result<Path> {
    Path::parse(path).map_err(|error| anyhow!("failed to parse storage path: {error}"))
}
