use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionChannelKindJob {
    pub templates: Vec<TemplateRef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TemplateRef {
    pub name: String,
}
