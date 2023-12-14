use std::{borrow::Cow, time::Duration};

use dash_pipe_provider::{
    storage::{MetadataStorageType, StorageType},
    MessengerType,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use tracing::{error, instrument, Level};

use crate::ctx::OptimizerContext;

/// Task
impl OptimizerContext {
    #[instrument(level = Level::INFO, skip_all)]
    pub(super) async fn loop_forever_sync_metrics(self) {
        const DURATION_WRITE_METADATA: Duration = Duration::from_secs(10);
        const DURATION_SYNC: Duration = DURATION_WRITE_METADATA;

        const INTERVAL: Duration = Duration::from_secs(5 * 60);

        loop {
            match self
                .get_metric_with_last(
                    MetricSpanKind::MetadataStorage {
                        type_: dash_pipe_provider::storage::MetadataStorageType::LakeHouse,
                    },
                    INTERVAL,
                )
                .await
            {
                Ok(metric) => {
                    dbg!(metric);
                }
                Err(error) => {
                    error!("{error}")
                }
            }
            sleep(DURATION_SYNC).await;
        }
    }
}

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
