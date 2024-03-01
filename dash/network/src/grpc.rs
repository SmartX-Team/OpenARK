use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use dash_network_api::{ArcNetworkGraph, NetworkNodeKey, NetworkValueBuilder};
use opentelemetry_proto::tonic::{
    collector::trace::v1::{
        trace_service_server::{TraceService, TraceServiceServer},
        ExportTracePartialSuccess, ExportTraceServiceRequest, ExportTraceServiceResponse,
    },
    common::v1::{any_value, KeyValue},
};
use tonic::{codec::CompressionEncoding, Request, Response, Status};
use tracing::{error, instrument, Level};

pub async fn loop_forever(graph: ArcNetworkGraph) {
    loop {
        if let Err(error) = try_loop_forever(graph.clone()).await {
            error!("failed to run gRPC: {error}");
        }
    }
}

async fn try_loop_forever(graph: ArcNetworkGraph) -> Result<()> {
    let addr = ::ark_core::env::infer("DASH_NETWORK_GRPC_ADDR")?;

    let mut server = ::tonic::transport::Server::builder();
    server
        .add_service(
            Server::new(Service { graph })
                .accept_compressed(CompressionEncoding::Gzip)
                .send_compressed(CompressionEncoding::Gzip),
        )
        .serve(addr)
        .await
        .map_err(Into::into)
}

type Server = TraceServiceServer<Service>;

pub struct Service {
    graph: ArcNetworkGraph,
}

#[async_trait]
impl TraceService for Service {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> Result<Response<ExportTraceServiceResponse>, Status> {
        let request = request.into_parts().2;

        let metrics: Vec<_> = request
            .resource_spans
            .iter()
            .filter_map(|spans| Some((&spans.resource.as_ref()?.attributes, &spans.scope_spans)))
            .flat_map(|(spans_attributes, scope_spans)| {
                scope_spans
                    .iter()
                    .flat_map(|spans| &spans.spans)
                    .filter_map(move |span| {
                        if span.name != "tick_function" {
                            return None;
                        }

                        let attributes = &span.attributes;

                        let kind = "function";
                        let namespace =
                            get_attribute_value_str(spans_attributes, "k8s.namespace.name")?;

                        let node_from = NetworkNodeKey {
                            kind: kind.into(),
                            name: get_attribute_value_str(attributes, "data.model_from")
                                .map(Into::into),
                            namespace: namespace.into(),
                        };
                        let node_to = NetworkNodeKey {
                            kind: kind.into(),
                            name: Some(get_attribute_value_str(attributes, "data.model")?.into()),
                            namespace: namespace.into(),
                        };
                        let key = (node_from, node_to);

                        let value = NetworkValueBuilder::new(Duration::from_nanos(
                            span.end_time_unix_nano - span.start_time_unix_nano,
                        ));

                        Some((key, value))
                    })
            })
            .collect();

        if !metrics.is_empty() {
            self.graph.add_edges(metrics).await;
        }

        Ok(Response::new(ExportTraceServiceResponse {
            partial_success: Some(ExportTracePartialSuccess {
                rejected_spans: 0,
                error_message: String::default(),
            }),
        }))
    }
}

fn get_attribute_value_str<'a>(attributes: &'a [KeyValue], key: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find(|attr| attr.key == key)
        .and_then(|attr| attr.value.as_ref())
        .and_then(|value| value.value.as_ref())
        .and_then(|value| match value {
            any_value::Value::StringValue(value) => Some(value.as_str()),
            _ => None,
        })
}
