use std::{collections::HashMap, str::FromStr, time::Duration};

use anyhow::Result;
use async_trait::async_trait;
use dash_network_api::{
    graph::{ArcNetworkGraph, NetworkNodeKey, NetworkValueBuilder},
    query::{NetworkQuery, NetworkQueryNodeType, NetworkQueryNodeValue},
};
use futures::{stream::FuturesUnordered, TryStreamExt};
use prometheus_http_query::{response::InstantVector, Client};

pub struct Pull {
    pulls: Vec<QueryPull>,
}

impl Default for Pull {
    fn default() -> Self {
        let client =
            Client::from_str("http://kube-prometheus-stack-prometheus.monitoring.svc:9090")
                .unwrap();

        Self {
            pulls: vec![QueryPull {
                client,
                query: NetworkQuery {
                    query: "sum by (data_model, data_model_from, job, le) (dash_metrics_duration_milliseconds_bucket{span_name=\"call_function\"})".into(),
                    sink: NetworkQueryNodeType {
                        kind: NetworkQueryNodeValue::Static(Some("model".into())),
                        name: NetworkQueryNodeValue::Key("data_model_from".into()),
                        namespace: NetworkQueryNodeValue::Key("k8s_namespace_name".into()),
                    },
                    src: NetworkQueryNodeType {
                        kind: NetworkQueryNodeValue::Static(Some("model".into())),
                        name: NetworkQueryNodeValue::Key("data_model".into()),
                        namespace: NetworkQueryNodeValue::Key("k8s_namespace_name".into()),
                    },
                },
            }],
        }
    }
}

#[async_trait]
impl super::Pull for Pull {
    const NAME: &'static str = "prometheus";
    const INTERVAL: Duration = Duration::from_secs(5);

    async fn pull(&self, graph: &ArcNetworkGraph) -> Result<()> {
        self.pulls
            .iter()
            .map(|pull| pull.pull(graph))
            .collect::<FuturesUnordered<_>>()
            .try_collect()
            .await
    }
}

struct QueryPull {
    client: Client,
    query: NetworkQuery,
}

#[async_trait]
impl super::Pull for QueryPull {
    const NAME: &'static str = "prometheus";
    const INTERVAL: Duration = Duration::from_secs(5);

    async fn pull(&self, graph: &ArcNetworkGraph) -> Result<()> {
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

        graph.add_edges(edges).await;
        Ok(())
    }
}

impl Search for NetworkQueryNodeType {
    type Output = NetworkNodeKey;

    fn search(&self, metric: &HashMap<String, String>) -> Option<<Self as Search>::Output> {
        Some(NetworkNodeKey {
            kind: self.kind.search(metric)?,
            name: self.name.search(metric),
            namespace: self.namespace.search(metric)?,
        })
    }
}

impl Search for NetworkQueryNodeValue {
    type Output = String;

    fn search(&self, metric: &HashMap<String, String>) -> Option<<Self as Search>::Output> {
        match self {
            Self::Key(key) => metric.get(key).cloned(),
            Self::Static(value) => value.clone(),
        }
    }
}

trait Search {
    type Output;

    fn search(&self, metric: &HashMap<String, String>) -> Option<<Self as Search>::Output>;
}
