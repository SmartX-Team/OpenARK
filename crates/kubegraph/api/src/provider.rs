use anyhow::Result;
use async_trait::async_trait;

use crate::{
    connector::{NetworkConnectorSpec, NetworkConnectorTypeRef},
    graph::{NetworkEdgeKey, NetworkGraphRow, NetworkValue},
};

#[async_trait]
pub trait NetworkGraphProvider
where
    Self: Send + Sync,
{
    async fn add_connector(&self, namespace: String, name: String, spec: NetworkConnectorSpec);

    async fn add_edges(
        &self,
        edges: impl Send + IntoIterator<Item = (NetworkEdgeKey, NetworkValue)>,
    ) -> Result<()>;

    async fn delete_connector(&self, namespace: String, name: String);

    async fn get_connectors(
        &self,
        r#type: NetworkConnectorTypeRef,
    ) -> Option<Vec<NetworkConnectorSpec>>;

    async fn get_edges(&self) -> Vec<NetworkGraphRow>;

    async fn close(self) -> Result<()>;
}
