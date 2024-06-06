#[cfg(feature = "analyzer-llm")]
pub mod llm;

use anyhow::Result;
use async_trait::async_trait;
use futures::{stream::FuturesUnordered, TryStreamExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{instrument, Level};

use crate::{
    frame::LazyFrame,
    graph::{
        Graph, GraphFilter, GraphMetadata, GraphMetadataExt, GraphMetadataPinned, GraphMetadataRaw,
        GraphScope,
    },
    problem::{NetworkProblemCrd, ProblemSpec, VirtualProblem},
    resource::NetworkResourceCollectionDB,
};

#[async_trait]
pub trait NetworkAnalyzerExt
where
    Self: NetworkAnalyzer,
{
    #[instrument(level = Level::INFO, skip(self, resource_db))]
    async fn inspect(
        &self,
        resource_db: &dyn NetworkResourceCollectionDB,
    ) -> Result<Vec<VirtualProblem>> {
        // TODO: to be implemented (aggregating other problems)
        let problems = resource_db.list(()).await;

        match problems {
            Some(problems) if !problems.is_empty() => {
                problems
                    .into_iter()
                    .map(pin_problem)
                    .map(|problem| self.pin_virtual_problem(problem))
                    .collect::<FuturesUnordered<_>>()
                    .try_collect()
                    .await
            }
            Some(_) | None => return Ok(Vec::default()),
        }
    }

    #[instrument(level = Level::INFO, skip(self, problem, graph))]
    async fn pin_graph(
        &self,
        problem: &VirtualProblem,
        graph: Graph<LazyFrame>,
    ) -> Result<Graph<LazyFrame, GraphMetadataPinned>> {
        let Graph {
            data,
            metadata,
            scope,
        } = graph;
        match metadata {
            GraphMetadata::Raw(metadata) => {
                let graph = Graph {
                    data,
                    metadata,
                    scope,
                };
                self.pin_graph_raw(problem, graph).await
            }
            GraphMetadata::Pinned(metadata) => Ok(Graph {
                data,
                metadata,
                scope,
            }),
            GraphMetadata::Standard(metadata) => Ok(Graph {
                data,
                metadata: metadata.to_pinned(),
                scope,
            }),
        }
    }

    #[instrument(level = Level::INFO, skip(self, problem))]
    async fn pin_virtual_problem(
        &self,
        problem: VirtualProblem<(), GraphMetadataRaw>,
    ) -> Result<VirtualProblem> {
        let VirtualProblem {
            analyzer: (),
            filter,
            scope,
            spec: ProblemSpec { metadata, verbose },
        } = problem;

        let (analyzer, metadata) = self.pin_graph_metadata_raw(metadata).await?;

        Ok(VirtualProblem {
            analyzer,
            filter,
            scope,
            spec: ProblemSpec { metadata, verbose },
        })
    }
}

#[async_trait]
impl<T> NetworkAnalyzerExt for T where Self: NetworkAnalyzer {}

#[async_trait]
pub trait NetworkAnalyzer
where
    Self: Sync,
{
    type Spec: Send;

    async fn pin_graph_raw(
        &self,
        problem: &VirtualProblem,
        graph: Graph<LazyFrame, GraphMetadataRaw>,
    ) -> Result<Graph<LazyFrame, GraphMetadataPinned>>;

    async fn pin_graph_metadata_raw(
        &self,
        metadata: GraphMetadataRaw,
    ) -> Result<(VirtualProblemAnalyzer, GraphMetadataPinned)>;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VirtualProblemAnalyzer<T = VirtualProblemAnalyzerType> {
    pub original_metadata: GraphMetadataRaw,
    pub r#type: T,
}

impl VirtualProblemAnalyzer {
    pub const fn name(&self) -> &'static str {
        self.r#type.name()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum VirtualProblemAnalyzerType {
    Empty,
    #[cfg(feature = "analyzer-llm")]
    LLM(self::llm::VirtualProblemAnalyzerLLM),
}

impl VirtualProblemAnalyzerType {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Empty => "empty",
            #[cfg(feature = "analyzer-llm")]
            Self::LLM(_) => "llm",
        }
    }
}

fn pin_problem(problem: NetworkProblemCrd) -> VirtualProblem<(), GraphMetadataRaw> {
    let scope = GraphScope::from_resource(&problem);

    VirtualProblem {
        analyzer: (),
        filter: GraphFilter::all(scope.namespace.clone()),
        scope,
        spec: problem.spec,
    }
}
