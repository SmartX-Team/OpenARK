use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct NetworkQuery {
    #[serde(default, rename = "le")]
    pub interval_ms: NetworkQueryNodeValue,
    pub link: NetworkQueryNodeType,
    pub query: String,
    pub sink: NetworkQueryNodeType,
    pub src: NetworkQueryNodeType,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct NetworkQueryNodeType {
    #[serde(default)]
    pub kind: NetworkQueryNodeValue,
    #[serde(default)]
    pub name: NetworkQueryNodeValue,
    #[serde(default)]
    pub namespace: NetworkQueryNodeValue,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type", content = "value")]
pub enum NetworkQueryNodeValue {
    Key(String),
    Static(#[serde(default, skip_serializing_if = "Option::is_none")] Option<String>),
}

impl Default for NetworkQueryNodeValue {
    #[inline]
    fn default() -> Self {
        Self::Static(None)
    }
}

mod impl_json_schema_for_network_query_node_value {
    use std::borrow::Cow;

    use schemars::{gen::SchemaGenerator, schema::Schema, JsonSchema};

    #[allow(dead_code)]
    #[derive(JsonSchema)]
    #[serde(rename_all = "camelCase")]
    struct NetworkQueryNodeValue {
        r#type: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<String>,
    }

    impl JsonSchema for super::NetworkQueryNodeValue {
        #[inline]
        fn is_referenceable() -> bool {
            <NetworkQueryNodeValue as JsonSchema>::is_referenceable()
        }

        #[inline]
        fn schema_name() -> String {
            <NetworkQueryNodeValue as JsonSchema>::schema_name()
        }

        #[inline]
        fn json_schema(gen: &mut SchemaGenerator) -> Schema {
            <NetworkQueryNodeValue as JsonSchema>::json_schema(gen)
        }

        #[inline]
        fn schema_id() -> Cow<'static, str> {
            <NetworkQueryNodeValue as JsonSchema>::schema_id()
        }
    }
}
