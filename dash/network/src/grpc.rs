use anyhow::Result;
use async_trait::async_trait;
use dash_network_api::NetworkGraph;
use opentelemetry_proto::tonic::collector::trace::v1::{
    trace_service_server::{TraceService, TraceServiceServer},
    ExportTracePartialSuccess, ExportTraceServiceRequest, ExportTraceServiceResponse,
};
use tonic::{codec::CompressionEncoding, Request, Response, Status};
use tracing::{error, instrument, Level};

pub async fn loop_forever(graph: NetworkGraph) {
    if let Err(error) = try_loop_forever(graph).await {
        error!("failed to run gRPC: {error}");
    }
}

async fn try_loop_forever(graph: NetworkGraph) -> Result<()> {
    let addr = ::ark_core::env::infer("DASH_COLLECTOR_GRPC_ADDR")?;

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
    graph: NetworkGraph,
}

#[async_trait]
impl TraceService for Service {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> Result<Response<ExportTraceServiceResponse>, Status> {
        let value = request.into_parts().2;

        // let graph = self.graph.clone();
        // self.graph.insert().await;

        Ok(Response::new(ExportTraceServiceResponse {
            partial_success: Some(ExportTracePartialSuccess {
                rejected_spans: 0,
                error_message: String::default(),
            }),
        }))
    }
}
