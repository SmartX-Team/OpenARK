use anyhow::Result;
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;

use crate::{
    frame::LazyFrame,
    graph::{Graph, GraphMetadataExt},
};

#[async_trait]
pub trait NetworkVisualizer {
    async fn try_new(signal: &FunctionSignal) -> Result<Self>
    where
        Self: Sized;

    async fn register<M>(&self, graph: Graph<LazyFrame, M>) -> Result<()>
    where
        M: Send + Clone + GraphMetadataExt;

    async fn wait_to_next(&self);

    async fn close(&self) -> Result<()>;
}
