use std::collections::BTreeMap;

use anyhow::Result;
use kubegraph_api::connector::{NetworkConnectorSourceRef, NetworkConnectorSpec};
use tokio::join;
use tracing::error;

use crate::db::NetworkGraphDB;

pub async fn loop_forever(graph: NetworkGraphDB) {
    if let Err(error) = try_loop_forever(&graph).await {
        error!("failed to run connect job: {error}")
    }
}

async fn try_loop_forever(graph: &NetworkGraphDB) -> Result<()> {
    use kubegraph_api::connector::NetworkConnector;

    join!(
        #[cfg(feature = "connector-prometheus")]
        ::kubegraph_connector_prometheus::NetworkConnector::default().loop_forever(graph.clone()),
    );
    Ok(())
}

#[derive(Default)]
pub(crate) struct NetworkConnectors {
    db: BTreeMap<(String, String), NetworkConnectorSpec>,
    has_updated: BTreeMap<NetworkConnectorSourceRef, bool>,
}

impl NetworkConnectors {
    pub(crate) fn insert(&mut self, namespace: String, name: String, value: NetworkConnectorSpec) {
        let key = connector_key(namespace, name);
        let src = value.src.to_ref();

        self.db.insert(key, value);
        self.has_updated
            .entry(src)
            .and_modify(|updated| *updated = true);
    }

    pub(crate) fn list(
        &mut self,
        src: NetworkConnectorSourceRef,
    ) -> Option<Vec<NetworkConnectorSpec>> {
        let updated = self.has_updated.entry(src).or_insert(true);
        if *updated {
            *updated = false;
            Some(
                self.db
                    .values()
                    .filter(|&spec| spec.src == src)
                    .cloned()
                    .collect(),
            )
        } else {
            None
        }
    }

    pub(crate) fn remove(&mut self, namespace: String, name: String) {
        let key = connector_key(namespace, name);
        let removed_object = self.db.remove(&key);

        if let Some(object) = removed_object {
            self.has_updated
                .entry(object.src.to_ref())
                .and_modify(|updated| *updated = true);
        }
    }
}

#[inline]
const fn connector_key<T>(namespace: T, name: T) -> (T, T) {
    (namespace, name)
}
