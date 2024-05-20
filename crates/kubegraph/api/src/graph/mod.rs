#[cfg(feature = "df-polars")]
pub mod polars;

use std::collections::BTreeMap;

use anyhow::Result;
use async_trait::async_trait;
use futures::try_join;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    frame::{DataFrame, LazyFrame},
    function::FunctionMetadata,
};

pub(crate) struct ScopedNetworkGraphDBContainer<'a, T>
where
    T: NetworkGraphDB,
{
    pub(crate) inner: &'a T,
    pub(crate) metadata: &'a GraphMetadata,
    pub(crate) scope: &'a GraphScope,
    pub(crate) static_edges: Option<GraphEdges<LazyFrame>>,
}

#[async_trait]
impl<'a, T> ScopedNetworkGraphDB for ScopedNetworkGraphDBContainer<'a, T>
where
    T: NetworkGraphDB,
{
    async fn insert(&self, nodes: LazyFrame) -> Result<()> {
        let Self {
            inner,
            metadata,
            scope,
            static_edges,
        } = self;

        let graph = Graph {
            data: GraphData {
                nodes,
                edges: static_edges.clone().unwrap_or_default().into_inner(),
            },
            metadata: (*metadata).clone(),
            scope: (*scope).clone(),
        };
        inner.insert(graph).await
    }
}

#[async_trait]
pub trait ScopedNetworkGraphDB
where
    Self: Sync,
{
    async fn insert(&self, nodes: LazyFrame) -> Result<()>;
}

#[async_trait]
pub trait NetworkGraphDBExt {
    async fn get_global_namespaced(&self, namespace: &str) -> Result<Option<Graph<LazyFrame>>>;
}

#[async_trait]
impl<T> NetworkGraphDBExt for T
where
    T: NetworkGraphDB,
{
    async fn get_global_namespaced(&self, namespace: &str) -> Result<Option<Graph<LazyFrame>>> {
        let scope = GraphScope {
            namespace: namespace.into(),
            name: GraphScope::NAME_GLOBAL.into(),
        };
        self.get(&scope).await
    }
}

#[async_trait]
pub trait NetworkGraphDB
where
    Self: Sync,
{
    async fn get(&self, scope: &GraphScope) -> Result<Option<Graph<LazyFrame>>>;

    async fn insert(&self, graph: Graph<LazyFrame>) -> Result<()>;

    async fn list(&self, filter: Option<&GraphFilter>) -> Result<Vec<Graph<LazyFrame>>>;

    async fn close(&self) -> Result<()>;
}

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

impl GraphEdges<LazyFrame> {
    pub fn from_static(key: &str, edges: LazyFrame) -> Result<Option<Self>> {
        let function = FunctionMetadata {
            name: FunctionMetadata::NAME_STATIC.into(),
        };

        match edges {
            LazyFrame::Empty => Ok(None),
            mut edges => edges
                .alias(key, &function)
                .map(|()| Self::new(edges))
                .map(Some),
        }
    }

    pub fn concat(self, other: Self) -> Result<Self> {
        self.0.concat(other.0).map(Self)
    }
}

