use anyhow::Result;
use async_trait::async_trait;

use crate::graph::{NetworkEntry, NetworkEntryKeyFilter, NetworkEntryMap};

#[async_trait]
pub trait NetworkGraphDB
where
    Self: Send + Sync,
{
    async fn add_entries(
        &self,
        entries: impl Send + IntoIterator<Item = NetworkEntry>,
    ) -> Result<()>;

    async fn get_entries(&self, filter: Option<&NetworkEntryKeyFilter>) -> NetworkEntryMap;

    async fn close(self) -> Result<()>;
}
