use std::{
    collections::{BTreeMap, HashMap},
    str::FromStr,
    time::Duration,
};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::StreamExt;
use kubegraph_api::{
    connector::{
        NetworkConnectorPrometheusSpec, NetworkConnectorSourceRef, NetworkConnectorSpec,
        NetworkConnectors,
    },
    db::NetworkGraphDB,
    graph::{NetworkEdgeKey, NetworkEntry, NetworkEntryKey, NetworkNodeKey, NetworkValue},
    query::{NetworkQuery, NetworkQueryNodeType, NetworkQueryNodeValue, NetworkQueryType},
};
use prometheus_http_query::{response::InstantVector, Client};
use tracing::{info, instrument, warn, Level};

#[derive(Default)]
pub struct NetworkConnector {
    clients: BTreeMap<NetworkConnectorPrometheusSpec, Option<Client>>,
    db: Vec<NetworkConnectorPrometheusSpec>,
}

#[async_trait]
impl ::kubegraph_api::connector::NetworkConnector for NetworkConnector {
    #[inline]
    fn name(&self) -> &str {
        "prometheus"
    }

    #[inline]
    fn interval(&self) -> Duration {
        Duration::from_secs(5)
    }

    #[instrument(level = Level::INFO, skip_all)]
    async fn pull(&mut self, graph: &(impl NetworkConnectors + NetworkGraphDB)) -> Result<()> {
        // update db
        if let Some(db) = graph
            .get_connectors(NetworkConnectorSourceRef::Prometheus)
            .await
        {
            info!("Reloading prometheus connector...");
            self.db = db
                .into_iter()
                .filter_map(|spec| match spec {
                    NetworkConnectorSpec::Prometheus(spec) => Some(spec),
                    #[allow(unused_variables)]
                    _ => None,
                })
                .collect();
        }
        if self.db.is_empty() {
            return Ok(());
        }

        let dataset = self.db.iter().filter_map(|spec| {
            let client = self
                .clients
                .entry(spec.clone())
                .or_insert_with(|| match load_client(spec) {
                    Ok(client) => Some(client),
                    Err(error) => {
                        warn!("{error}");
                        None
                    }
                })
                .clone()?;

            let NetworkConnectorPrometheusSpec { template, url: _ } = spec;
            Some((client, template))
        });

        ::futures::stream::iter(dataset)
            .for_each(|(client, query)| async move {
                if let Err(error) = pull_with(graph, client, query).await {
                    warn!("failed to pull prometheus query {query:?}: {error}");
                }
            })
            .await;
        Ok(())
    }
}

#[instrument(level = Level::INFO, skip_all)]
fn load_client(spec: &NetworkConnectorPrometheusSpec) -> Result<Client> {
    let NetworkConnectorPrometheusSpec { template: _, url } = spec;

    Client::from_str(url.as_str())
        .map_err(|error| anyhow!("failed to init prometheus client {url:?}: {error}"))
}

#[instrument(level = Level::INFO, skip_all)]
async fn pull_with(
    graph: &impl NetworkGraphDB,
    client: Client,
    template: &NetworkQuery,
) -> Result<()> {
    let NetworkQuery {
        interval_ms,
        query,
        r#type,
    } = template;

    // Evaluate a PromQL query.
    let response = client.query(query).get().await?;
    let (data, _) = response.into_inner();
    let vector = data.into_vector().ok().unwrap();

    let dataset = vector.into_iter().map(InstantVector::into_inner);

    let entries = dataset.clone().filter_map(|(metric, sample)| {
        Some(NetworkEntry {
            key: match r#type {
                NetworkQueryType::Edge { link, sink, src } => {
                    NetworkEntryKey::Edge(NetworkEdgeKey {
                        interval_ms: interval_ms
                            .search(&metric)
                            .and_then(|value| value.parse().ok()),
                        link: link.search(&metric)?,
                        sink: sink.search(&metric)?,
                        src: src.search(&metric)?,
                    })
                }
                NetworkQueryType::Node { node } => NetworkEntryKey::Node(node.search(&metric)?),
            },
            value: NetworkValue::Number(sample.value()),
        })
    });

    graph.add_entries(entries).await
}

trait Search {
    type Output;

    fn search(&self, metric: &Metric) -> Option<<Self as Search>::Output>;
}

impl Search for NetworkQueryNodeType {
    type Output = NetworkNodeKey;

    #[inline]
    fn search(&self, metric: &Metric) -> Option<<Self as Search>::Output> {
        Some(NetworkNodeKey {
            kind: self.kind.search(metric)?,
            name: self.name.search(metric)?,
            namespace: self.namespace.search(metric)?,
        })
    }
}

impl Search for NetworkQueryNodeValue {
    type Output = String;

    #[inline]
    fn search(&self, metric: &Metric) -> Option<<Self as Search>::Output> {
        match self {
            Self::Key(key) => metric.get(key).cloned(),
            Self::Static(value) => value.clone(),
        }
    }
}

type Metric = HashMap<String, String>;
