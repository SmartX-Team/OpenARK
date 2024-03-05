mod prometheus;

use std::time::{Duration, Instant};

use anyhow::Result;
use async_trait::async_trait;
use kubegraph_api::{
    connector::NetworkConnectorPrometheusSpec,
    query::{NetworkQuery, NetworkQueryNodeType, NetworkQueryNodeValue},
};
use kubegraph_client::NetworkGraphClient;
use tokio::time::sleep;
use tracing::error;

pub async fn loop_forever(graph: NetworkGraphClient) {
    if let Err(error) = try_loop_forever(graph).await {
        error!("failed to run connect job: {error}")
    }
}

async fn try_loop_forever(graph: NetworkGraphClient) -> Result<()> {
    let connector = NetworkConnectorPrometheusSpec {
        url: "http://kube-prometheus-stack-prometheus.monitoring.svc:9090".parse()?,
    };
    let query =  NetworkQuery {
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
    };

    self::prometheus::Connector::try_new(query, connector)?
        .loop_forever(graph)
        .await;
    Ok(())
}

#[async_trait]
trait Connector {
    fn name(&self) -> &str;

    fn interval(&self) -> Duration {
        Duration::from_secs(15)
    }

    async fn loop_forever(self, graph: NetworkGraphClient)
    where
        Self: Sized,
    {
        let name = <Self as Connector>::name(&self);
        let interval = <Self as Connector>::interval(&self);

        loop {
            let instant = Instant::now();
            if let Err(error) = self.pull(&graph).await {
                error!("failed to connect to dataset from {name:?}: {error}");
            }

            let elapsed = instant.elapsed();
            if elapsed < interval {
                sleep(interval - elapsed).await;
            }
        }
    }

    async fn pull(&self, graph: &NetworkGraphClient) -> Result<()>;
}
