use anyhow::Result;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{
    connector::NetworkConnectorDB,
    frame::LazyFrame,
    graph::{Graph, GraphMetadata, GraphMetadataRaw, GraphMetadataStandard, GraphScope},
};

use super::ProblemSpec;

#[async_trait]
pub trait NetworkVirtualProblemExt {
    async fn pin_graph(
        &self,
        graph: Graph<LazyFrame>,
    ) -> Result<Graph<LazyFrame, GraphMetadataStandard>>;
}

#[async_trait]
impl<T> NetworkVirtualProblemExt for T
where
    T: NetworkVirtualProblem,
{
    async fn pin_graph(
        &self,
        graph: Graph<LazyFrame>,
    ) -> Result<Graph<LazyFrame, GraphMetadataStandard>> {
        let Graph {
            data,
            metadata,
            scope,
        } = graph;
        match metadata {
            GraphMetadata::Raw(metadata) => {
                self.pin_graph_raw(Graph {
                    data,
                    metadata,
                    scope,
                })
                .await
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
pub trait NetworkVirtualProblem
where
    Self: Sync,
{
    async fn inspect(
        &self,
        connector_db: &dyn NetworkConnectorDB,
        scope: GraphScope,
    ) -> Result<VirtualProblem>;

    async fn pin_graph_raw(
        &self,
        graph: Graph<LazyFrame, GraphMetadataRaw>,
    ) -> Result<Graph<LazyFrame, GraphMetadataStandard>>;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(bound = "M: Default + JsonSchema")]
#[serde(
    rename_all = "camelCase",
    bound = "M: Default + Serialize + DeserializeOwned"
)]
pub struct VirtualProblem<M = GraphMetadataStandard> {
    #[serde(flatten)]
    pub scope: GraphScope,
    #[serde(default)]
    pub spec: ProblemSpec<M>,
}
