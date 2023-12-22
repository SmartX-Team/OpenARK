pub mod edge;
pub mod node;

use std::borrow::Cow;

use dash_pipe_provider::{
    storage::{MetadataStorageType, StorageType},
    MessengerType,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use crate::metadata::ObjectMetadata;

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct MetricSpan<'a> {
    #[serde(flatten)]
    pub duration: MetricDuration,
    #[serde(flatten)]
    pub kind: MetricSpanKind<'a>,
    pub len: usize,
    #[serde(flatten)]
    pub metadata: ObjectMetadata<'a>,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct MetricRow<'a> {
    #[serde(flatten)]
    pub kind: MetricSpanKind<'a>,
    #[serde(flatten)]
    pub metadata: ObjectMetadata<'a>,
    #[serde(flatten)]
    pub value: self::node::NodeMetric,
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct MetricDuration {
    pub begin_ns: u64,
    pub end_ns: u64,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(tag = "kind")]
pub enum MetricSpanKind<'a> {
    Function {
        op: FunctionOperation,
        #[serde(rename = "type")]
        type_: FunctionType,
    },
    Messenger {
        model: Cow<'a, str>,
        op: MessengerOperation,
        #[serde(rename = "type")]
        type_: MessengerType,
    },
    MetadataStorage {
        model: Cow<'a, str>,
        op: MetadataStorageOperation,
        #[serde(rename = "type")]
        type_: MetadataStorageType,
    },
    Storage {
        model: Cow<'a, str>,
        op: StorageOperation,
        #[serde(rename = "type")]
        type_: StorageType,
    },
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
pub enum FunctionOperation {
    Call,
    Tick,
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
pub enum FunctionType {
    Dash,
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
pub enum MessengerOperation {
    Read,
    Reply,
    Request,
    Send,
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
pub enum MetadataStorageOperation {
    List,
    Put,
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
pub enum StorageOperation {
    Get,
    Put,
    Delete,
}
