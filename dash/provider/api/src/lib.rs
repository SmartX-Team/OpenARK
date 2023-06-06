pub mod job;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FunctionChannel {
    pub metadata: SessionContextMetadata,
    pub actor: FunctionChannelKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "spec")]
pub enum FunctionChannelKind {
    Job(self::job::FunctionChannelKindJob),
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
