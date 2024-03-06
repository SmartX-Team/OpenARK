use std::collections::HashMap;

use anyhow::{bail, Error, Result};
use ark_core_k8s::data::Name;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::{stream::FuturesOrdered, StreamExt, TryStreamExt};
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
pub use serde_json::Value as DynValue;
use strum::{Display, EnumString};
use tracing::{instrument, Level};

use crate::storage::{StorageSet, StorageType};

pub type DynMap = serde_json::Map<String, DynValue>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PipeMessages<Value = DynValue, Payload = Bytes>
where
    Payload: JsonSchema,
{
    None,
    Single(PipeMessage<Value, Payload>),
    Batch(Vec<PipeMessage<Value, Payload>>),
}

#[cfg(feature = "pyo3")]
impl From<PipeMessages> for Vec<PyPipeMessage> {
    fn from(value: PipeMessages) -> Self {
        match value {
            PipeMessages::None => Self::default(),
            PipeMessages::Single(value) => {
                vec![value.into()]
            }
            PipeMessages::Batch(values) => values.into_iter().map(Into::into).collect(),
        }
    }
}

impl<Value, Payload> PipeMessages<Value, Payload>
where
    Payload: JsonSchema,
{
    pub(crate) fn as_payloads_map(&self) -> HashMap<String, PipePayload<Payload>>
    where
        Payload: Clone,
    {
        match self {
            Self::None => HashMap::default(),
            Self::Single(value) => value.iter_payloads_map().collect(),
            Self::Batch(values) => values
                .iter()
                .flat_map(|value| value.iter_payloads_map())
                .collect(),
        }
    }

    pub fn into_vec(self) -> Vec<PipeMessage<Value, Payload>> {
        match self {
            Self::None => Vec::default(),
            Self::Single(value) => vec![value],
            Self::Batch(values) => values,
        }
    }

    pub(crate) fn drop_payloads<P>(self) -> PipeMessages<Value, P>
    where
        P: JsonSchema,
    {
        match self {
            Self::None => PipeMessages::None,
            Self::Single(value) => PipeMessages::Single(value.drop_payloads()),
            Self::Batch(values) => PipeMessages::Batch(
                values
                    .into_iter()
                    .map(|value| value.drop_payloads())
                    .collect(),
            ),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::None => true,
            Self::Single(_) => false,
            Self::Batch(values) => values.is_empty(),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::None => 0,
            Self::Single(_) => 1,
            Self::Batch(values) => values.len(),
        }
    }
}

impl<Value> PipeMessages<Value> {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub(crate) async fn dump_payloads(
        self,
        storage: &StorageSet,
        model: Option<&Name>,
        input_payloads: Option<&HashMap<String, PipePayload>>,
    ) -> Result<PipeMessages<Value>> {
        match self {
            Self::None => Ok(PipeMessages::None),
            Self::Single(value) => value
                .dump_payloads(storage, model, input_payloads)
                .await
                .map(PipeMessages::Single),
            Self::Batch(values) => values
                .into_iter()
                .map(|value| value.dump_payloads(storage, model, input_payloads))
                .collect::<FuturesOrdered<_>>()
                .try_collect()
                .await
                .map(PipeMessages::Batch),
        }
    }
}

#[cfg(feature = "pyo3")]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[::pyo3::pyclass]
pub struct PyPipeMessage {
    #[serde(default, rename = "__payloads")]
    payloads: Vec<PipePayload>,
    #[serde(default, flatten, skip_serializing_if = "Option::is_none")]
    reply: Option<PipeReply>,
    #[serde(rename = "__timestamp")]
    timestamp: DateTime<Utc>,
    #[serde(flatten)]
    value: DynValue,
}

#[cfg(feature = "pyo3")]
impl From<PipeMessage> for PyPipeMessage {
    fn from(
        PipeMessage {
            payloads,
            timestamp,
            reply,
            value,
        }: PipeMessage,
    ) -> Self {
        Self {
            payloads,
            timestamp,
            reply,
            value,
        }
    }
}

#[cfg(feature = "pyo3")]
impl From<PyPipeMessage> for PipeMessage {
    fn from(
        PyPipeMessage {
            payloads,
            timestamp,
            reply,
            value,
        }: PyPipeMessage,
    ) -> Self {
        Self {
            payloads,
            timestamp,
            reply,
            value,
        }
    }
}

