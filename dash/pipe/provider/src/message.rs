use std::{collections::HashMap, fmt, ops, str::FromStr};

use anyhow::{bail, Error, Result};
use bytes::Bytes;
use futures::future::try_join_all;
use regex::Regex;
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};

use crate::storage::{StorageSet, StorageType};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PipeMessages<Value = ::serde_json::Value, Payload = Bytes>
where
    Payload: Default + JsonSchema,
    Value: Default,
{
    None,
    Single(PipeMessage<Value, Payload>),
    Batch(Vec<PipeMessage<Value, Payload>>),
}

#[cfg(feature = "pyo3")]
impl From<PipeMessages> for Vec<PyPipeMessage> {
    fn from(value: PipeMessages) -> Self {
        match value {
            PipeMessages::None => Default::default(),
            PipeMessages::Single(value) => {
                vec![value.into()]
            }
            PipeMessages::Batch(values) => values.into_iter().map(Into::into).collect(),
        }
    }
}

impl<Value> PipeMessages<Value>
where
    Value: Default,
{
    pub(crate) async fn dump_payloads(
        self,
        storage: &StorageSet,
        input_payloads: &HashMap<String, PipePayload<()>>,
    ) -> Result<PipeMessages<Value, ()>> {
        match self {
            Self::None => Ok(PipeMessages::None),
            Self::Single(value) => value
                .dump_payloads(storage, input_payloads)
                .await
                .map(PipeMessages::Single),
            Self::Batch(values) => try_join_all(
                values
                    .into_iter()
                    .map(|value| value.dump_payloads(storage, input_payloads)),
            )
            .await
            .map(PipeMessages::Batch),
        }
    }
}

