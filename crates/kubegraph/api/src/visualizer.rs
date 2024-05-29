use anyhow::Result;
use async_trait::async_trait;

use crate::{
    frame::LazyFrame,
    graph::{Graph, GraphMetadataExt},
};

#[async_trait]
pub trait NetworkVisualizer {
    async fn try_default() -> Result<Self>
    where
        Self: Sized;

    async fn register<M>(&self, graph: Graph<LazyFrame, M>) -> Result<()>
    where
        M: Send + Clone + GraphMetadataExt;

    async fn close(&self) -> Result<()>;
}
