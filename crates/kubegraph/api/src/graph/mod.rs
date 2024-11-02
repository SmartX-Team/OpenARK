#[cfg(feature = "df-polars")]
pub mod polars;

use std::{collections::BTreeMap, fmt, mem::swap, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use futures::try_join;
use kube::ResourceExt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{instrument, Level};

use crate::{
    connector::NetworkConnectorCrd,
    frame::{DataFrame, LazyFrame},
    function::FunctionMetadata,
    vm::{Feature, Number},
};

pub struct ScopedNetworkGraphDBContainer<'a, T>
where
    T: NetworkGraphDB,
{
    pub(crate) inner: &'a T,
    pub(crate) scope: &'a GraphScope,
}

#[async_trait]
impl<'a, DB, T, M> ScopedNetworkGraphDB<T, M> for ScopedNetworkGraphDBContainer<'a, DB>
where
    DB: NetworkGraphDB,
{
    #[instrument(level = Level::INFO, skip(self, graph))]
    async fn insert(&self, graph: Graph<GraphData<T>, M>) -> Result<()>
    where
        T: 'async_trait + Send + Into<LazyFrame>,
        M: 'async_trait + Send + Into<GraphMetadata>,
    {
        let Self {
            inner,
            scope: GraphScope { namespace, name: _ },
        } = self;
        let Graph {
            connector,
            data: GraphData { edges, nodes },
            metadata,
            scope: GraphScope { namespace: _, name },
        } = graph;

        let graph = Graph {
            connector,
            data: GraphData {
                edges: edges.into(),
                nodes: nodes.into(),
            },
            metadata: metadata.into(),
            scope: GraphScope {
                namespace: namespace.clone(),
                name,
            },
        };
        inner.insert(graph).await
    }
}

#[async_trait]
pub trait ScopedNetworkGraphDB<T, M>
where
    Self: Sync,
{
    async fn insert(&self, graph: Graph<GraphData<T>, M>) -> Result<()>
    where
        T: 'async_trait + Send + Into<LazyFrame>,
        M: 'async_trait + Send + Into<GraphMetadata>;
}

#[async_trait]
pub trait NetworkGraphDBExt
where
    Self: NetworkGraphDB,
{
    #[instrument(level = Level::INFO, skip(self))]
    async fn get_global_namespaced(
        &self,
        namespace: &str,
    ) -> Result<Option<Graph<GraphData<LazyFrame>>>> {
        let scope = GraphScope {
            namespace: namespace.into(),
            name: GraphScope::NAME_GLOBAL.into(),
        };
        self.get(&scope).await
    }
}

#[async_trait]
impl<T> NetworkGraphDBExt for T where Self: NetworkGraphDB {}

#[async_trait]
pub trait NetworkGraphDB
where
    Self: Sync,
{
    async fn get(&self, scope: &GraphScope) -> Result<Option<Graph<GraphData<LazyFrame>>>>;

    async fn insert(&self, graph: Graph<GraphData<LazyFrame>>) -> Result<()>;

    async fn list(&self, filter: &GraphFilter) -> Result<Vec<Graph<GraphData<LazyFrame>>>>;

    async fn remove(&self, scope: GraphScope) -> Result<()>;

    async fn close(&self) -> Result<()>;
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
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
    pub fn mark_as_static<M>(self, metadata: &M, namespace: impl Into<String>) -> Result<Self>
    where
        M: GraphMetadataExt,
    {
        let function = FunctionMetadata {
            scope: GraphScope {
                namespace: namespace.into(),
                name: FunctionMetadata::NAME_STATIC.into(),
            },
        };

        match self.0 {
            LazyFrame::Empty => Ok(self),
            mut edges => edges
                .alias_function(metadata, &function)
                .map(|()| Self::new(edges)),
        }
    }

    pub fn concat(self, other: Self) -> Result<Self> {
        self.0.concat(other.0).map(Self)
    }
}

impl Extend<Self> for GraphEdges<LazyFrame> {
    fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = Self>,
    {
        let mut src = Self(LazyFrame::Empty);
        swap(self, &mut src);

        *self = Some(src).into_iter().chain(iter).collect();
    }
}

impl FromIterator<Self> for GraphEdges<LazyFrame> {
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = Self>,
    {
        let mut iter = iter
            .into_iter()
            .filter(|Self(edges)| !matches!(edges, LazyFrame::Empty))
            .peekable();

        match iter.peek() {
            Some(Self(LazyFrame::Empty)) | None => Self(LazyFrame::Empty),
            #[cfg(feature = "df-polars")]
            Some(Self(LazyFrame::Polars(_))) => iter
                .filter_map(|Self(edges)| edges.try_into_polars().ok().map(GraphEdges))
                .collect(),
        }
    }
}

pub trait IntoGraph<T> {
    /// Disaggregate two dataframes.
    fn try_into_graph(self) -> Result<GraphData<T>>
    where
        Self: Sized;
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Graph<T, M = GraphMetadata> {
    #[serde(default)]
    pub connector: Option<Arc<NetworkConnectorCrd>>,
    pub data: T,
    pub metadata: M,
    pub scope: GraphScope,
}

#[cfg(feature = "petgraph")]
impl<M> TryFrom<Graph<GraphData<LazyFrame>, M>>
    for ::petgraph::stable_graph::StableDiGraph<GraphEntry, GraphEntry>
where
    M: GraphMetadataExt,
{
    type Error = ::anyhow::Error;

    fn try_from(graph: Graph<GraphData<LazyFrame>, M>) -> Result<Self, Self::Error> {
        let Graph {
            connector: _,
            data: GraphData { edges, nodes },
            metadata,
            scope: _,
        } = graph;

        let mut graph = ::petgraph::stable_graph::StableDiGraph::default();

        let name_map = match nodes {
            LazyFrame::Empty => GraphNameMap::default(),
            #[cfg(feature = "df-polars")]
            LazyFrame::Polars(df) => self::polars::transform_petgraph_nodes(
                &mut graph, &metadata, df,
            )
            .map_err(|error| {
                ::anyhow::anyhow!(
                    "failed to transform polars nodes dataframe into petgraph: {error}"
                )
            })?,
        };
        match edges {
            LazyFrame::Empty => (),
            #[cfg(feature = "df-polars")]
            LazyFrame::Polars(df) => {
                self::polars::transform_petgraph_edges(&mut graph, &metadata, name_map, df)
                    .map_err(|error| {
                        ::anyhow::anyhow!(
                            "failed to transform polars edges dataframe into petgraph: {error}"
                        )
                    })?
            }
        }
        Ok(graph)
    }
}

impl<M> Graph<GraphData<DataFrame>, M> {
    pub fn drop_null_columns(self) -> Self {
        let Self {
            connector,
            data,
            metadata,
            scope,
        } = self;
        Self {
            connector,
            data: data.drop_null_columns(),
            metadata,
            scope,
        }
    }

    pub fn lazy(self) -> Graph<GraphData<LazyFrame>, M> {
        let Self {
            connector,
            data,
            metadata,
            scope,
        } = self;
        Graph {
            connector,
            data: data.lazy(),
            metadata,
            scope,
        }
    }
}

impl<M> Graph<GraphData<LazyFrame>, M>
where
    M: GraphMetadataPinnedExt,
{
    pub fn cast<MT>(self, to: MT) -> Graph<GraphData<LazyFrame>, MT>
    where
        MT: GraphMetadataPinnedExt,
    {
        let Self {
            connector,
            data,
            metadata,
            scope,
        } = self;
        Graph {
            connector,
            data: data.cast(&metadata, &to),
            metadata: to,
            scope,
        }
    }
}

impl<M> Graph<GraphData<LazyFrame>, M> {
    pub async fn collect(self) -> Result<Graph<GraphData<DataFrame>, M>> {
        let Self {
            connector,
            data,
            metadata,
            scope,
        } = self;
        Ok(Graph {
            connector,
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
        MF: GraphMetadataExt,
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

pub trait GraphMetadataExt
where
    Self: Into<GraphMetadata>,
{
    fn all(&self) -> Vec<String> {
        let mut values = vec![
            self.capacity().into(),
            self.connector().into(),
            self.flow().into(),
            self.function().into(),
            self.interval_ms().into(),
            self.name().into(),
            self.sink().into(),
            self.src().into(),
            self.supply().into(),
            self.unit_cost().into(),
        ];
        if let Some(extras) = self.extras() {
            values.extend(extras.values().cloned())
        }
        values
    }

    fn all_cores(&self) -> [&str; 10] {
        [
            self.capacity(),
            self.connector(),
            self.flow(),
            self.function(),
            self.interval_ms(),
            self.name(),
            self.sink(),
            self.src(),
            self.supply(),
            self.unit_cost(),
        ]
    }

    fn all_node_inputs(&self) -> [&str; 9] {
        [
            self.capacity(),
            self.connector(),
            self.function(),
            self.interval_ms(),
            self.name(),
            self.sink(),
            self.src(),
            self.supply(),
            self.unit_cost(),
        ]
    }

    fn all_node_inputs_raw(&self) -> Vec<String> {
        let mut values = vec![
            self.connector().into(),
            self.interval_ms().into(),
            self.name().into(),
            self.sink().into(),
            self.src().into(),
        ];
        if let Some(extras) = self.extras() {
            values.extend(extras.values().cloned())
        }
        values
    }

    fn extras(&self) -> Option<&BTreeMap<String, String>>;

    fn capacity(&self) -> &str {
        self.extras()
            .and_then(|extras| extras.get("capacity"))
            .map(|value| value.as_str())
            .unwrap_or(GraphMetadataStandard::DEFAULT_CAPACITY)
    }

    fn connector(&self) -> &str {
        self.extras()
            .and_then(|extras| extras.get("connector"))
            .map(|value| value.as_str())
            .unwrap_or(GraphMetadataStandard::DEFAULT_CONNECTOR)
    }

    fn flow(&self) -> &str {
        self.extras()
            .and_then(|extras| extras.get("flow"))
            .map(|value| value.as_str())
            .unwrap_or(GraphMetadataStandard::DEFAULT_FLOW)
    }

    fn function(&self) -> &str {
        self.extras()
            .and_then(|extras| extras.get("function"))
            .map(|value| value.as_str())
            .unwrap_or(GraphMetadataStandard::DEFAULT_FUNCTION)
    }

    fn interval_ms(&self) -> &str;

    fn name(&self) -> &str;

    fn sink(&self) -> &str;

    fn src(&self) -> &str;

    fn supply(&self) -> &str {
        self.extras()
            .and_then(|extras| extras.get("supply"))
            .map(|value| value.as_str())
            .unwrap_or(GraphMetadataStandard::DEFAULT_SUPPLY)
    }

    fn unit_cost(&self) -> &str {
        self.extras()
            .and_then(|extras| extras.get("unitCost"))
            .map(|value| value.as_str())
            .unwrap_or(GraphMetadataStandard::DEFAULT_UNIT_COST)
    }

    fn to_raw(&self) -> GraphMetadataRaw;

    fn to_pinned(&self) -> GraphMetadataPinned {
        GraphMetadataPinned {
            capacity: self.capacity().into(),
            connector: self.connector().into(),
            flow: self.flow().into(),
            function: self.function().into(),
            interval_ms: self.interval_ms().into(),
            name: self.name().into(),
            sink: self.sink().into(),
            src: self.src().into(),
            supply: self.supply().into(),
            unit_cost: self.unit_cost().into(),
        }
    }
}

impl GraphMetadataExt for GraphMetadata {
    fn all(&self) -> Vec<String> {
        match self {
            GraphMetadata::Raw(m) => m.all(),
            GraphMetadata::Pinned(m) => m.all(),
            GraphMetadata::Standard(m) => m.all(),
        }
    }

    fn all_cores(&self) -> [&str; 10] {
        match self {
            GraphMetadata::Raw(m) => m.all_cores(),
            GraphMetadata::Pinned(m) => m.all_cores(),
            GraphMetadata::Standard(m) => m.all_cores(),
        }
    }

    fn all_node_inputs(&self) -> [&str; 9] {
        match self {
            GraphMetadata::Raw(m) => m.all_node_inputs(),
            GraphMetadata::Pinned(m) => m.all_node_inputs(),
            GraphMetadata::Standard(m) => m.all_node_inputs(),
        }
    }

    fn all_node_inputs_raw(&self) -> Vec<String> {
        match self {
            GraphMetadata::Raw(m) => m.all_node_inputs_raw(),
            GraphMetadata::Pinned(m) => m.all_node_inputs_raw(),
            GraphMetadata::Standard(m) => m.all_node_inputs_raw(),
        }
    }

    fn extras(&self) -> Option<&BTreeMap<String, String>> {
        match self {
            GraphMetadata::Raw(m) => m.extras(),
            GraphMetadata::Pinned(m) => m.extras(),
            GraphMetadata::Standard(m) => m.extras(),
        }
    }

    fn capacity(&self) -> &str {
        match self {
            GraphMetadata::Raw(m) => m.capacity(),
            GraphMetadata::Pinned(m) => GraphMetadataExt::capacity(m),
            GraphMetadata::Standard(m) => GraphMetadataExt::capacity(m),
        }
    }

    fn connector(&self) -> &str {
        match self {
            GraphMetadata::Raw(m) => m.connector(),
            GraphMetadata::Pinned(m) => GraphMetadataExt::connector(m),
            GraphMetadata::Standard(m) => GraphMetadataExt::connector(m),
        }
    }

    fn flow(&self) -> &str {
        match self {
            GraphMetadata::Raw(m) => m.flow(),
            GraphMetadata::Pinned(m) => GraphMetadataExt::flow(m),
            GraphMetadata::Standard(m) => GraphMetadataExt::flow(m),
        }
    }

    fn function(&self) -> &str {
        match self {
            GraphMetadata::Raw(m) => m.function(),
            GraphMetadata::Pinned(m) => GraphMetadataExt::function(m),
            GraphMetadata::Standard(m) => GraphMetadataExt::function(m),
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

    fn supply(&self) -> &str {
        match self {
            GraphMetadata::Raw(m) => m.supply(),
            GraphMetadata::Pinned(m) => GraphMetadataExt::supply(m),
            GraphMetadata::Standard(m) => GraphMetadataExt::supply(m),
        }
    }

    fn unit_cost(&self) -> &str {
        match self {
            GraphMetadata::Raw(m) => m.unit_cost(),
            GraphMetadata::Pinned(m) => GraphMetadataExt::unit_cost(m),
            GraphMetadata::Standard(m) => GraphMetadataExt::unit_cost(m),
        }
    }

    fn to_raw(&self) -> GraphMetadataRaw {
        match self {
            GraphMetadata::Raw(m) => m.to_raw(),
            GraphMetadata::Pinned(m) => m.to_raw(),
            GraphMetadata::Standard(m) => m.to_raw(),
        }
    }

    fn to_pinned(&self) -> GraphMetadataPinned {
        match self {
            GraphMetadata::Raw(m) => m.to_pinned(),
            GraphMetadata::Pinned(m) => m.to_pinned(),
            GraphMetadata::Standard(m) => m.to_pinned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphMetadataRaw {
    #[serde(default, flatten)]
    pub extras: BTreeMap<String, String>,
    #[serde(default = "GraphMetadataPinned::default_interval_ms", rename = "le")]
    pub interval_ms: String,
    #[serde(default = "GraphMetadataPinned::default_name")]
    pub name: String,
    #[serde(default = "GraphMetadataPinned::default_sink")]
    pub sink: String,
    #[serde(default = "GraphMetadataPinned::default_src")]
    pub src: String,
}

impl From<GraphMetadataRaw> for GraphMetadata {
    fn from(value: GraphMetadataRaw) -> Self {
        Self::Raw(value)
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

    fn to_raw(&self) -> GraphMetadataRaw {
        self.clone()
    }
}

impl GraphMetadataRaw {
    #[cfg(feature = "df-polars")]
    pub fn from_polars(df: &::pl::frame::DataFrame) -> Self {
        let mut metadata = Self::default();
        for column in df.get_columns() {
            let key = column.name();
            if column.is_empty() || matches!(column.dtype(), ::pl::datatypes::DataType::Null) {
                continue;
            }

            metadata.insert(key.to_string(), key.to_string());
        }
        metadata
    }

    fn insert(&mut self, key: String, value: String) {
        match key.as_str() {
            GraphMetadataStandard::DEFAULT_INTERVAL_MS => self.interval_ms = value,
            GraphMetadataStandard::DEFAULT_NAME => self.name = value,
            GraphMetadataStandard::DEFAULT_SINK => self.sink = value,
            GraphMetadataStandard::DEFAULT_SRC => self.src = value,
            _ => {
                self.extras.insert(key, value);
            }
        }
    }
}

mod impl_json_schema_for_graph_metadata_raw {
    use std::{borrow::Cow, collections::BTreeMap};

    use schemars::{gen::SchemaGenerator, schema::Schema, JsonSchema};

    #[allow(dead_code)]
    #[derive(JsonSchema)]
    #[serde(transparent)]
    struct GraphMetadataRaw(#[validate(inner(length(min = 1)))] BTreeMap<String, String>);

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

pub trait GraphMetadataPinnedExt
where
    Self: Into<GraphMetadata>,
{
    fn capacity(&self) -> &str;

    fn connector(&self) -> &str;

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
    Self: GraphMetadataPinnedExt,
{
    fn all_node_inputs_raw(&self) -> Vec<String> {
        vec![
            self.capacity().into(),
            self.connector().into(),
            self.interval_ms().into(),
            self.name().into(),
            self.sink().into(),
            self.src().into(),
            self.supply().into(),
            self.unit_cost().into(),
        ]
    }

    fn extras(&self) -> Option<&BTreeMap<String, String>> {
        None
    }

    fn capacity(&self) -> &str {
        GraphMetadataPinnedExt::capacity(self)
    }

    fn connector(&self) -> &str {
        GraphMetadataPinnedExt::connector(self)
    }

    fn flow(&self) -> &str {
        GraphMetadataPinnedExt::flow(self)
    }

    fn function(&self) -> &str {
        GraphMetadataPinnedExt::function(self)
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

    fn supply(&self) -> &str {
        GraphMetadataPinnedExt::supply(self)
    }

    fn unit_cost(&self) -> &str {
        GraphMetadataPinnedExt::unit_cost(self)
    }

    fn to_raw(&self) -> GraphMetadataRaw {
        let extras = vec![
            (
                GraphMetadataStandard::DEFAULT_CAPACITY.into(),
                self.capacity().into(),
            ),
            (
                GraphMetadataStandard::DEFAULT_CONNECTOR.into(),
                self.connector().into(),
            ),
            (
                GraphMetadataStandard::DEFAULT_FLOW.into(),
                self.flow().into(),
            ),
            (
                GraphMetadataStandard::DEFAULT_FUNCTION.into(),
                self.function().into(),
            ),
            (
                GraphMetadataStandard::DEFAULT_SUPPLY.into(),
                self.supply().into(),
            ),
            (
                GraphMetadataStandard::DEFAULT_UNIT_COST.into(),
                self.unit_cost().into(),
            ),
        ]
        .into_iter()
        .collect();

        GraphMetadataRaw {
            extras,
            interval_ms: self.interval_ms().into(),
            name: self.name().into(),
            sink: self.sink().into(),
            src: self.src().into(),
        }
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct GraphMetadataPinned {
    #[serde(default = "GraphMetadataPinned::default_capacity")]
    #[validate(length(min = 1))]
    pub capacity: String,
    #[serde(default = "GraphMetadataPinned::default_connector")]
    #[validate(length(min = 1))]
    pub connector: String,
    #[serde(default = "GraphMetadataPinned::default_flow")]
    #[validate(length(min = 1))]
    pub flow: String,
    #[serde(default = "GraphMetadataPinned::default_function")]
    #[validate(length(min = 1))]
    pub function: String,
    #[serde(default = "GraphMetadataPinned::default_interval_ms", rename = "le")]
    #[validate(length(min = 1))]
    pub interval_ms: String,
    #[serde(default = "GraphMetadataPinned::default_name")]
    #[validate(length(min = 1))]
    pub name: String,
    #[serde(default = "GraphMetadataPinned::default_sink")]
    #[validate(length(min = 1))]
    pub sink: String,
    #[serde(default = "GraphMetadataPinned::default_src")]
    #[validate(length(min = 1))]
    pub src: String,
    #[serde(default = "GraphMetadataPinned::default_supply")]
    #[validate(length(min = 1))]
    pub supply: String,
    #[serde(default = "GraphMetadataPinned::default_unit_cost")]
    #[validate(length(min = 1))]
    pub unit_cost: String,
}

impl From<GraphMetadataPinned> for GraphMetadata {
    fn from(value: GraphMetadataPinned) -> Self {
        Self::Pinned(value)
    }
}

impl Default for GraphMetadataPinned {
    fn default() -> Self {
        Self {
            capacity: Self::default_capacity(),
            connector: Self::default_connector(),
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
    pub fn default_capacity() -> String {
        GraphMetadataStandard::DEFAULT_CAPACITY.into()
    }

    pub fn default_connector() -> String {
        GraphMetadataStandard::DEFAULT_CONNECTOR.into()
    }

    pub fn default_flow() -> String {
        GraphMetadataStandard::DEFAULT_FLOW.into()
    }

    pub fn default_function() -> String {
        GraphMetadataStandard::DEFAULT_FUNCTION.into()
    }

    pub fn default_interval_ms() -> String {
        GraphMetadataStandard::DEFAULT_INTERVAL_MS.into()
    }

    pub fn default_name() -> String {
        GraphMetadataStandard::DEFAULT_NAME.into()
    }

    pub fn default_sink() -> String {
        GraphMetadataStandard::DEFAULT_SINK.into()
    }

    pub fn default_src() -> String {
        GraphMetadataStandard::DEFAULT_SRC.into()
    }

    pub fn default_supply() -> String {
        GraphMetadataStandard::DEFAULT_SUPPLY.into()
    }

    pub fn default_unit_cost() -> String {
        GraphMetadataStandard::DEFAULT_UNIT_COST.into()
    }
}

impl GraphMetadataPinnedExt for GraphMetadataPinned {
    fn capacity(&self) -> &str {
        &self.capacity
    }

    fn connector(&self) -> &str {
        &self.connector
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
    pub const DEFAULT_CONNECTOR: &'static str = "connector";
    pub const DEFAULT_FLOW: &'static str = "flow";
    pub const DEFAULT_FUNCTION: &'static str = "function";
    pub const DEFAULT_INTERVAL_MS: &'static str = "le";
    pub const DEFAULT_NAME: &'static str = "name";
    pub const DEFAULT_SINK: &'static str = "sink";
    pub const DEFAULT_SRC: &'static str = "src";
    pub const DEFAULT_SUPPLY: &'static str = "supply";
    pub const DEFAULT_UNIT_COST: &'static str = "unit_cost";
}

impl From<GraphMetadataStandard> for GraphMetadata {
    fn from(value: GraphMetadataStandard) -> Self {
        Self::Standard(value)
    }
}

impl GraphMetadataPinnedExt for GraphMetadataStandard {
    fn capacity(&self) -> &str {
        Self::DEFAULT_CAPACITY
    }

    fn connector(&self) -> &str {
        Self::DEFAULT_CONNECTOR
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
    pub namespace: String,
    #[serde(default)]
    pub name: Option<String>,
}

impl GraphFilter {
    pub const fn all(namespace: String) -> Self {
        Self {
            namespace,
            name: None,
        }
    }

    pub fn contains(&self, key: &GraphScope) -> bool {
        let Self { namespace, name } = self;

        #[inline]
        fn test(a: Option<&String>, b: &String) -> bool {
            match a {
                Some(a) => a.is_empty() || a == b,
                None => true,
            }
        }

        test(Some(namespace), &key.namespace) && test(name.as_ref(), &key.name)
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

impl fmt::Display for GraphScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { namespace, name } = self;
        write!(f, "{namespace}/{name}")
    }
}

impl GraphScope {
    pub const NAME_GLOBAL: &'static str = "__global__";

    pub fn from_resource<K>(object: &K) -> Self
    where
        K: ResourceExt,
    {
        Self {
            namespace: Self::parse_namespace(object),
            name: Self::parse_name(object),
        }
    }

    pub fn parse_namespace<K>(object: &K) -> String
    where
        K: ResourceExt,
    {
        object.namespace().unwrap_or_else(|| "default".into())
    }

    pub fn parse_name<K>(object: &K) -> String
    where
        K: ResourceExt,
    {
        object.name_any()
    }
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

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GraphEntry {
    #[serde(flatten)]
    pub others: BTreeMap<String, GraphEntryValue>,
}

impl GraphEntry {
    #[cfg(feature = "petgraph")]
    fn get_as_petgraph(
        &self,
        name_map: &GraphNameMap,
        key: &str,
    ) -> Result<::petgraph::graph::NodeIndex> {
        self.others
            .get(key)
            .ok_or_else(|| ::anyhow::anyhow!("failed to get graph entry column {key}"))
            .and_then(|name| {
                name.as_string()
                    .ok_or_else(|| {
                        ::anyhow::anyhow!("failed to assert that graph node is a string {key}")
                    })
                    .and_then(|name| {
                        name_map.data.get(name).ok_or_else(|| {
                            ::anyhow::anyhow!("failed to find the graph node {name:?}")
                        })
                    })
            })
            .copied()
            .map(::petgraph::graph::NodeIndex::new)
    }

    pub fn name(&self) -> Option<&String> {
        self.others
            .get(GraphMetadataStandard::DEFAULT_NAME)
            .and_then(|value| match value {
                GraphEntryValue::String(value) => Some(value),
                _ => None,
            })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum GraphEntryValue {
    Feature(Feature),
    Number(Number),
    String(String),
}

impl GraphEntryValue {
    pub const fn as_feature(&self) -> Option<Feature> {
        match self {
            Self::Feature(value) => Some(*value),
            _ => None,
        }
    }

    pub const fn as_number(&self) -> Option<Number> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }

    pub const fn as_string(&self) -> Option<&String> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }
}

#[cfg(feature = "petgraph")]
#[derive(Default)]
struct GraphNameMap {
    data: BTreeMap<String, usize>,
}

#[cfg(feature = "petgraph")]
impl FromIterator<(String, usize)> for GraphNameMap {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = (String, usize)>,
    {
        Self {
            data: iter.into_iter().collect(),
        }
    }
}
