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
pub struct NodeMetric {
    pub elapsed_ns: i64,
    pub len: i64,
    pub total_bytes: i64,
}
