#[cfg(feature = "lakehouse")]
pub mod lakehouse;
#[cfg(feature = "s3")]
pub mod s3;

use std::{marker::PhantomData, pin::Pin, sync::Arc};

use anyhow::{anyhow, Result};
use async_stream::try_stream;
use async_trait::async_trait;
use bytes::Bytes;
use clap::{ArgAction, Parser};
use futures::{StreamExt, TryStreamExt};
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use strum::{Display, EnumString};
use tracing::debug;
use url::Url;

use crate::{
    function::FunctionContext,
    message::{Name, PipeMessage},
};

pub struct StorageIO {
    pub input: Arc<StorageSet>,
    pub output: Arc<StorageSet>,
}

impl StorageIO {
    pub(crate) async fn flush(&self) -> Result<()> {
        self.input.flush().await?;
        self.output.flush().await?;
        Ok(())
    }
}

pub struct StorageSet {
    default: StorageType,
    default_metadata: MetadataStorageType,
    #[cfg(feature = "lakehouse")]
    lakehouse: self::lakehouse::Storage,
    #[cfg(feature = "s3")]
    s3: self::s3::Storage,
}

impl StorageSet {
    pub async fn try_new<Value>(
        args: &StorageArgs,
        ctx: &mut FunctionContext,
        model: Option<&Name>,
        default_metadata: MetadataStorageArgs<Value>,
    ) -> Result<Self>
    where
        Value: Default + JsonSchema,
    {
        debug!("Initializing Storage Set ({model:?})");

        if !args.persistence_metadata.unwrap_or_default() {
            ctx.disable_store_metadata();
        }

        let default = match args.persistence {
            Some(true) => StorageType::PERSISTENT,
            Some(false) | None => StorageType::TEMPORARY,
        };
        let pipe_name = args
            .pipe_name
            .clone()
            .or_else(|| {
                ::gethostname::gethostname()
                    .to_str()
                    .and_then(|hostname| hostname.parse().ok())
            })
            .ok_or_else(|| anyhow!("failed to get/parse pipe name; you may set environment variable \"PIPE_NAME\" manually"))?;

        Ok(Self {
            default,
            default_metadata: default_metadata.default_storage,
            #[cfg(feature = "lakehouse")]
            lakehouse: self::lakehouse::Storage::try_new::<Value>(&args.s3, model).await?,
            #[cfg(feature = "s3")]
            s3: self::s3::Storage::try_new(&args.s3, model, &pipe_name).await?,
        })
    }

    pub const fn get(&self, storage_type: StorageType) -> &(dyn Send + Sync + Storage) {
        match storage_type {
            #[cfg(feature = "s3")]
            StorageType::S3 => &self.s3,
        }
    }

    pub const fn get_metadata<Value>(
        &self,
        storage_type: MetadataStorageType,
    ) -> &(dyn Send + Sync + MetadataStorage<Value>) {
        match storage_type {
            #[cfg(feature = "lakehouse")]
            MetadataStorageType::LakeHouse => &self.lakehouse,
        }
    }

    pub const fn get_default(&self) -> &(dyn Send + Sync + Storage) {
        self.get(self.default)
    }

    pub const fn get_default_metadata<Value>(&self) -> &(dyn Send + Sync + MetadataStorage<Value>) {
        self.get_metadata(self.default_metadata)
    }

    async fn flush(&self) -> Result<()> {
        #[cfg(feature = "lakehouse")]
        (&self.lakehouse as &(dyn Sync + MetadataStorage))
            .flush()
            .await?;

        Ok(())
    }
}

pub struct MetadataStorageArgs<T> {
    default_storage: MetadataStorageType,
    _type: PhantomData<T>,
}

impl<T> MetadataStorageArgs<T> {
    pub const fn new(default_storage: MetadataStorageType) -> Self {
        Self {
            default_storage,
            _type: PhantomData,
        }
    }
}

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    EnumString,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
pub enum MetadataStorageType {
    #[cfg(feature = "lakehouse")]
    #[default]
    LakeHouse,
}

