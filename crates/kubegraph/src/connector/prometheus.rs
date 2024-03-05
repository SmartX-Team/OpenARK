use std::{collections::HashMap, str::FromStr, time::Duration};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use kubegraph_api::{
    connector::NetworkConnectorPrometheusSpec,
    graph::{NetworkNodeKey, NetworkValueBuilder},
    query::{NetworkQuery, NetworkQueryNodeType, NetworkQueryNodeValue},
};
use kubegraph_client::NetworkGraphClient;
use prometheus_http_query::{response::InstantVector, Client};
use tracing::{instrument, Level};

pub struct Connector {
    client: Client,
    query: NetworkQuery,
}

impl Connector {
    #[instrument(level = Level::INFO, skip_all)]
    pub fn try_new(query: NetworkQuery, spec: NetworkConnectorPrometheusSpec) -> Result<Connector> {
        let NetworkConnectorPrometheusSpec { url } = spec;

        Ok(Self {
            client: Client::from_str(url.as_str())
                .map_err(|error| anyhow!("failed to init prometheus client: {error}"))?,
            query,
        })
    }
}

#[async_trait]
impl super::Connector for Connector {
    fn name(&self) -> &str {
        "prometheus"
    }

    fn interval(&self) -> Duration {
        Duration::from_secs(5)
    }

    #[instrument(level = Level::INFO, skip_all)]
    async fn pull(&self, graph: &NetworkGraphClient) -> Result<()> {
        let NetworkQuery { query, sink, src } = &self.query;

        // Evaluate a PromQL query.
        let response = self.client.query(query).get().await?;
        let (data, _) = response.into_inner();
        let vector = data.into_vector().ok().unwrap();

        let edges = vector
            .into_iter()
            .map(InstantVector::into_inner)
            .filter_map(|(metric, sample)| {
                let src = src.search(&metric)?;
                let sink = sink.search(&metric)?;
                let key = (src, sink);

                let count = sample.value();
                if count < usize::MIN as f64 || count > usize::MAX as f64 {
                    return None;
                }
                let count = count as usize;

                let duration = Duration::from_millis(metric.get("le")?.parse().ok()?);
                let value = NetworkValueBuilder::new(duration).count(count);
                Some((key, value))
            });

        graph.add_edges(edges).await
    }
}

impl Search for NetworkQueryNodeType {
    type Output = NetworkNodeKey;

    fn search(&self, metric: &Metric) -> Option<<Self as Search>::Output> {
        Some(NetworkNodeKey {
            kind: self.kind.search(metric)?,
            name: self.name.search(metric),
            namespace: self.namespace.search(metric)?,
        })
    }
}

impl Search for NetworkQueryNodeValue {
    type Output = String;

    fn search(&self, metric: &Metric) -> Option<<Self as Search>::Output> {
        match self {
            Self::Key(key) => metric.get(key).cloned(),
            Self::Static(value) => value.clone(),
        }
    }
}

trait Search {
    type Output;

    fn search(&self, metric: &Metric) -> Option<<Self as Search>::Output>;
}

type Metric = HashMap<String, String>;
