use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkFunction<Filter = String, Provide = String, Script = String>
where
    Script: Default,
{
    #[serde(default)]
    pub handlers: NetworkHandlers<Filter, Provide, Script>,
}

impl<Filter, Provide, Script> Default for NetworkFunction<Filter, Provide, Script>
where
    Script: Default,
{
    fn default() -> Self {
        Self {
            handlers: Default::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkHandlers<Filter = String, Provide = String, Script = String>
where
    Script: Default,
{
    #[serde(default)]
    pub fake: NetworkFakeHandler<Filter, Provide, Script>,
}

impl<Filter, Provide, Script> Default for NetworkHandlers<Filter, Provide, Script>
where
    Script: Default,
{
    fn default() -> Self {
        Self {
            fake: Default::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkFakeHandler<Filter = String, Provide = String, Script = String>
where
    Script: Default,
{
    #[serde(default)]
    pub data: BTreeMap<String, NetworkFakeHandlerData<Filter, Provide>>,

    #[serde(default)]
    pub script: Script,
}

impl<Filter, Provide, Script> Default for NetworkFakeHandler<Filter, Provide, Script>
where
    Script: Default,
{
    fn default() -> Self {
        Self {
            data: Default::default(),
            script: Default::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkFakeHandlerData<Filter = String, Provide = String> {
    #[serde(default)]
    pub filters: Vec<Filter>,

    #[serde(default)]
    pub provides: Vec<Provide>,
}