#[cfg(feature = "pyo3")]
#[::pyo3::pymethods]
impl PyPipeMessage {
    #[new]
    #[pyo3(signature = (payloads, value, reply = None))]
    fn new(
        payloads: Vec<(String, Option<Vec<u8>>)>,
        value: &::pyo3::PyAny,
        reply: Option<(String, Option<String>)>,
    ) -> ::pyo3::PyResult<Self> {
        fn value_to_native(value: &::pyo3::PyAny) -> DynValue {
            if value.is_none() {
                DynValue::Null
            } else if let Ok(value) = value.extract::<bool>() {
                DynValue::Bool(value)
            } else if let Ok(value) = value.extract::<u64>() {
                DynValue::Number(value.into())
            } else if let Ok(value) = value.extract::<i64>() {
                DynValue::Number(value.into())
            } else if let Some(value) = value
                .extract::<f64>()
                .ok()
                .and_then(::serde_json::Number::from_f64)
            {
                DynValue::Number(value)
            } else if let Ok(value) = value.extract::<String>() {
                DynValue::String(value)
            } else if let Ok(values) = value.downcast::<::pyo3::types::PyList>() {
                DynValue::Array(values.iter().map(value_to_native).collect())
            } else if let Ok(values) = value.downcast::<::pyo3::types::PyDict>() {
                DynValue::Object(
                    values
                        .iter()
                        .filter_map(|(key, value)| {
                            key.extract().ok().map(|key| (key, value_to_native(value)))
                        })
                        .collect(),
                )
            } else {
                // do not save the value
                DynValue::Null
            }
        }

        Ok(Self {
            payloads: payloads
                .into_iter()
                .map(|(key, value)| PipePayload::new(key, value.map(Into::into)))
                .collect(),
            reply: reply.map(|(inbox, target)| PipeReply {
                inbox,
                target: target.and_then(|target| target.parse().ok()),
            }),
            timestamp: Utc::now(),
            value: value_to_native(value),
        })
    }

    #[getter]
    fn get_payloads(&self) -> Vec<(&str, Option<&[u8]>)> {
        self.payloads
            .iter()
            .map(
                |PipePayload {
                     key,
                     model: _,
                     path: _,
                     storage: _,
                     value,
                 }| { (key.as_str(), value.as_deref()) },
            )
            .collect()
    }

    #[getter]
    fn get_reply(&self) -> Option<(&str, Option<&str>)> {
        self.reply.as_ref().map(|PipeReply { inbox, target }| {
            (
                inbox.as_str(),
                target.as_ref().map(|target| target.as_str()),
            )
        })
    }

    #[getter]
    fn get_value(&self, py: ::pyo3::Python) -> ::pyo3::PyObject {
        use pyo3::{types::IntoPyDict, IntoPy};

        fn value_to_py(py: ::pyo3::Python, value: &DynValue) -> ::pyo3::PyObject {
            match value {
                DynValue::Null => ().into_py(py),
                DynValue::Bool(value) => value.into_py(py),
                DynValue::Number(value) => match value.as_u64() {
                    Some(value) => value.into_py(py),
                    None => match value.as_i64() {
                        Some(value) => value.into_py(py),
                        None => match value.as_f64() {
                            Some(value) => value.into_py(py),
                            None => {
                                unreachable!("one of the Rust Json Number type should be matched")
                            }
                        },
                    },
                },
                DynValue::String(value) => value.into_py(py),
                DynValue::Array(values) => values
                    .iter()
                    .map(|value| value_to_py(py, value))
                    .collect::<Vec<_>>()
                    .into_py(py),
                DynValue::Object(values) => values
                    .iter()
                    .map(|(key, value)| (key, value_to_py(py, value)))
                    .into_py_dict(py)
                    .into(),
            }
        }

        value_to_py(py, &self.value)
    }

    #[getter]
    fn timestamp(&self) -> String {
        self.timestamp
            .to_rfc3339_opts(::chrono::SecondsFormat::Nanos, true)
    }

    #[classmethod]
    fn from_json(_cls: &pyo3::types::PyType, data: &str) -> ::pyo3::PyResult<String> {
        ::serde_json::from_str(data)
            .map_err(|error| ::pyo3::exceptions::PyException::new_err(error.to_string()))
    }

