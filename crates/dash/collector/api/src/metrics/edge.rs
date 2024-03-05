use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Copy,
    Clone,
    Debug,
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
pub struct EdgeMetric {
    pub latency_ms: i64,
    pub throughput_per_sec: i64,
}