#[async_trait]
pub trait MetadataStorageExt<Value> {
    async fn list(&self, storage: &Arc<StorageSet>) -> Result<Stream<PipeMessage<Value>>>
    where
        Value: 'static + Send + Default + DeserializeOwned;

    async fn list_as_empty(&self) -> Result<Stream<PipeMessage<Value>>>
    where
        Value: 'static + Send + Default + DeserializeOwned;
}

#[async_trait]
impl<T, Value> MetadataStorageExt<Value> for T
where
    T: ?Sized + Send + Sync + MetadataStorage<Value>,
{
    async fn list(&self, storage: &Arc<StorageSet>) -> Result<Stream<PipeMessage<Value>>>
    where
        Value: 'static + Send + Default + DeserializeOwned,
    {
        let mut list = self.list_metadata().await?;

        let storage = storage.clone();
        Ok(try_stream! {
            while let Some(message) = list.try_next().await? {
                yield message.load_payloads(&storage).await?;
            }
        }
        .boxed())
    }

    async fn list_as_empty(&self) -> Result<Stream<PipeMessage<Value>>>
    where
        Value: 'static + Send + Default + DeserializeOwned,
    {
        let mut list = self.list_metadata().await?;
        Ok(try_stream! {
            while let Some(message) = list.try_next().await? {
                yield message.load_payloads_as_empty();
            }
        }
        .boxed())
    }
}

#[async_trait]
pub trait MetadataStorage<Value = ()> {
    fn table_name(&self) -> &str;

    async fn list_metadata(&self) -> Result<Stream<PipeMessage<Value, ()>>>
    where
        Value: 'static + Send + Default + DeserializeOwned;

    async fn put_metadata(&self, values: &[&PipeMessage<Value, ()>]) -> Result<()>
    where
        Value: 'async_trait + Send + Sync + Default + Serialize + JsonSchema;

    async fn flush(&self) -> Result<()>;
}

#[async_trait]
pub trait MetadataStorageMut<Value = ()> {
    fn table_name(&self) -> &str;

    async fn list_metadata(&mut self) -> Result<Stream<PipeMessage<Value, ()>>>
    where
        Value: 'static + Send + Default + DeserializeOwned;

    async fn put_metadata(&mut self, values: &[&PipeMessage<Value, ()>]) -> Result<()>
    where
        Value: 'async_trait + Send + Sync + Default + Serialize + JsonSchema;

    async fn flush(&mut self) -> Result<()>;
}

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    EnumString,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
pub enum StorageType {
    #[cfg(feature = "s3")]
    S3,
}

impl StorageType {
    #[cfg(feature = "s3")]
    pub const TEMPORARY: Self = Self::S3;

    #[cfg(feature = "s3")]
    pub const PERSISTENT: Self = Self::S3;
}

#[async_trait]
pub trait Storage {
    fn model(&self) -> Option<&Name>;

    fn storage_type(&self) -> StorageType;

    async fn get(&self, model: &Name, path: &str) -> Result<Bytes>;

    async fn put(&self, path: &str, bytes: Bytes) -> Result<String>;
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct StorageArgs {
    #[arg(long, env = "PIPE_PERSISTENCE", action = ArgAction::SetTrue)]
    #[serde(default)]
    persistence: Option<bool>,

    #[arg(long, env = "PIPE_PERSISTENCE_METADATA", action = ArgAction::SetTrue)]
    #[serde(default)]
    persistence_metadata: Option<bool>,

    #[arg(long, env = "PIPE_NAME", value_name = "NAME")]
    pipe_name: Option<Name>,

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
    pub(super) s3_endpoint: Url,

    #[arg(long, env = "AWS_SECRET_ACCESS_KEY", value_name = "VALUE")]
    pub(super) secret_key: String,
}

pub type Stream<T> = Pin<Box<dyn Send + ::futures::Stream<Item = Result<T>>>>;

mod name {
    pub const KIND_METADATA: &str = "metadata";
    pub const KIND_STORAGE: &str = "payloads";
}