    fn to_json(&self) -> ::pyo3::PyResult<String> {
        ::serde_json::to_string(self)
            .map_err(|error| ::pyo3::exceptions::PyException::new_err(error.to_string()))
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MaybePipeMessage<Value = DynValue, Payload = Bytes>
where
    Payload: JsonSchema,
{
    #[serde(default, rename = "__payloads", skip_serializing_if = "Vec::is_empty")]
    pub payloads: Vec<PipePayload<Payload>>,
    #[serde(default, flatten, skip_serializing_if = "Option::is_none")]
    pub(crate) reply: Option<PipeReply>,
    #[serde(
        default,
        rename = "__timestamp",
        skip_serializing_if = "Option::is_none"
    )]
    timestamp: Option<DateTime<Utc>>,
    #[serde(flatten)]
    pub value: Value,
}

impl From<MaybePipeMessage> for PipeMessage {
    fn from(message: MaybePipeMessage) -> Self {
        let MaybePipeMessage {
            payloads,
            reply,
            timestamp,
            value,
        } = message;

        Self {
            payloads,
            reply,
            timestamp: timestamp.unwrap_or_else(Utc::now),
            value,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PipeMessage<Value = DynValue, Payload = Bytes>
where
    Payload: JsonSchema,
{
    #[serde(rename = "__payloads")]
    pub payloads: Vec<PipePayload<Payload>>,
    #[serde(default, flatten, skip_serializing_if = "Option::is_none")]
    pub(crate) reply: Option<PipeReply>,
    #[serde(rename = "__timestamp")]
    timestamp: DateTime<Utc>,
    #[serde(flatten)]
    pub value: Value,
}

impl<Value, Payload> TryFrom<&[u8]> for PipeMessage<Value, Payload>
where
    Payload: DeserializeOwned + JsonSchema,
    Value: DeserializeOwned,
{
    type Error = Error;

    fn try_from(value: &[u8]) -> Result<Self> {
        match value.first().copied().map(Into::into) {
            None | Some(OpCode::AsciiEnd) => ::serde_json::from_slice(value).map_err(Into::into),
            Some(OpCode::MessagePack) => ::rmp_serde::from_slice(&value[1..]).map_err(Into::into),
            Some(OpCode::Cbor) => ::serde_cbor::from_slice(&value[1..]).map_err(Into::into),
            Some(OpCode::Unsupported) => bail!("cannot infer serde opcode"),
        }
    }
}

impl<Value, Payload> TryFrom<Bytes> for PipeMessage<Value, Payload>
where
    Payload: DeserializeOwned + JsonSchema,
    Value: DeserializeOwned,
{
    type Error = Error;

    fn try_from(value: Bytes) -> Result<Self> {
        <&[u8]>::try_into(&value)
    }
}

impl<Value, Payload> TryFrom<&str> for PipeMessage<Value, Payload>
where
    Payload: DeserializeOwned + JsonSchema,
    Value: DeserializeOwned,
{
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        ::serde_json::from_str(value).map_err(Into::into)
    }
}

impl<Value, Payload> TryFrom<DynValue> for PipeMessage<Value, Payload>
where
    Payload: DeserializeOwned + JsonSchema,
    Value: DeserializeOwned,
{
    type Error = Error;

    fn try_from(value: DynValue) -> Result<Self> {
        ::serde_json::from_value(value).map_err(Into::into)
    }
}

impl<Value, Payload> TryFrom<&PipeMessage<Value, Payload>> for Bytes
where
    Payload: Serialize + JsonSchema,
    Value: Serialize,
{
    type Error = Error;

    fn try_from(value: &PipeMessage<Value, Payload>) -> Result<Self> {
        value.to_bytes(Codec::default())
    }
}

impl<Value, Payload> TryFrom<&PipeMessage<Value, Payload>> for DynValue
where
    Payload: Serialize + JsonSchema,
    Value: Serialize,
{
    type Error = Error;

    fn try_from(value: &PipeMessage<Value, Payload>) -> Result<Self> {
        ::serde_json::to_value(value).map_err(Into::into)
    }
}

impl<Value, Payload> PipeMessage<Value, Payload>
where
    Payload: JsonSchema,
{
    pub fn new(value: Value) -> Self {
        Self {
            payloads: Vec::default(),
            timestamp: Utc::now(),
            reply: None,
            value,
        }
    }

    pub fn with_payloads(payloads: Vec<PipePayload<Payload>>, value: Value) -> Self {
        Self {
            payloads,
            timestamp: Utc::now(),
            reply: None,
            value,
        }
    }

    pub fn with_request<P, V>(
        request: &PipeMessage<V, P>,
        payloads: Vec<PipePayload<Payload>>,
        value: Value,
    ) -> Self
    where
        P: JsonSchema,
    {
        Self {
            payloads,
            timestamp: Utc::now(),
            reply: request.reply.clone(),
            value,
        }
    }

    pub(crate) fn with_reply_inbox(mut self, inbox: String) -> Self {
        if !inbox.is_empty() {
            self.reply = Some(PipeReply {
                inbox,
                target: None,
            });
            self
        } else {
            self.drop_reply()
        }
    }

    pub(crate) fn with_reply_target(mut self, target: &Option<Name>) -> Self {
        if let Some(reply) = &mut self.reply {
            if reply.target.is_none() {
                reply.target = target.clone();
            }
        }
        self
    }

    pub const fn with_timestamp(mut self, timestamp: DateTime<Utc>) -> Self {
        self.timestamp = timestamp;
        self
    }

    pub(crate) fn drop_reply(mut self) -> Self {
        self.reply = None;
        self
    }

    fn iter_payloads_map(&self) -> impl '_ + Iterator<Item = (String, PipePayload<Payload>)>
    where
        Payload: Clone,
    {
        self.payloads
            .iter()
            .map(|payload| (payload.key.clone(), payload.clone()))
    }

    pub(crate) fn drop_payloads<P>(self) -> PipeMessage<Value, P>
    where
        P: JsonSchema,
    {
        PipeMessage {
            payloads: self
                .payloads
                .into_iter()
                .map(|payload| payload.drop())
                .collect(),
            reply: self.reply,
            timestamp: self.timestamp,
            value: self.value,
        }
    }

    pub(crate) fn as_dropped_payloads<P>(&self) -> PipeMessage<Value, P>
    where
        P: JsonSchema,
        Value: Clone,
    {
        PipeMessage {
            payloads: self
                .payloads
                .iter()
                .map(|payload| payload.as_dropped())
                .collect(),
            reply: self.reply.clone(),
            timestamp: self.timestamp,
            value: self.value.clone(),
        }
    }

    pub const fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    pub fn to_bytes(&self, encoder: Codec) -> Result<Bytes>
    where
        Payload: Serialize,
        Value: Serialize,
    {
        match encoder {
            Codec::Json => ::serde_json::to_vec(self)
                .map(Into::into)
                .map_err(Into::into),
            Codec::MessagePack => {
                // opcode
                let mut buf = vec![OpCode::MessagePack as u8];

                ::rmp_serde::encode::write(&mut buf, self)
                    .map(|()| buf.into())
                    .map_err(Into::into)
            }
            Codec::Cbor => {
                // opcode
                let mut buf = vec![OpCode::Cbor as u8];

                ::serde_cbor::to_writer(&mut buf, self)
                    .map(|()| buf.into())
                    .map_err(Into::into)
            }
        }
    }
}

impl<Value> PipeMessage<Value> {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub(crate) async fn load_payloads(self, storage: &StorageSet) -> Result<Self> {
        Ok(Self {
            payloads: self
                .payloads
                .into_iter()
                .map(|payload| payload.load(storage))
                .collect::<FuturesOrdered<_>>()
                .try_collect()
                .await?,
            reply: self.reply,
            timestamp: self.timestamp,
            value: self.value,
        })
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub(crate) async fn dump_payloads(
        self,
        storage: &StorageSet,
        model: Option<&Name>,
        input_payloads: Option<&HashMap<String, PipePayload>>,
    ) -> Result<Self> {
        Ok(Self {
            payloads: self
                .payloads
                .into_iter()
                .map(|payload| payload.dump(storage, model, input_payloads))
                .collect::<FuturesOrdered<_>>()
                .filter_map(|payload| async { payload.transpose() })
                .try_collect::<Vec<_>>()
                .await?,
            reply: self.reply,
            timestamp: self.timestamp,
            value: self.value,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PipePayload<Value = Bytes>
where
    Value: JsonSchema,
{
    key: String,
    #[serde(default)]
    model: Option<Name>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    storage: Option<StorageType>,
    value: Option<Value>,
}

impl<Value> PipePayload<Value>
where
    Value: JsonSchema,
{
    pub fn new(key: String, value: Option<Value>) -> Self {
        Self {
            key,
            model: None,
            path: None,
            storage: None,
            value,
        }
    }

    fn drop<T>(self) -> PipePayload<T>
    where
        T: JsonSchema,
    {
        let Self {
            key,
            model,
            path,
            storage,
            value: _,
        } = self;

        PipePayload {
            key,
            model,
            path,
            storage,
            value: None,
        }
    }

    fn as_dropped<T>(&self) -> PipePayload<T>
    where
        T: JsonSchema,
    {
        let Self {
            key,
            model,
            path,
            storage,
            value: _,
        } = self;

        PipePayload {
            key: key.clone(),
            model: model.clone(),
            path: path.clone(),
            storage: *storage,
            value: None,
        }
    }

    pub const fn value(&self) -> Option<&Value> {
        self.value.as_ref()
    }
}

impl PipePayload {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn load(self, storage: &StorageSet) -> Result<Self> {
        let Self {
            key,
            model,
            path,
            storage: storage_type,
            value,
        } = self;

        Ok(Self {
            key,
            value: match storage_type {
                Some(StorageType::Passthrough) => value,
                #[cfg(feature = "s3")]
                Some(StorageType::S3) => match model.as_ref().zip(path.as_ref()) {
                    Some((model, path)) => storage
                        .get(StorageType::S3)
                        .get(model, path)
                        .await
                        .map(Some)?,
                    None => None,
                },
                None => bail!("storage type not defined"),
            },
            path,
            model,
            storage: storage_type,
        })
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn dump(
        self,
        storage: &StorageSet,
        model: Option<&Name>,
        input_payloads: Option<&HashMap<String, PipePayload>>,
    ) -> Result<Option<Self>> {
        let Self {
            key,
            model: last_model,
            path: last_path,
            storage: last_storage_type,
            value,
        } = self;

        let last = input_payloads.and_then(|map| map.get(&key));
        let last_model = last
            .and_then(|payload| payload.model.clone())
            .or(last_model);
        let last_storage_type = last
            .and_then(|payload| payload.storage)
            .or(last_storage_type);

        let next_storage = storage.get_default();
        let next_storage_type = next_storage.storage_type();

        let is_storage_same = last_storage_type
            .map(|last_storage_type| last_storage_type == next_storage_type)
            .unwrap_or_default();

        if last_model.is_some() && is_storage_same {
            // do not restore the payloads to the same storage
            Ok(Some(Self {
                storage: last_storage_type,
                path: last_path,
                model: last_model,
                key,
                value: None,
            }))
        } else {
            match next_storage_type {
                StorageType::Passthrough => Ok(Some(Self {
                    storage: Some(next_storage_type),
                    path: None,
                    model: None,
                    key,
                    value,
                })),
                #[cfg(feature = "s3")]
                StorageType::S3 => match model.or_else(|| next_storage.model()).cloned().zip(value)
                {
                    Some((next_model, value)) => Ok(Some(Self {
                        storage: Some(next_storage_type),
                        path: Some(next_storage.put(Some(&next_model), &key, value).await?),
                        model: Some(next_model),
                        key,
                        value: None,
                    })),
                    None => Ok(None),
                },
            }
        }
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct PipeReply {
    #[serde(default, rename = "__reply_inbox")]
    pub inbox: String,
    #[serde(default, rename = "__reply_target")]
    pub target: Option<Name>,
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
pub enum Codec {
    #[default]
    Json,
    MessagePack,
    Cbor,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum OpCode {
    // Special opcodes
    Unsupported = 0x00,

    // NOTE: The opcode for text serde should be in ASCII
    AsciiEnd = 0x7F,

    // NOTE: The opcodes for binary serde should be in extended ASCII
    MessagePack = 0x80,
    Cbor = 0x81,
}

impl From<u8> for OpCode {
    fn from(value: u8) -> Self {
        match value {
            value if value <= Self::AsciiEnd as u8 => Self::AsciiEnd,
            value if value == Self::MessagePack as u8 => Self::MessagePack,
            value if value == Self::Cbor as u8 => Self::Cbor,
            _ => Self::Unsupported,
        }
    }
}
