use anyhow::Result;
use async_trait::async_trait;

use crate::{
    frame::LazyFrame,
    graph::{Graph, GraphMetadata, GraphMetadataRaw, GraphMetadataStandard},
    problem::VirtualProblem,
    resource::NetworkResourceCollectionDB,
};

#[async_trait]
pub trait NetworkAnalyzerExt
where
    Self: NetworkAnalyzer,
{
    async fn pin_graph(
        &self,
        problem: &VirtualProblem,
        graph: Graph<LazyFrame>,
    ) -> Result<Graph<LazyFrame, GraphMetadataStandard>> {
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
            }
            .cast(GraphMetadataStandard {})),
            GraphMetadata::Standard(metadata) => Ok(Graph {
                data,
                metadata,
                scope,
            }),
        }
    }
}

#[async_trait]
impl<T> NetworkAnalyzerExt for T where Self: NetworkAnalyzer {}

#[async_trait]
pub trait NetworkAnalyzer
where
    Self: Sync,
{
    async fn inspect(
        &self,
        resource_db: &dyn NetworkResourceCollectionDB,
    ) -> Result<Vec<VirtualProblem>>;

    async fn pin_graph_raw(
        &self,
        problem: &VirtualProblem,
        graph: Graph<LazyFrame, GraphMetadataRaw>,
    ) -> Result<Graph<LazyFrame, GraphMetadataStandard>>;
}
