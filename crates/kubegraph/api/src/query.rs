use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct NetworkQuery<M = NetworkQueryMetadata> {
    #[serde(flatten)]
    pub metadata: M,
    pub query: String,
}

impl NetworkQuery {
    pub const fn name(&self) -> &'static str {
        self.metadata.name()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NetworkQueryMetadata {
    Edge(#[serde(default)] NetworkQueryEdgeMetadata),
    Node(#[serde(default)] NetworkQueryNodeMetadata),
}

impl NetworkQueryMetadata {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Edge(_) => "edge",
            Self::Node(_) => "node",
        }
    }
}

mod impl_json_schema_for_network_query_type {
    use std::borrow::Cow;

    use schemars::{gen::SchemaGenerator, schema::Schema, JsonSchema};

    #[allow(dead_code)]
    #[derive(JsonSchema)]
    enum NetworkQueryMetadataRef {
        Edge,
        Node,
    }

    #[allow(dead_code)]
    #[derive(JsonSchema)]
    #[serde(rename_all = "camelCase")]
    struct NetworkQueryMetadata {
        #[serde(default)]
        capacity: super::NetworkQueryValue,
        #[serde(default, rename = "le")]
        interval_ms: super::NetworkQueryValue,
        #[serde(default)]
        name: super::NetworkQueryValue,
        #[serde(default)]
        sink: super::NetworkQueryValue,
        #[serde(default)]
        src: super::NetworkQueryValue,
        #[serde(default)]
        supply: super::NetworkQueryValue,
        r#type: NetworkQueryMetadataRef,
        #[serde(default)]
        unit_cost: super::NetworkQueryValue,
    }

    impl JsonSchema for super::NetworkQueryMetadata {
        #[inline]
        fn is_referenceable() -> bool {
            <NetworkQueryMetadata as JsonSchema>::is_referenceable()
        }

        #[inline]
        fn schema_name() -> String {
            <NetworkQueryMetadata as JsonSchema>::schema_name()
        }

        #[inline]
        fn json_schema(gen: &mut SchemaGenerator) -> Schema {
            <NetworkQueryMetadata as JsonSchema>::json_schema(gen)
        }

        #[inline]
        fn schema_id() -> Cow<'static, str> {
            <NetworkQueryMetadata as JsonSchema>::schema_id()
        }
    }
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct NetworkQueryEdgeMetadata {
    #[serde(default, flatten)]
    pub extras: BTreeMap<String, NetworkQueryValue>,
    #[serde(default, rename = "le")]
    pub interval_ms: NetworkQueryValue,
    #[serde(default)]
    pub sink: NetworkQueryValue,
    #[serde(default)]
    pub src: NetworkQueryValue,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct NetworkQueryNodeMetadata {
    #[serde(default, flatten)]
    pub extras: BTreeMap<String, NetworkQueryValue>,
    #[serde(default, rename = "le")]
    pub interval_ms: NetworkQueryValue,
    #[serde(default)]
    pub name: NetworkQueryValue,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum NetworkQueryValue {
    Key(String),
    Static(#[serde(default, skip_serializing_if = "Option::is_none")] Option<String>),
}

impl Default for NetworkQueryValue {
    #[inline]
    fn default() -> Self {
        Self::Static(None)
    }
}

mod impl_json_schema_for_network_query_value {
    use std::borrow::Cow;

    use schemars::{gen::SchemaGenerator, schema::Schema, JsonSchema};

    #[allow(dead_code)]
    #[derive(Default, JsonSchema)]
    enum NetworkQueryValueRef {
        Key,
        #[default]
        Static,
    }

    #[allow(dead_code)]
    #[derive(JsonSchema)]
    #[serde(rename_all = "camelCase")]
    struct NetworkQueryValue {
        #[serde(default)]
        r#type: NetworkQueryValueRef,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<String>,
    }

    impl JsonSchema for super::NetworkQueryValue {
        #[inline]
        fn is_referenceable() -> bool {
            <NetworkQueryValue as JsonSchema>::is_referenceable()
        }

        #[inline]
        fn schema_name() -> String {
            <NetworkQueryValue as JsonSchema>::schema_name()
        }

        #[inline]
        fn json_schema(gen: &mut SchemaGenerator) -> Schema {
            <NetworkQueryValue as JsonSchema>::json_schema(gen)
        }

        #[inline]
        fn schema_id() -> Cow<'static, str> {
            <NetworkQueryValue as JsonSchema>::schema_id()
        }
    }
}
