use async_trait::async_trait;
use opentelemetry_proto::tonic::collector::metrics::v1::{
    metrics_service_server::{MetricsService, MetricsServiceServer},
    ExportMetricsPartialSuccess, ExportMetricsServiceRequest, ExportMetricsServiceResponse,
};
use tonic::{Request, Response, Status};
use tracing::{instrument, Level};

pub fn init(
    #[cfg(feature = "exporter")] exporter: ::std::sync::Arc<
        dyn crate::exporter::Exporter<ExportMetricsServiceRequest, ExportMetricsServiceResponse>,
    >,
) -> Server {
    Server::new(Service {
        #[cfg(feature = "exporter")]
        exporter,
    })
}

pub type Server = MetricsServiceServer<Service>;

pub struct Service {
    #[cfg(feature = "exporter")]
    exporter: ::std::sync::Arc<
        dyn crate::exporter::Exporter<ExportMetricsServiceRequest, ExportMetricsServiceResponse>,
    >,
}

#[async_trait]
impl MetricsService for Service {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn export(
        &self,
        request: Request<ExportMetricsServiceRequest>,
    ) -> Result<Response<ExportMetricsServiceResponse>, Status> {
        #[cfg(feature = "exporter")]
        super::ExportRequest::export_request(&self.exporter, request);

        Ok(Response::new(ExportMetricsServiceResponse {
            partial_success: Some(ExportMetricsPartialSuccess {
                rejected_data_points: 0,
                error_message: String::default(),
            }),
        }))
    }
}
