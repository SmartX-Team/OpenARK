use async_trait::async_trait;
use opentelemetry_proto::tonic::collector::logs::v1::{
    logs_service_server::{LogsService, LogsServiceServer},
    ExportLogsPartialSuccess, ExportLogsServiceRequest, ExportLogsServiceResponse,
};
use tonic::{Request, Response, Status};
use tracing::{instrument, Level};

pub fn init(
    #[cfg(feature = "exporter")] exporter: ::std::sync::Arc<
        dyn crate::exporter::Exporter<ExportLogsServiceRequest, ExportLogsServiceResponse>,
    >,
) -> Server {
    Server::new(Service {
        #[cfg(feature = "exporter")]
        exporter,
    })
}

pub type Server = LogsServiceServer<Service>;

pub struct Service {
    #[cfg(feature = "exporter")]
    exporter: ::std::sync::Arc<
        dyn crate::exporter::Exporter<ExportLogsServiceRequest, ExportLogsServiceResponse>,
    >,
}

#[async_trait]
impl LogsService for Service {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn export(
        &self,
        request: Request<ExportLogsServiceRequest>,
    ) -> Result<Response<ExportLogsServiceResponse>, Status> {
        #[cfg(feature = "exporter")]
        super::ExportRequest::export_request(&self.exporter, request);

        Ok(Response::new(ExportLogsServiceResponse {
            partial_success: Some(ExportLogsPartialSuccess {
                rejected_log_records: 0,
                error_message: String::default(),
            }),
        }))
    }
}
