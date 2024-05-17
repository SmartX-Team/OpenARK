#[cfg(feature = "df-polars")]
pub mod polars;

use std::fmt;

use anyhow::Result;
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::frame::LazyFrame;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdges<T>(pub(crate) T);

impl<T> GraphEdges<T> {
    pub const fn new(edges: T) -> Self {
        Self(edges)
    }

    pub fn into_inner(self) -> T {
        self.0
    }
}

impl FromIterator<Self> for GraphEdges<LazyFrame> {
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = Self>,
    {
        let mut iter = iter.into_iter().peekable();
        match iter.peek() {
            Some(GraphEdges(LazyFrame::Empty)) | None => Self(LazyFrame::Empty),
            #[cfg(feature = "df-polars")]
            Some(GraphEdges(LazyFrame::Polars(_))) => iter
                .filter_map(|GraphEdges(edges)| edges.try_into_polars().ok().map(GraphEdges))
                .collect::<GraphEdges<_>>(),
        }
    }
}

pub trait IntoGraph<T> {
    /// Disaggregate two dataframes.
    fn try_into_graph(self) -> Result<Graph<T>>
    where
        Self: Sized;
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Graph<T> {
    pub edges: T,
    pub nodes: T,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct NetworkGraphMetadata {
    #[serde(default = "NetworkGraphMetadata::default_capacity")]
    pub capacity: String,
    #[serde(default = "NetworkGraphMetadata::default_flow")]
    pub flow: String,
    #[serde(default = "NetworkGraphMetadata::default_function")]
    pub function: String,
    #[serde(default = "NetworkGraphMetadata::default_name")]
    pub name: String,
    #[serde(default = "NetworkGraphMetadata::default_sink")]
    pub sink: String,
    #[serde(default = "NetworkGraphMetadata::default_src")]
    pub src: String,
    #[serde(default = "NetworkGraphMetadata::default_supply")]
    pub supply: String,
    #[serde(default = "NetworkGraphMetadata::default_unit_cost")]
    pub unit_cost: String,
}

impl Default for NetworkGraphMetadata {
    fn default() -> Self {
        Self {
            capacity: Self::default_capacity(),
            flow: Self::default_flow(),
            function: Self::default_function(),
            name: Self::default_name(),
            sink: Self::default_sink(),
            src: Self::default_src(),
            supply: Self::default_supply(),
            unit_cost: Self::default_unit_cost(),
        }
    }
}

impl NetworkGraphMetadata {
    pub fn default_capacity() -> String {
        "capacity".into()
    }

    pub fn default_flow() -> String {
        "flow".into()
    }

    pub fn default_function() -> String {
        "function".into()
    }

    pub fn default_name() -> String {
        "name".into()
    }

    pub fn default_link() -> String {
        "link".into()
    }

    pub fn default_sink() -> String {
        "sink".into()
    }

    pub fn default_src() -> String {
        "src".into()
    }

    pub fn default_supply() -> String {
        "supply".into()
    }

    pub fn default_unit_cost() -> String {
        "unit_cost".into()
    }
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
        let Self { namespace } = self;

        #[inline]
        fn test(a: &Option<String>, b: &String) -> bool {
            match a.as_ref() {
                Some(a) => a == b,
                None => true,
            }
        }

        test(namespace, &key.namespace)
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum NetworkEntryKey {
    Edge(NetworkEdgeKey),
    Node(NetworkNodeKey),
}

impl NetworkEntryKey {
    pub fn namespace(&self) -> &str {
        match self {
            Self::Edge(key) => key.namespace(),
            Self::Node(key) => &key.namespace,
        }
    }
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

impl NetworkEdgeKey {
    pub fn namespace(&self) -> &str {
        &self.link.namespace
    }
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
    pub name: String,
    pub namespace: String,
}

impl fmt::Display for NetworkNodeKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { name, namespace } = self;

        write!(f, "{namespace}/{name}")
    }
}

#[derive(Clone, Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkValue {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supply: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_cost: Option<i64>,
}

mod prefix {
    ::serde_with::with_prefix!(pub(super) link "link_");
    ::serde_with::with_prefix!(pub(super) sink "sink_");
    ::serde_with::with_prefix!(pub(super) src "src_");
}
