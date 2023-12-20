use async_trait::async_trait;
use dash_collector_api::metrics::MetricSpan;
use opentelemetry_proto::tonic::collector::trace::v1::{
    trace_service_server::{TraceService, TraceServiceServer},
    ExportTracePartialSuccess, ExportTraceServiceRequest, ExportTraceServiceResponse,
};
use tonic::{Request, Response, Status};
use tracing::{instrument, Level};

pub fn init(
    #[cfg(feature = "exporter")] exporter: ::std::sync::Arc<
        dyn crate::exporter::Exporter<ExportTraceServiceRequest, MetricSpan<'static>>,
    >,
) -> Server {
    Server::new(Service {
        #[cfg(feature = "exporter")]
        exporter,
    })
}

pub type Server = TraceServiceServer<Service>;

pub struct Service {
    #[cfg(feature = "exporter")]
    exporter: ::std::sync::Arc<
        dyn crate::exporter::Exporter<ExportTraceServiceRequest, MetricSpan<'static>>,
    >,
}

#[async_trait]
impl TraceService for Service {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> Result<Response<ExportTraceServiceResponse>, Status> {
        #[cfg(feature = "exporter")]
        super::ExportRequest::export_request(&self.exporter, request);

        Ok(Response::new(ExportTraceServiceResponse {
            partial_success: Some(ExportTracePartialSuccess {
                rejected_spans: 0,
                error_message: String::default(),
            }),
        }))
    }
}
