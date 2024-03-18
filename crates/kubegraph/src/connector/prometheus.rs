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
        NetworkConnectorPrometheusSpec, NetworkConnectorSource, NetworkConnectorSourceRef,
        NetworkConnectorSpec,
    },
    graph::{NetworkEdgeKey, NetworkEntry, NetworkEntrykey, NetworkNodeKey, NetworkValue},
    provider::NetworkGraphProvider,
    query::{NetworkQuery, NetworkQueryNodeType, NetworkQueryNodeValue, NetworkQueryType},
};
use prometheus_http_query::{response::InstantVector, Client};
use tracing::{info, instrument, warn, Level};

#[derive(Default)]
pub struct Connector {
    clients: BTreeMap<NetworkConnectorPrometheusSpec, Option<Client>>,
    db: Vec<NetworkConnectorSpec<NetworkConnectorPrometheusSpec>>,
}

#[async_trait]
impl super::Connector for Connector {
    #[inline]
    fn name(&self) -> &str {
        "prometheus"
    }

    #[inline]
    fn interval(&self) -> Duration {
        Duration::from_secs(5)
    }

    #[instrument(level = Level::INFO, skip_all)]
    async fn pull(&mut self, graph: &impl NetworkGraphProvider) -> Result<()> {
        // update db
        if let Some(db) = graph
            .get_connectors(NetworkConnectorSourceRef::Prometheus)
            .await
        {
            info!("Reloading prometheus connector...");
            self.db = db
                .into_iter()
                .filter_map(|spec| {
                    let NetworkConnectorSpec { src, template } = spec;
                    match src {
                        NetworkConnectorSource::Prometheus(src) => {
                            Some(NetworkConnectorSpec { src, template })
                        }
                    }
                })
                .collect();
        }
        if self.db.is_empty() {
            return Ok(());
        }

        let dataset = self.db.iter().filter_map(|spec| {
            let NetworkConnectorSpec { src, template } = spec;

            let client = self
                .clients
                .entry(src.clone())
                .or_insert_with(|| match load_client(src) {
                    Ok(client) => Some(client),
                    Err(error) => {
                        warn!("{error}");
                        None
                    }
                })
                .clone()?;

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
fn load_client(src: &NetworkConnectorPrometheusSpec) -> Result<Client> {
    let NetworkConnectorPrometheusSpec { url } = src;

    Client::from_str(url.as_str())
        .map_err(|error| anyhow!("failed to init prometheus client {url:?}: {error}"))
}

#[instrument(level = Level::INFO, skip_all)]
async fn pull_with(
    graph: &impl NetworkGraphProvider,
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
        let key = match r#type {
            NetworkQueryType::Edge { link, sink, src } => NetworkEntrykey::Edge(NetworkEdgeKey {
                interval_ms: interval_ms
                    .search(&metric)
                    .and_then(|value| value.parse().ok()),
                link: link.search(&metric)?,
                sink: sink.search(&metric)?,
                src: src.search(&metric)?,
            }),
            NetworkQueryType::Node { node } => NetworkEntrykey::Node(node.search(&metric)?),
        };

        let value = NetworkValue({
            let count = sample.value();
            if count < u64::MIN as f64 || count > u64::MAX as f64 {
                return None;
            }
            count as u64
        });

        Some(NetworkEntry { key, value })
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
