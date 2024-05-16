use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use kubegraph_api::{
    connector::{NetworkConnectorCrd, NetworkConnectorSourceRef},
    graph::{NetworkEntry, NetworkEntryKeyFilter, NetworkEntryMap},
};
use tokio::sync::Mutex;
use tracing::info;

use crate::connector::NetworkConnectors;

#[derive(Clone)]
pub struct NetworkGraphDB {
    connectors: Arc<Mutex<NetworkConnectors>>,
    #[cfg(feature = "db-local")]
    db_local: ::kubegraph_db_local::NetworkGraphDB,
    #[cfg(feature = "db-memory")]
    db_memory: ::kubegraph_db_memory::NetworkGraphDB,
}

impl NetworkGraphDB {
    pub async fn try_default() -> Result<Self> {
        Ok(Self {
            connectors: Arc::default(),
            #[cfg(feature = "db-local")]
            db_local: ::kubegraph_db_local::NetworkGraphDB::try_default().await?,
            #[cfg(feature = "db-memory")]
            db_memory: ::kubegraph_db_memory::NetworkGraphDB::default(),
        })
    }

    fn get_default_db(&self) -> &impl ::kubegraph_api::db::NetworkGraphDB {
        #[cfg(feature = "db-local")]
        {
            &self.db_local
        }
        #[cfg(feature = "db-memory")]
        {
            &self.db_memory
        }
    }
}

#[async_trait]
impl ::kubegraph_api::connector::NetworkConnectors for NetworkGraphDB {
    async fn add_connector(&self, object: NetworkConnectorCrd) {
        self.connectors.lock().await.insert(object)
    }

    async fn delete_connector(&self, namespace: String, name: String) {
        self.connectors.lock().await.remove(namespace, name)
    }

    async fn get_connectors(
        &self,
        r#type: NetworkConnectorSourceRef,
    ) -> Option<Vec<NetworkConnectorCrd>> {
        self.connectors.lock().await.list(r#type)
    }
}

#[async_trait]
impl ::kubegraph_api::db::NetworkGraphDB for NetworkGraphDB {
    async fn add_entries(
        &self,
        entries: impl Send + IntoIterator<Item = NetworkEntry>,
    ) -> Result<()> {
        self.get_default_db().add_entries(entries).await
    }

    async fn get_entries(&self, filter: Option<&NetworkEntryKeyFilter>) -> NetworkEntryMap {
        self.get_default_db().get_entries(filter).await
    }

    async fn close(self) -> Result<()> {
        info!("Closing network graph...");

        #[cfg(feature = "db-local")]
        self.db_local.close().await?;

        #[cfg(feature = "db-memory")]
        self.db_memory.close().await?;

        Ok(())
    }
}
