pub mod edge;
pub mod node;

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
    pub kind: MetricSpanKind,
    pub len: usize,
    #[serde(flatten)]
    pub metadata: ObjectMetadata<'a>,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct MetricRow<'a> {
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
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(tag = "kind")]
pub enum MetricSpanKind {
    Function {
        op: FunctionOperation,
        #[serde(rename = "type")]
        type_: FunctionType,
    },
    Messenger {
        op: MessengerOperation,
        #[serde(rename = "type")]
        type_: MessengerType,
    },
    MetadataStorage {
        op: MetadataStorageOperation,
        #[serde(rename = "type")]
        type_: MetadataStorageType,
    },
    Storage {
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
