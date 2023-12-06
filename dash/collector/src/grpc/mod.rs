macro_rules! init_signals {
    [ $( $signal:ident with feature $signal_feature:expr , )* ] => {
        $(
            #[cfg(feature = $signal_feature)]
            mod $signal;
        )*

        #[::tracing::instrument(level = tracing::Level::INFO, skip_all, err(Display))]
        pub async fn init_server(
            #[cfg(feature = "exporter")]
            exporters: Box<dyn $crate::exporter::Exporters>,
        ) -> ::anyhow::Result<()> {
            let addr = ::ark_core::env::infer("DASH_COLLECTOR_GRPC_ADDR")?;

            let mut server = ::tonic::transport::Server::builder();
            $(
                #[cfg(feature = $signal_feature)]
                let server = server.add_service(self::$signal::init(
                    #[cfg(feature = "exporter")]
                    exporters.$signal(),
                ));
            )*

            ::tracing::info!("Running GRPC server...");
            server.serve(addr).await.map_err(Into::into)
        }
    };
}

init_signals! [
    logs with feature "logs",
    metrics with feature "metrics",
    trace with feature "trace",
];

#[cfg(feature = "exporter")]
trait ExportRequest
where
    Self: 'static + Clone + crate::exporter::Exporter,
{
    fn export_request<T>(&self, request: ::tonic::Request<T>)
    where
        T: Send + ::serde::Serialize,
    {
        match ::serde_json::to_value(request.into_parts().2) {
            Ok(value) => {
                let message = ::dash_pipe_provider::PipeMessage::new(value);

                let exporter = self.clone();
                ::tokio::task::spawn(async move { exporter.export(&message).await });
            }
            Err(error) => {
                ::tracing::warn!("failed to parse request as JSON: {error}")
            }
        }
    }
}

#[cfg(feature = "exporter")]
impl ExportRequest for ::std::sync::Arc<dyn crate::exporter::Exporter> {}
