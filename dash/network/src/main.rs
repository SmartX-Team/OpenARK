mod actix;
mod grpc;
mod routes;

use dash_network_api::graph::ArcNetworkGraph;
use opentelemetry::global;
use tokio::spawn;
use tracing::{error, info};

// #[tokio::main]
// async fn main() {
//     ::ark_core::tracer::init_once();

//     let signal = ::dash_pipe_provider::FunctionSignal::default();
//     if let Err(error) = signal.trap_on_sigint() {
//         error!("{error}");
//         return;
//     }

//     let graph = ArcNetworkGraph::default();

//     let handlers = vec![
//         spawn(crate::actix::loop_forever(graph.clone())),
//         spawn(crate::grpc::loop_forever(graph)),
//     ];
//     signal.wait_to_terminate().await;

//     info!("Terminating...");
//     for handler in handlers {
//         handler.abort();
//     }

//     info!("Terminated.");
//     global::shutdown_tracer_provider();
// }

use std::str::FromStr;

use prometheus_http_query::{Client, Error};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Error> {
    let client = Client::from_str("http://kube-prometheus-stack-prometheus.monitoring.svc:9090")?;

    // Evaluate a PromQL query.
    let q = "sum by (le) (dash_metrics_duration_milliseconds_bucket{job=\"question-answering\", span_name=\"call_function\"})";
    let response = client.query(q).get().await?;
    let (data, _) = response.into_inner();
    let vector = data.into_vector().ok().unwrap();
    dbg!(vector);

    Ok(())
}
