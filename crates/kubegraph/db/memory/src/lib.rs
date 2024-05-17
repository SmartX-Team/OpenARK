use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use itertools::Itertools;
use kubegraph_api::graph::{
    NetworkEntry, NetworkEntryKey, NetworkEntryKeyFilter, NetworkEntryMap, NetworkValue,
};
use tokio::sync::RwLock;
use tracing::{info, instrument, Level};

#[derive(Clone, Default)]
pub struct NetworkGraphDB {
    map: Arc<RwLock<BTreeMap<NetworkEntryKey, NetworkValue>>>,
}

#[async_trait]
impl ::kubegraph_api::db::NetworkGraphDB for NetworkGraphDB {
    #[instrument(level = Level::INFO, skip(self, entries))]
    async fn add_entries(
        &self,
        entries: impl Send + IntoIterator<Item = NetworkEntry>,
    ) -> Result<()> {
        let mut map = self.map.write().await;
        entries.into_iter().for_each(|NetworkEntry { key, value }| {
            map.insert(key, value);
        });
        Ok(())
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn get_entries(&self, filter: Option<&NetworkEntryKeyFilter>) -> NetworkEntryMap {
        self.map
            .read()
            .await
            .iter()
            .filter(|(key, _)| filter.map(|filter| filter.contains(key)).unwrap_or(true))
            .map(|(key, value)| NetworkEntry {
                key: key.clone(),
                value: value.clone(),
            })
            .fold(NetworkEntryMap::default(), |mut map, entry| {
                map.push(entry);
                map
            })
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn get_namespaces(&self) -> Vec<String> {
        self.map
            .read()
            .await
            .keys()
            .map(|key| key.namespace().into())
            .unique()
            .collect()
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn close(self) -> Result<()> {
        info!("Closing in-memory db...");
        Ok(())
    }
}
