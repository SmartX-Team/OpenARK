use anyhow::Result;
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    frame::LazyFrame,
    graph::{Graph, GraphMetadataExt},
};

#[async_trait]
pub trait NetworkVisualizerExt
where
    Self: NetworkVisualizer,
{
    async fn wait_to_next(&self) -> Result<()> {
        self.call(NetworkVisualizerEvent::Next).await
    }
}

#[async_trait]
impl<T> NetworkVisualizerExt for T where Self: NetworkVisualizer {}

#[async_trait]
pub trait NetworkVisualizer
where
    Self: Sync,
{
    async fn try_new(signal: &FunctionSignal) -> Result<Self>
    where
        Self: Sized;

    async fn register<M>(&self, graph: Graph<LazyFrame, M>) -> Result<()>
    where
        M: Send + Clone + GraphMetadataExt;

    async fn call(&self, event: NetworkVisualizerEvent) -> Result<()>;

    async fn close(&self) -> Result<()>;
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub enum NetworkVisualizerEvent {
    Next,
}
