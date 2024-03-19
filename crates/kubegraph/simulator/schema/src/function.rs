use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{derive::NetworkDerive, value::NetworkValue};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkFunction {
    #[serde(default)]
    pub handlers: BTreeMap<String, NetworkHandler>,

    #[serde(default)]
    pub recursive: bool,

    #[serde(default)]
    pub values: BTreeMap<String, NetworkFunctionValue>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkHandler {
    #[serde(default)]
    pub values: BTreeMap<String, NetworkHandlerValue>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkHandlerValue {
    #[serde(default)]
    pub input: NetworkHandlerDelta,
    #[serde(default)]
    pub output: NetworkHandlerDelta,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum NetworkHandlerDelta {
    Constant(NetworkValue),
}

impl Default for NetworkHandlerDelta {
    #[inline]
    fn default() -> Self {
        Self::Constant(NetworkValue::default())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkFunctionValue {
    pub input: String,
    pub output: String,
    pub derive: NetworkDerive,
}