impl FromIterator<Self> for GraphEdges<LazyFrame> {
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = Self>,
    {
        let mut iter = iter
            .into_iter()
            .filter(|GraphEdges(edges)| !matches!(edges, LazyFrame::Empty))
            .peekable();

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
    fn try_into_graph(self) -> Result<GraphData<T>>
    where
        Self: Sized;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Graph<T, M = GraphMetadata> {
    pub data: GraphData<T>,
    pub metadata: M,
    pub scope: GraphScope,
}

impl<M> Graph<DataFrame, M> {
    pub fn drop_null_columns(self) -> Self {
        let Self {
            data,
            metadata,
            scope,
        } = self;
        Self {
            data: data.drop_null_columns(),
            metadata,
            scope,
        }
    }

    pub fn lazy(self) -> Graph<LazyFrame, M> {
        let Self {
            data,
            metadata,
            scope,
        } = self;
        Graph {
            data: data.lazy(),
            metadata,
            scope,
        }
    }
}

impl<M> Graph<LazyFrame, M>
where
    M: GraphMetadataPinnedExt,
{
    pub fn cast<MT>(self, to: MT) -> Graph<LazyFrame, MT>
    where
        MT: GraphMetadataPinnedExt,
    {
        let Self {
            data,
            metadata,
            scope,
        } = self;
        Graph {
            data: data.cast(&metadata, &to),
            metadata: to,
            scope,
        }
    }
}

impl Graph<LazyFrame> {
    pub async fn collect(self) -> Result<Graph<DataFrame>> {
        let Self {
            data,
            metadata,
            scope,
        } = self;
        Ok(Graph {
            data: data.collect().await?,
            metadata,
            scope,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GraphData<T> {
    pub edges: T,
    pub nodes: T,
}

impl GraphData<DataFrame> {
    pub fn drop_null_columns(self) -> Self {
        let Self { edges, nodes } = self;
        Self {
            edges: edges.drop_null_columns(),
            nodes: nodes.drop_null_columns(),
        }
    }

    pub fn lazy(self) -> GraphData<LazyFrame> {
        let Self { edges, nodes } = self;
        GraphData {
            edges: edges.lazy(),
            nodes: nodes.lazy(),
        }
    }
}

impl GraphData<LazyFrame> {
    pub fn cast<MF, MT>(self, from: &MF, to: &MT) -> Self
    where
        MF: GraphMetadataPinnedExt,
        MT: GraphMetadataPinnedExt,
    {
        let Self { edges, nodes } = self;
        Self {
            edges: edges.cast(GraphDataType::Edge, from, to),
            nodes: nodes.cast(GraphDataType::Node, from, to),
        }
    }

    pub async fn collect(self) -> Result<GraphData<DataFrame>> {
        let Self { edges, nodes } = self;
        let (edges, nodes) = try_join!(edges.collect(), nodes.collect(),)?;
        Ok(GraphData { edges, nodes })
    }

    pub fn concat(self, other: Self) -> Result<Self> {
        let Self {
            edges: edges_a,
            nodes: nodes_a,
        } = self;
        let Self {
            edges: edges_b,
            nodes: nodes_b,
        } = other;

        Ok(Self {
            edges: edges_a.concat(edges_b)?,
            nodes: nodes_a.concat(nodes_b)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "dataType", rename_all = "camelCase")]
pub enum GraphMetadata {
    Raw(GraphMetadataRaw),
    Pinned(GraphMetadataPinned),
    Standard(GraphMetadataStandard),
}

impl Default for GraphMetadata {
    fn default() -> Self {
        Self::Standard(GraphMetadataStandard::default())
    }
}

pub trait GraphMetadataExt {
    fn extras(&self) -> Option<&BTreeMap<String, String>>;

    fn interval_ms(&self) -> &str;

    fn name(&self) -> &str;

    fn sink(&self) -> &str;

    fn src(&self) -> &str;
}

impl GraphMetadataExt for GraphMetadata {
    fn extras(&self) -> Option<&BTreeMap<String, String>> {
        match self {
            GraphMetadata::Raw(m) => m.extras(),
            GraphMetadata::Pinned(m) => m.extras(),
            GraphMetadata::Standard(m) => m.extras(),
        }
    }

    fn interval_ms(&self) -> &str {
        match self {
            GraphMetadata::Raw(m) => m.interval_ms(),
            GraphMetadata::Pinned(m) => GraphMetadataExt::interval_ms(m),
            GraphMetadata::Standard(m) => GraphMetadataExt::interval_ms(m),
        }
    }

    fn name(&self) -> &str {
        match self {
            GraphMetadata::Raw(m) => m.name(),
            GraphMetadata::Pinned(m) => GraphMetadataExt::name(m),
            GraphMetadata::Standard(m) => GraphMetadataExt::name(m),
        }
    }

    fn sink(&self) -> &str {
        match self {
            GraphMetadata::Raw(m) => m.sink(),
            GraphMetadata::Pinned(m) => GraphMetadataExt::sink(m),
            GraphMetadata::Standard(m) => GraphMetadataExt::sink(m),
        }
    }

    fn src(&self) -> &str {
        match self {
            GraphMetadata::Raw(m) => m.src(),
            GraphMetadata::Pinned(m) => GraphMetadataExt::src(m),
            GraphMetadata::Standard(m) => GraphMetadataExt::src(m),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphMetadataRaw {
    #[serde(default, flatten)]
    pub extras: BTreeMap<String, String>,
    #[serde(default = "GraphMetadataPinned::default_interval_ms")]
    pub interval_ms: String,
    #[serde(default = "GraphMetadataPinned::default_name")]
    pub name: String,
    #[serde(default = "GraphMetadataPinned::default_sink")]
    pub sink: String,
    #[serde(default = "GraphMetadataPinned::default_src")]
    pub src: String,
}

mod impl_json_schema_for_graph_metadata_raw {
    use std::{borrow::Cow, collections::BTreeMap};

    use schemars::{gen::SchemaGenerator, schema::Schema, JsonSchema};

    #[allow(dead_code)]
    #[derive(JsonSchema)]
    #[serde(transparent)]
    struct GraphMetadataRaw(BTreeMap<String, String>);

    impl JsonSchema for super::GraphMetadataRaw {
        #[inline]
        fn is_referenceable() -> bool {
            <GraphMetadataRaw as JsonSchema>::is_referenceable()
        }

        #[inline]
        fn schema_name() -> String {
            <GraphMetadataRaw as JsonSchema>::schema_name()
        }

        #[inline]
        fn json_schema(gen: &mut SchemaGenerator) -> Schema {
            <GraphMetadataRaw as JsonSchema>::json_schema(gen)
        }

        #[inline]
        fn schema_id() -> Cow<'static, str> {
            <GraphMetadataRaw as JsonSchema>::schema_id()
        }
    }
}

impl Default for GraphMetadataRaw {
    fn default() -> Self {
        Self {
            extras: BTreeMap::default(),
            interval_ms: GraphMetadataPinned::default_interval_ms(),
            name: GraphMetadataPinned::default_name(),
            sink: GraphMetadataPinned::default_sink(),
            src: GraphMetadataPinned::default_src(),
        }
    }
}

impl GraphMetadataExt for GraphMetadataRaw {
    fn extras(&self) -> Option<&BTreeMap<String, String>> {
        Some(&self.extras)
    }

    fn interval_ms(&self) -> &str {
        &self.interval_ms
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn sink(&self) -> &str {
        &self.sink
    }

    fn src(&self) -> &str {
        &self.src
    }
}

pub trait GraphMetadataPinnedExt {
    fn capacity(&self) -> &str;

    fn flow(&self) -> &str;

    fn function(&self) -> &str;

    fn interval_ms(&self) -> &str;

    fn name(&self) -> &str;

    fn sink(&self) -> &str;

    fn src(&self) -> &str;

    fn supply(&self) -> &str;

    fn unit_cost(&self) -> &str;
}

impl<T> GraphMetadataExt for T
where
    T: GraphMetadataPinnedExt,
{
    fn extras(&self) -> Option<&BTreeMap<String, String>> {
        None
    }

    fn interval_ms(&self) -> &str {
        GraphMetadataPinnedExt::interval_ms(self)
    }

    fn name(&self) -> &str {
        GraphMetadataPinnedExt::name(self)
    }

    fn sink(&self) -> &str {
        GraphMetadataPinnedExt::sink(self)
    }

    fn src(&self) -> &str {
        GraphMetadataPinnedExt::src(self)
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct GraphMetadataPinned {
    #[serde(default = "GraphMetadataPinned::default_capacity")]
    pub capacity: String,
    #[serde(default = "GraphMetadataPinned::default_flow")]
    pub flow: String,
    #[serde(default = "GraphMetadataPinned::default_function")]
    pub function: String,
    #[serde(default = "GraphMetadataPinned::default_interval_ms")]
    pub interval_ms: String,
    #[serde(default = "GraphMetadataPinned::default_name")]
    pub name: String,
    #[serde(default = "GraphMetadataPinned::default_sink")]
    pub sink: String,
    #[serde(default = "GraphMetadataPinned::default_src")]
    pub src: String,
    #[serde(default = "GraphMetadataPinned::default_supply")]
    pub supply: String,
    #[serde(default = "GraphMetadataPinned::default_unit_cost")]
    pub unit_cost: String,
}

impl Default for GraphMetadataPinned {
    fn default() -> Self {
        Self {
            capacity: Self::default_capacity(),
            flow: Self::default_flow(),
            function: Self::default_function(),
            interval_ms: Self::default_interval_ms(),
            name: Self::default_name(),
            sink: Self::default_sink(),
            src: Self::default_src(),
            supply: Self::default_supply(),
            unit_cost: Self::default_unit_cost(),
        }
    }
}

impl GraphMetadataPinned {
    fn default_capacity() -> String {
        GraphMetadataStandard::DEFAULT_CAPACITY.into()
    }

    fn default_flow() -> String {
        GraphMetadataStandard::DEFAULT_FLOW.into()
    }

    fn default_function() -> String {
        GraphMetadataStandard::DEFAULT_FUNCTION.into()
    }

    fn default_interval_ms() -> String {
        GraphMetadataStandard::DEFAULT_INTERVAL_MS.into()
    }

    fn default_name() -> String {
        GraphMetadataStandard::DEFAULT_NAME.into()
    }

    fn default_sink() -> String {
        GraphMetadataStandard::DEFAULT_SINK.into()
    }

    fn default_src() -> String {
        GraphMetadataStandard::DEFAULT_SRC.into()
    }

    fn default_supply() -> String {
        GraphMetadataStandard::DEFAULT_SUPPLY.into()
    }

    fn default_unit_cost() -> String {
        GraphMetadataStandard::DEFAULT_UNIT_COST.into()
    }
}

impl GraphMetadataPinnedExt for GraphMetadataPinned {
    fn capacity(&self) -> &str {
        &self.capacity
    }

    fn flow(&self) -> &str {
        &self.flow
    }

    fn function(&self) -> &str {
        &self.function
    }

    fn interval_ms(&self) -> &str {
        &self.interval_ms
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn sink(&self) -> &str {
        &self.sink
    }

    fn src(&self) -> &str {
        &self.src
    }

    fn supply(&self) -> &str {
        &self.supply
    }

    fn unit_cost(&self) -> &str {
        &self.unit_cost
    }
}

#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct GraphMetadataStandard {}

impl GraphMetadataStandard {
    pub const DEFAULT_CAPACITY: &'static str = "capacity";
    pub const DEFAULT_FLOW: &'static str = "flow";
    pub const DEFAULT_FUNCTION: &'static str = "function";
    pub const DEFAULT_INTERVAL_MS: &'static str = "le";
    pub const DEFAULT_NAME: &'static str = "name";
    pub const DEFAULT_SINK: &'static str = "sink";
    pub const DEFAULT_SRC: &'static str = "src";
    pub const DEFAULT_SUPPLY: &'static str = "supply";
    pub const DEFAULT_UNIT_COST: &'static str = "unit_cost";
}

impl GraphMetadataPinnedExt for GraphMetadataStandard {
    fn capacity(&self) -> &str {
        Self::DEFAULT_CAPACITY
    }

    fn flow(&self) -> &str {
        Self::DEFAULT_FLOW
    }

    fn function(&self) -> &str {
        Self::DEFAULT_FUNCTION
    }

    fn interval_ms(&self) -> &str {
        Self::DEFAULT_INTERVAL_MS
    }

    fn name(&self) -> &str {
        Self::DEFAULT_NAME
    }

    fn sink(&self) -> &str {
        Self::DEFAULT_SINK
    }

    fn src(&self) -> &str {
        Self::DEFAULT_SRC
    }

    fn supply(&self) -> &str {
        Self::DEFAULT_SUPPLY
    }

    fn unit_cost(&self) -> &str {
        Self::DEFAULT_UNIT_COST
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct GraphFilter {
    pub name: Option<String>,
    pub namespace: Option<String>,
}

impl GraphFilter {
    pub fn contains(&self, key: &GraphScope) -> bool {
        let Self { name, namespace } = self;

        #[inline]
        fn test(a: &Option<String>, b: &String) -> bool {
            match a.as_ref() {
                Some(a) => a == b,
                None => true,
            }
        }

        test(namespace, &key.namespace) && test(name, &key.name)
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct GraphScope {
    pub namespace: String,
    pub name: String,
}

impl GraphScope {
    pub const NAME_GLOBAL: &'static str = "__global__";
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GraphNamespacedScope {
    pub name: String,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum GraphDataType {
    Edge,
    Node,
}
