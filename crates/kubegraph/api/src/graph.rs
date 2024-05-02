use std::fmt;

use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Graph<T> {
    pub edges: T,
    pub nodes: T,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkEntryMap {
    #[serde(default)]
    pub edges: Vec<NetworkEdge>,
    #[serde(default)]
    pub nodes: Vec<NetworkNode>,
}

impl NetworkEntryMap {
    pub fn push(&mut self, entry: NetworkEntry) {
        let NetworkEntry { key, value } = entry;
        match key {
            NetworkEntryKey::Edge(key) => {
                self.edges.push(NetworkEdge { key, value });
            }
            NetworkEntryKey::Node(key) => {
                self.nodes.push(NetworkNode { key, value });
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkEntry {
    #[serde(flatten)]
    pub key: NetworkEntryKey,
    pub value: NetworkValue,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct NetworkEntryKeyFilter {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub namespace: Option<String>,
}

impl NetworkEntryKeyFilter {
    pub fn contains(&self, key: &NetworkEntryKey) -> bool {
        match key {
            NetworkEntryKey::Edge(key) => {
                self.contains_node_key(&key.link)
                    && self.contains_node_key(&key.sink)
                    && self.contains_node_key(&key.src)
            }
            NetworkEntryKey::Node(key) => self.contains_node_key(key),
        }
    }

    fn contains_node_key(&self, key: &NetworkNodeKey) -> bool {
        let Self { kind, namespace } = self;

        fn test(a: &Option<String>, b: &String) -> bool {
            match a.as_ref() {
                Some(a) => a == b,
                None => true,
            }
        }

        test(kind, &key.kind) && test(namespace, &key.namespace)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum NetworkEntryKey {
    Edge(NetworkEdgeKey),
    Node(NetworkNodeKey),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkEdge {
    #[serde(flatten)]
    pub key: NetworkEdgeKey,
    pub value: NetworkValue,
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(
    rename_all = "camelCase",
    bound = "
    NodeKey: Ord + Serialize + DeserializeOwned,
"
)]
pub struct NetworkEdgeKey<NodeKey = NetworkNodeKey>
where
    NodeKey: Ord,
{
    #[serde(default, rename = "le", skip_serializing_if = "Option::is_none")]
    pub interval_ms: Option<u64>,
    #[serde(
        flatten,
        deserialize_with = "self::prefix::link::deserialize",
        serialize_with = "self::prefix::link::serialize"
    )]
    pub link: NodeKey,
    #[serde(
        flatten,
        deserialize_with = "self::prefix::sink::deserialize",
        serialize_with = "self::prefix::sink::serialize"
    )]
    pub sink: NodeKey,
    #[serde(
        flatten,
        deserialize_with = "self::prefix::src::deserialize",
        serialize_with = "self::prefix::src::serialize"
    )]
    pub src: NodeKey,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkNode {
    #[serde(flatten)]
    pub key: NetworkNodeKey,
    pub value: NetworkValue,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct NetworkNodeKey {
    pub kind: String,
    pub name: String,
    pub namespace: String,
}

impl fmt::Display for NetworkNodeKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            kind,
            name,
            namespace,
        } = self;

        write!(f, "{kind}/{namespace}/{name}")
    }
}

#[derive(Clone, Debug, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum NetworkValue {
    Boolean(bool),
    Number(f64),
    String(String),
}

mod prefix {
    ::serde_with::with_prefix!(pub(super) link "link_");
    ::serde_with::with_prefix!(pub(super) sink "sink_");
    ::serde_with::with_prefix!(pub(super) src "src_");
}
