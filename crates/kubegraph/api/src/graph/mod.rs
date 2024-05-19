#[cfg(feature = "df-polars")]
pub mod polars;

use anyhow::Result;
use async_trait::async_trait;
use futures::try_join;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    frame::{DataFrame, LazyFrame},
    function::FunctionMetadata,
    problem::r#virtual::VirtualProblem,
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
pub struct Graph<T> {
    pub data: GraphData<T>,
    pub metadata: GraphMetadata,
    pub scope: GraphScope,
}

impl Graph<LazyFrame> {
    pub fn cast(self, problem: &VirtualProblem) -> Self {
        let Self {
            data,
            metadata,
            scope,
        } = self;
        Self {
            data: data.cast(&metadata, problem),
            metadata,
            scope,
        }
    }

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

impl GraphData<LazyFrame> {
    pub fn cast(self, origin: &GraphMetadata, problem: &VirtualProblem) -> Self {
        let Self { edges, nodes } = self;
        Self {
            edges: edges.cast(GraphDataType::Edge, origin, problem),
            nodes: nodes.cast(GraphDataType::Node, origin, problem),
        }
    }

    pub async fn collect(self) -> Result<GraphData<DataFrame>> {
        let Self { edges, nodes } = self;
        let (edges, nodes) = try_join!(edges.collect(), nodes.collect(),)?;
        Ok(GraphData { edges, nodes })
    }
}

impl FromIterator<Graph<LazyFrame>> for Result<GraphData<LazyFrame>> {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = Graph<LazyFrame>>,
    {
        iter.into_iter().map(|item| item.data).collect()
    }
}

impl FromIterator<GraphData<LazyFrame>> for Result<GraphData<LazyFrame>> {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = GraphData<LazyFrame>>,
    {
        let mut edges = LazyFrame::Empty;
        let mut nodes = LazyFrame::Empty;

        for GraphData { edges: e, nodes: n } in iter {
            edges = edges.concat(e)?;
            nodes = nodes.concat(n)?;
        }
        Ok(GraphData { edges, nodes })
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct GraphMetadata {
    #[serde(default = "GraphMetadata::default_capacity")]
    pub capacity: String,
    #[serde(default = "GraphMetadata::default_flow")]
    pub flow: String,
    #[serde(default = "GraphMetadata::default_function")]
    pub function: String,
    #[serde(default = "GraphMetadata::default_name")]
    pub name: String,
    #[serde(default = "GraphMetadata::default_sink")]
    pub sink: String,
    #[serde(default = "GraphMetadata::default_src")]
    pub src: String,
    #[serde(default = "GraphMetadata::default_supply")]
    pub supply: String,
    #[serde(default = "GraphMetadata::default_unit_cost")]
    pub unit_cost: String,
}

impl Default for GraphMetadata {
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

impl GraphMetadata {
    fn default_capacity() -> String {
        "capacity".into()
    }

    fn default_flow() -> String {
        "flow".into()
    }

    fn default_function() -> String {
        "function".into()
    }

    fn default_name() -> String {
        "name".into()
    }

    fn default_sink() -> String {
        "sink".into()
    }

    fn default_src() -> String {
        "src".into()
    }

    fn default_supply() -> String {
        "supply".into()
    }

    fn default_unit_cost() -> String {
        "unit_cost".into()
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
