mod pipe_function;

use anyhow::Result;
use async_trait::async_trait;
use dash_network_api::ArcNetworkGraph;
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
            .filter_map(|spans| {
                Some((
                    spans.resource.as_ref()?.attributes.as_slice(),
                    &spans.scope_spans,
                ))
            })
            .flat_map(|(resource_attributes, scope_spans)| {
                scope_spans
                    .iter()
                    .flat_map(|spans| &spans.spans)
                    .filter_map(move |span| {
                        let attributes = span.attributes.as_slice();
                        let parse = |f: fn(_, _, _) -> _| f(resource_attributes, span, attributes);

                        match span.name.as_str() {
                            "call_function" => parse(self::pipe_function::parse),
                            _ => None,
                        }
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
        .filter(|&value| !value.is_empty())
}
