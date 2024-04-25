use anyhow::Result;
use async_trait::async_trait;

use crate::{
    connector::{NetworkConnectorSourceRef, NetworkConnectorSpec},
    graph::{NetworkEntry, NetworkEntryKeyFilter, NetworkEntryMap},
};

#[async_trait]
pub trait NetworkGraphProvider
where
    Self: Send + Sync,
{
    async fn add_connector(&self, namespace: String, name: String, spec: NetworkConnectorSpec);

    async fn add_entries(
        &self,
        entries: impl Send + IntoIterator<Item = NetworkEntry>,
    ) -> Result<()>;

    async fn delete_connector(&self, namespace: String, name: String);

    async fn get_connectors(
        &self,
        r#type: NetworkConnectorSourceRef,
    ) -> Option<Vec<NetworkConnectorSpec>>;

    async fn get_entries(&self, filter: Option<&NetworkEntryKeyFilter>) -> NetworkEntryMap;

    async fn close(self) -> Result<()>;
}
