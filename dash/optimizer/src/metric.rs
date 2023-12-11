use std::borrow::Cow;

use dash_pipe_provider::{
    storage::{MetadataStorageType, StorageType},
    MessengerType,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct MetricSpan<'a> {
    #[serde(flatten)]
    pub duration: MetricDuration,
    #[serde(flatten)]
    pub kind: MetricSpanKind<'a>,
    pub namespace: Cow<'a, str>,
    pub len: usize,
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
    Messenger {
        topic: Cow<'a, str>,
        #[serde(rename = "type")]
        type_: MessengerType,
    },
    MetadataStorage {
        #[serde(rename = "type")]
        type_: MetadataStorageType,
    },
    Storage {
        #[serde(rename = "type")]
        type_: StorageType,
    },
}
