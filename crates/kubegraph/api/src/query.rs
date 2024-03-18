use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct NetworkQuery {
    #[serde(default, rename = "le")]
    pub interval_ms: NetworkQueryNodeValue,
    pub query: String,
    #[serde(flatten)]
    pub r#type: NetworkQueryType,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum NetworkQueryType {
    Edge {
        #[serde(default)]
        link: NetworkQueryNodeType,
        #[serde(default)]
        sink: NetworkQueryNodeType,
        #[serde(default)]
        src: NetworkQueryNodeType,
    },
    Node {
        #[serde(default)]
        node: NetworkQueryNodeType,
    },
}

mod impl_json_schema_for_network_query_type {
    use std::borrow::Cow;

    use schemars::{gen::SchemaGenerator, schema::Schema, JsonSchema};

    #[allow(dead_code)]
    #[derive(JsonSchema)]
    #[serde(rename_all = "camelCase")]
    enum NetworkQueryTypeRef {
        Edge,
        Node,
    }

    #[allow(dead_code)]
    #[derive(JsonSchema)]
    #[serde(rename_all = "camelCase")]
    struct NetworkQueryType {
        #[serde(default)]
        link: super::NetworkQueryNodeType,
        #[serde(default)]
        node: super::NetworkQueryNodeType,
        #[serde(default)]
        sink: super::NetworkQueryNodeType,
        #[serde(default)]
        src: super::NetworkQueryNodeType,
        r#type: NetworkQueryTypeRef,
    }

    impl JsonSchema for super::NetworkQueryType {
        #[inline]
        fn is_referenceable() -> bool {
            <NetworkQueryType as JsonSchema>::is_referenceable()
        }

        #[inline]
        fn schema_name() -> String {
            <NetworkQueryType as JsonSchema>::schema_name()
        }

        #[inline]
        fn json_schema(gen: &mut SchemaGenerator) -> Schema {
            <NetworkQueryType as JsonSchema>::json_schema(gen)
        }

        #[inline]
        fn schema_id() -> Cow<'static, str> {
            <NetworkQueryType as JsonSchema>::schema_id()
        }
    }
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
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
    #[derive(Default, JsonSchema)]
    #[serde(rename_all = "camelCase")]
    enum NetworkQueryNodeValueRef {
        Key,
        #[default]
        Static,
    }

    #[allow(dead_code)]
    #[derive(JsonSchema)]
    #[serde(rename_all = "camelCase")]
    struct NetworkQueryNodeValue {
        #[serde(default)]
        r#type: NetworkQueryNodeValueRef,
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
