#[cfg(feature = "lakehouse")]
pub mod lakehouse;
#[cfg(feature = "s3")]
pub mod s3;

use std::{marker::PhantomData, pin::Pin, sync::Arc, time::Duration};

use anyhow::{anyhow, Result};
use ark_core_k8s::data::Name;
use async_stream::try_stream;
use async_trait::async_trait;
use bytes::Bytes;
use clap::{ArgAction, Parser};
use futures::{StreamExt, TryStreamExt};
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use strum::{Display, EnumString};
use tracing::{debug, instrument, Level};

use crate::{function::FunctionContext, message::PipeMessage};

pub struct StorageIO {
    pub input: Arc<StorageSet>,
    pub output: Arc<StorageSet>,
}

impl StorageIO {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub(crate) async fn flush(&self) -> Result<()> {
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
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn try_new<Value>(
        args: &StorageArgs,
        ctx: &mut FunctionContext,
        model: Option<&Name>,
        default_metadata: MetadataStorageArgs<Value>,
    ) -> Result<Self>
    where
        Value: JsonSchema,
    {
        debug!("Initializing Storage Set ({model:?})");

        let persistence_metadata = args.persistence_metadata.unwrap_or_default();
        if !persistence_metadata {
            ctx.disable_store_metadata();
        }

        let default = match args.persistence {
            Some(true) => StorageType::PERSISTENT,
            Some(false) | None => StorageType::TEMPORARY,
        };
        let namespace = || ctx.namespace().to_string();
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
            lakehouse: if persistence_metadata {
                // TODO: to be implemented!
                let flush = if ctx.is_disabled_store_metadata() {
                    None
                } else {
                    Some(Duration::from_secs(10))
                };
                self::lakehouse::Storage::try_new::<Value>(&args.s3, namespace(), model, flush)
                    .await?
            } else {
                self::lakehouse::Storage::default()
            },
            #[cfg(feature = "s3")]
            s3: self::s3::Storage::try_new(&args.s3, namespace(), model, &pipe_name)?,
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

    #[instrument(level = Level::INFO, skip_all, err(Display))]
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
        Value: 'static + Send + DeserializeOwned;

    async fn list_as_empty(&self) -> Result<Stream<PipeMessage<Value>>>
    where
        Value: 'static + Send + DeserializeOwned;
}

#[async_trait]
impl<T, Value> MetadataStorageExt<Value> for T
where
    T: ?Sized + Send + Sync + MetadataStorage<Value>,
{
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn list(&self, storage: &Arc<StorageSet>) -> Result<Stream<PipeMessage<Value>>>
    where
        Value: 'static + Send + DeserializeOwned,
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

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn list_as_empty(&self) -> Result<Stream<PipeMessage<Value>>>
    where
        Value: 'static + Send + DeserializeOwned,
    {
        let mut list = self.list_metadata().await?;
        Ok(try_stream! {
            while let Some(message) = list.try_next().await? {
                yield message.drop_payloads();
            }
        }
        .boxed())
    }
}

#[async_trait]
pub trait MetadataStorage<Value = ()> {
    async fn list_metadata(&self) -> Result<Stream<PipeMessage<Value, ()>>>
    where
        Value: 'static + Send + DeserializeOwned;

    async fn put_metadata(&self, values: &[&PipeMessage<Value, ()>]) -> Result<()>
    where
        Value: 'async_trait + Send + Sync + Serialize + JsonSchema;

    async fn flush(&self) -> Result<()>;
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

    async fn delete(&self, path: &str) -> Result<()>;
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
    s3: ::dash_pipe_api::storage::StorageS3Args,
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct DummyStorageArgs {}

pub type Stream<T> = Pin<Box<dyn Send + ::futures::Stream<Item = Result<T>>>>;

mod name {
    pub const KIND_METADATA: &str = "metadata";
    pub const KIND_STORAGE: &str = "payloads";
}
