pub mod job;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskChannel {
    pub metadata: SessionContextMetadata,
    pub actor: TaskChannelKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "spec")]
pub enum TaskChannelKind {
    Job(self::job::TaskChannelKindJob),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionContext<Spec> {
    pub metadata: SessionContextMetadata,
    pub spec: Spec,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionContextMetadata {
    pub name: String,
    pub namespace: String,
}

pub mod name {
    pub const RE: &str = r"^/([a-z_-][a-z0-9_-]*[a-z0-9]?/)*$";
    pub const RE_CHILD: &str = r"^[a-z_-][a-z0-9_-]*[a-z0-9]?$";
    pub const RE_SET: &str = r"^(/[1-9]?[0-9]+|/[a-z_-][a-z0-9_-]*[a-z0-9]?)*/([A-Za-z0-9._-]*)$";
}
