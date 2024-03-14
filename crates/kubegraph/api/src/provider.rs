use anyhow::Result;
use async_trait::async_trait;

use crate::graph::{NetworkEdgeKey, NetworkGraphRow, NetworkValue};

#[async_trait]
pub trait NetworkGraphProvider
where
    Self: Send + Sync,
{
    async fn add_edges(
        &self,
        edges: impl Send + IntoIterator<Item = (NetworkEdgeKey, NetworkValue)>,
    ) -> Result<()>;

    async fn get_edges(&self) -> Vec<NetworkGraphRow>;

    async fn close(self) -> Result<()>;
}
