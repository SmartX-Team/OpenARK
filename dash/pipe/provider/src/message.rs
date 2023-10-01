use anyhow::{bail, Error, Result};
use bytes::Bytes;
use futures::future::try_join_all;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::storage::{StorageSet, StorageType};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PipeMessages<Value = (), Payload = Bytes>
where
    Payload: Default,
{
    None,
    Single(PipeMessage<Value, Payload>),
    Batch(Vec<PipeMessage<Value, Payload>>),
}

impl<Value> PipeMessages<Value> {
    pub async fn dump_payloads(self, storage: &StorageSet) -> Result<PipeMessages<Value, ()>> {
        match self {
            Self::None => Ok(PipeMessages::None),
            Self::Single(value) => value.dump_payloads(storage).await.map(PipeMessages::Single),
            Self::Batch(values) => {
                try_join_all(values.into_iter().map(|value| value.dump_payloads(storage)))
                    .await
                    .map(PipeMessages::Batch)
            }
        }
    }
}

impl<Value, Payload> PipeMessages<Value, Payload>
where
    Payload: Default,
{
    pub async fn load_payloads(self, storage: &StorageSet) -> Result<PipeMessages<Value>> {
        match self {
            Self::None => Ok(PipeMessages::None),
            Self::Single(value) => value.load_payloads(storage).await.map(PipeMessages::Single),
            Self::Batch(values) => {
                try_join_all(values.into_iter().map(|value| value.load_payloads(storage)))
                    .await
                    .map(PipeMessages::Batch)
            }
        }
    }

    pub fn into_vec(self) -> Vec<PipeMessage<Value, Payload>> {
        match self {
            Self::None => Default::default(),
            Self::Single(value) => vec![value],
            Self::Batch(values) => values,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PipeMessage<Value, Payload = Bytes> {
    #[serde(default)]
    pub payloads: Vec<PipePayload<Payload>>,
    pub value: Value,
}

impl<Value> TryFrom<Bytes> for PipeMessage<Value, ()>
where
    Value: DeserializeOwned,
{
    type Error = Error;

    fn try_from(value: Bytes) -> Result<Self> {
        ::serde_json::from_reader(&*value).map_err(Into::into)
    }
}

impl<Value> TryFrom<::nats::Message> for PipeMessage<Value, ()>
where
    Value: DeserializeOwned,
{
    type Error = Error;

    fn try_from(message: ::nats::Message) -> Result<Self> {
        message.payload.try_into()
    }
}

impl<Value, Payload> TryFrom<PipeMessage<Value, Payload>> for Bytes
where
    Payload: Serialize,
    Value: Serialize,
{
    type Error = Error;

    fn try_from(value: PipeMessage<Value, Payload>) -> Result<Self> {
        ::serde_json::to_vec(&value)
            .map(Into::into)
            .map_err(Into::into)
    }
}

impl<Value> PipeMessage<Value> {
    pub async fn dump_payloads(self, storage: &StorageSet) -> Result<PipeMessage<Value, ()>> {
        Ok(PipeMessage {
            payloads: try_join_all(
                self.payloads
                    .into_iter()
                    .map(|payload| payload.dump(storage)),
            )
            .await?,
            value: self.value,
        })
    }
}

impl<Value, Payload> PipeMessage<Value, Payload> {
    pub async fn load_payloads(self, storage: &StorageSet) -> Result<PipeMessage<Value>> {
        Ok(PipeMessage {
            payloads: try_join_all(
                self.payloads
                    .into_iter()
                    .map(|payload| payload.load(storage)),
            )
            .await?,
            value: self.value,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PipePayload<Value = Bytes> {
    pub key: String,
    #[serde(default)]
    pub storage: Option<StorageType>,
    #[serde(skip)]
    pub value: Value,
}

impl PipePayload {
    pub async fn dump(self, storage: &StorageSet) -> Result<PipePayload<()>> {
        Ok(PipePayload {
            value: storage
                .get_default_output()
                .put_with_str(&self.key, self.value)
                .await?,
            key: self.key,
            storage: Some(storage.get_default_output().storage_type()),
        })
    }
}

impl<Value> PipePayload<Value> {
    pub async fn load(self, storage: &StorageSet) -> Result<PipePayload> {
        Ok(PipePayload {
            value: match self.storage {
                Some(type_) => storage.get(type_).get_with_str(&self.key).await?,
                None => bail!("storage type not defined"),
            },
            key: self.key,
            storage: self.storage,
        })
    }
}
