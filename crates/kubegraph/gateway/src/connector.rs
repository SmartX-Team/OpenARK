use std::collections::BTreeMap;

use futures::{stream::FuturesUnordered, StreamExt};
use kube::ResourceExt;
use kubegraph_api::connector::{NetworkConnector, NetworkConnectorCrd, NetworkConnectorSourceRef};

use crate::db::NetworkGraphDB;

pub async fn loop_forever(graph: NetworkGraphDB) {
    FuturesUnordered::from_iter(vec![
        #[cfg(feature = "connector-prometheus")]
        ::kubegraph_connector_prometheus::NetworkConnector::default().loop_forever(graph.clone()),
        #[cfg(feature = "connector-simulation")]
        ::kubegraph_connector_simulation::NetworkConnector::default().loop_forever(graph.clone()),
    ])
    .collect()
    .await
}

#[derive(Default)]
pub(crate) struct NetworkConnectors {
    db: BTreeMap<(String, String), NetworkConnectorCrd>,
    has_updated: BTreeMap<NetworkConnectorSourceRef, bool>,
}

impl NetworkConnectors {
    pub(crate) fn insert(&mut self, object: NetworkConnectorCrd) {
        let namespace = object.namespace().unwrap_or_else(|| "default".into());
        let name = object.name_any();
        let key = connector_key(namespace, name);
        let src = object.spec.to_ref();

        self.db.insert(key, object);
        self.has_updated
            .entry(src)
            .and_modify(|updated| *updated = true);
    }

    pub(crate) fn list(
        &mut self,
        src: NetworkConnectorSourceRef,
    ) -> Option<Vec<NetworkConnectorCrd>> {
        let updated = self.has_updated.entry(src).or_insert(true);
        if *updated {
            *updated = false;
            Some(
                self.db
                    .values()
                    .filter(|&cr| cr.spec == src)
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
                .entry(object.spec.to_ref())
                .and_modify(|updated| *updated = true);
        }
    }
}

#[inline]
const fn connector_key<T>(namespace: T, name: T) -> (T, T) {
    (namespace, name)
}