impl<Value, Payload> PipeMessages<Value, Payload>
where
    Payload: Default + JsonSchema,
    Value: Default,
{
    pub(crate) fn get_payloads_ref(&self) -> HashMap<String, PipePayload<()>> {
        match self {
            PipeMessages::None => Default::default(),
            PipeMessages::Single(value) => value.get_payloads_ref().collect(),
            PipeMessages::Batch(values) => values
                .iter()
                .flat_map(|value| value.get_payloads_ref())
                .collect(),
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

#[cfg(feature = "pyo3")]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[::pyo3::pyclass]
pub struct PyPipeMessage {
    payloads: Vec<PipePayload>,
    value: ::serde_json::Value,
}

#[cfg(feature = "pyo3")]
impl From<PipeMessage> for PyPipeMessage {
    fn from(PipeMessage { payloads, value }: PipeMessage) -> Self {
        Self { payloads, value }
    }
}

#[cfg(feature = "pyo3")]
impl From<PyPipeMessage> for PipeMessage {
    fn from(PyPipeMessage { payloads, value }: PyPipeMessage) -> Self {
        Self { payloads, value }
    }
}

#[cfg(feature = "pyo3")]
#[::pyo3::pymethods]
impl PyPipeMessage {
    #[new]
    fn new(payloads: Vec<(String, Option<Vec<u8>>)>, value: &::pyo3::PyAny) -> Self {
        fn value_to_native(value: &::pyo3::PyAny) -> ::serde_json::Value {
            if value.is_none() {
                ::serde_json::Value::Null
            } else if let Ok(value) = value.extract::<bool>() {
                ::serde_json::Value::Bool(value)
            } else if let Ok(value) = value.extract::<u64>() {
                ::serde_json::Value::Number(value.into())
            } else if let Ok(value) = value.extract::<i64>() {
                ::serde_json::Value::Number(value.into())
            } else if let Some(value) = value
                .extract::<f64>()
                .ok()
                .and_then(::serde_json::Number::from_f64)
            {
                ::serde_json::Value::Number(value)
            } else if let Ok(value) = value.extract::<String>() {
                ::serde_json::Value::String(value)
            } else if let Ok(values) = value.downcast::<::pyo3::types::PyList>() {
                ::serde_json::Value::Array(values.iter().map(value_to_native).collect())
            } else if let Ok(values) = value.downcast::<::pyo3::types::PyDict>() {
                ::serde_json::Value::Object(
                    values
                        .iter()
                        .filter_map(|(key, value)| {
                            key.extract().ok().map(|key| (key, value_to_native(value)))
                        })
                        .collect(),
                )
            } else {
                // do not save the value
                ::serde_json::Value::Null
            }
        }

        Self {
            payloads: payloads
                .into_iter()
                .map(|(key, value)| {
                    PipePayload::new(key, value.map(Into::into).unwrap_or_default())
                })
                .collect(),
            value: value_to_native(value),
        }
    }

    #[getter]
    fn get_payloads(&self) -> Vec<(&str, &[u8])> {
        self.payloads
            .iter()
            .map(
                |PipePayload {
                     key,
                     model: _,
                     storage: _,
                     value,
                 }| { (key.as_str(), value as &[u8]) },
            )
            .collect()
    }

    #[getter]
    fn get_value(&self, py: ::pyo3::Python) -> ::pyo3::PyObject {
        use pyo3::{types::IntoPyDict, IntoPy};

        fn value_to_py(py: ::pyo3::Python, value: &::serde_json::Value) -> ::pyo3::PyObject {
            match value {
                ::serde_json::Value::Null => ().into_py(py),
                ::serde_json::Value::Bool(value) => value.into_py(py),
                ::serde_json::Value::Number(value) => match value.as_u64() {
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
                ::serde_json::Value::String(value) => value.into_py(py),
                ::serde_json::Value::Array(values) => values
                    .iter()
                    .map(|value| value_to_py(py, value))
                    .collect::<Vec<_>>()
                    .into_py(py),
                ::serde_json::Value::Object(values) => values
                    .iter()
                    .map(|(key, value)| (key, value_to_py(py, value)))
                    .into_py_dict(py)
                    .into(),
            }
        }

        value_to_py(py, &self.value)
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
pub struct PipeMessage<Value = ::serde_json::Value, Payload = Bytes>
where
    Payload: Default + JsonSchema,
    Value: Default,
{
    #[serde(default)]
    pub payloads: Vec<PipePayload<Payload>>,
    #[serde(default)]
    pub value: Value,
}

impl<Value> TryFrom<Bytes> for PipeMessage<Value, ()>
where
    Value: Default + DeserializeOwned,
{
    type Error = Error;

    fn try_from(value: Bytes) -> Result<Self> {
        ::serde_json::from_reader(&*value).map_err(Into::into)
    }
}

impl<Value> TryFrom<::nats::Message> for PipeMessage<Value, ()>
where
    Value: Default + DeserializeOwned,
{
    type Error = Error;

    fn try_from(message: ::nats::Message) -> Result<Self> {
        message.payload.try_into()
    }
}

impl<Value, Payload> TryFrom<&PipeMessage<Value, Payload>> for Bytes
where
    Payload: Default + Serialize + JsonSchema,
    Value: Default + Serialize,
{
    type Error = Error;

    fn try_from(value: &PipeMessage<Value, Payload>) -> Result<Self> {
        ::serde_json::to_vec(value)
            .map(Into::into)
            .map_err(Into::into)
    }
}

impl<Value, Payload> TryFrom<&PipeMessage<Value, Payload>> for ::serde_json::Value
where
    Payload: Default + Serialize + JsonSchema,
    Value: Default + Serialize,
{
    type Error = Error;

    fn try_from(value: &PipeMessage<Value, Payload>) -> Result<Self> {
        ::serde_json::to_value(value).map_err(Into::into)
    }
}

impl<Value> PipeMessage<Value>
where
    Value: Default,
{
    async fn dump_payloads(
        self,
        storage: &StorageSet,
        input_payloads: &HashMap<String, PipePayload<()>>,
    ) -> Result<PipeMessage<Value, ()>> {
        Ok(PipeMessage {
            payloads: try_join_all(
                self.payloads
                    .into_iter()
                    .map(|payload| payload.dump(storage, input_payloads)),
            )
            .await?,
            value: self.value,
        })
    }
}

impl<Value, Payload> PipeMessage<Value, Payload>
where
    Payload: Default + JsonSchema,
    Value: Default,
{
    fn get_payloads_ref(&self) -> impl '_ + Iterator<Item = (String, PipePayload<()>)> {
        self.payloads
            .iter()
            .map(|payload| (payload.key.clone(), payload.get_ref()))
    }

    pub(crate) async fn load_payloads(self, storage: &StorageSet) -> Result<PipeMessage<Value>> {
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

    pub(crate) fn load_payloads_as_empty(self) -> PipeMessage<Value> {
        PipeMessage {
            payloads: self
                .payloads
                .into_iter()
                .map(|payload| payload.load_as_empty())
                .collect(),
            value: self.value,
        }
    }

    pub fn to_json(&self) -> Result<::serde_json::Value>
    where
        Payload: Serialize,
        Value: Serialize,
    {
        self.try_into()
    }

    pub fn to_json_bytes(&self) -> Result<Bytes>
    where
        Payload: Serialize,
        Value: Serialize,
    {
        self.try_into()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PipePayload<Value = Bytes>
where
    Value: Default + JsonSchema,
{
    key: String,
    #[serde(default)]
    model: Option<ModelRef>,
    #[serde(default)]
    storage: Option<StorageType>,
    #[serde(default)]
    value: Value,
}

impl<Value> PipePayload<Value>
where
    Value: Default + JsonSchema,
{
    pub fn new(key: String, value: Value) -> Self {
        Self {
            key,
            model: None,
            storage: None,
            value,
        }
    }

    fn get_ref<T>(&self) -> PipePayload<T>
    where
        T: Default + JsonSchema,
    {
        let Self {
            key,
            model,
            storage,
            value: _,
        } = self;

        PipePayload {
            key: key.clone(),
            model: model.clone(),
            storage: *storage,
            value: Default::default(),
        }
    }

    async fn load(self, storage: &StorageSet) -> Result<PipePayload> {
        let Self {
            key,
            model,
            storage: storage_type,
            value: _,
        } = self;

        Ok(PipePayload {
            value: match model.as_ref().zip(storage_type) {
                Some((model, storage_type)) => storage.get(storage_type).get(model, &key).await?,
                None => bail!("storage type not defined"),
            },
            key,
            model,
            storage: storage_type,
        })
    }

    fn load_as_empty<T>(self) -> PipePayload<T>
    where
        T: Default + JsonSchema,
    {
        let Self {
            key,
            model,
            storage,
            value: _,
        } = self;

        PipePayload {
            key,
            model,
            storage,
            value: Default::default(),
        }
    }

    pub const fn value(&self) -> &Value {
        &self.value
    }
}

impl PipePayload {
    async fn dump(
        self,
        storage: &StorageSet,
        input_payloads: &HashMap<String, PipePayload<()>>,
    ) -> Result<PipePayload<()>> {
        let Self {
            key,
            model,
            storage: last_storage,
            value,
        } = self;

        let last_storage = input_payloads
            .get(&key)
            .and_then(|payload| payload.storage)
            .or(last_storage);
        let next_storage = storage.get_default().storage_type();

        let model = if model.is_some()
            || last_storage
                .map(|last_storage| last_storage == next_storage)
                .unwrap_or_default()
        {
            // do not restore the payloads to the same storage
            model
        } else if let Some(model) = storage.get_default().model().cloned() {
            storage.get_default().put(&key, value).await?;
            Some(model)
        } else {
            None
        };

        Ok(PipePayload {
            key,
            model,
            storage: Some(next_storage),
            value: (),
        })
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct ModelRef(String);

impl FromStr for ModelRef {
    type Err = Error;

    fn from_str(name: &str) -> Result<Self, <Self as FromStr>::Err> {
        let re = Regex::new(r"^[a-z][a-z0-9_-]*[a-z0-9]?$")?;
        if re.is_match(name) {
            Ok(Self(name.into()))
        } else {
            bail!("model name is invalid: {name:?}")
        }
    }
}

impl From<ModelRef> for String {
    fn from(value: ModelRef) -> Self {
        value.0
    }
}

impl ops::Deref for ModelRef {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Debug for ModelRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <String as fmt::Debug>::fmt(&self.0, f)
    }
}

impl fmt::Display for ModelRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <String as fmt::Display>::fmt(&self.0, f)
    }
}
impl<'de> Deserialize<'de> for ModelRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        <String as Deserialize<'de>>::deserialize(deserializer)
            .and_then(|name| Self::from_str(&name).map_err(::serde::de::Error::custom))
    }
}
