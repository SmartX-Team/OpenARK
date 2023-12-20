use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use dash_pipe_provider::PipeMessage;
use opentelemetry_proto::tonic::collector;
use tracing::{instrument, Level};

macro_rules! init_exporters {
    [ $(
            $signal:ident {
            feature : $signal_feature:expr ,
            request : $signal_request:ty ,
            response : $signal_response:ty ,
        } ,
    )* ] => {
        #[async_trait]
        pub trait Exporters
        where
            Self: Send + Sync,
        {
            async fn try_default() -> Result<Self>
            where
                Self: Sized;

            $(
                #[cfg(feature = $signal_feature)]
                fn $signal(&self) -> Arc<dyn Exporter<$signal_request, $signal_response>>;
            )*
        }

        #[cfg(feature = "exporter-messenger")]
        mod topics {
            $(
                #[cfg(feature = $signal_feature)]
                pub fn $signal() -> super::Result<::ark_core_k8s::data::Name> {
                    format!(
                        "dash.raw.{signal}",
                        signal = stringify!($signal),
                    ).parse()
                }
            )*
        }
    };
}

init_exporters![
    logs {
        feature: "logs",
        request: collector::logs::v1::ExportLogsServiceRequest,
        response: collector::logs::v1::ExportLogsServiceResponse,
    },
    metrics {
        feature: "metrics",
        request: collector::metrics::v1::ExportMetricsServiceRequest,
        response: collector::metrics::v1::ExportMetricsServiceResponse,
    },
    trace {
        feature: "trace",
        request: collector::trace::v1::ExportTraceServiceRequest,
        response: ::dash_collector_api::metrics::MetricSpan<'static>,
    },
];

macro_rules! init_exporter_impls {
    [ $( $exporter:ident with feature $exporter_feature:expr , )* ] => {
        $(
            #[cfg(feature = $exporter_feature)]
            mod $exporter;
        )*

        #[instrument(level = Level::INFO, skip_all)]
        pub async fn init_exporters() -> Box<dyn Exporters> {
            $(
                match self::$exporter::Exporters::try_default().await {
                    Ok(exporter) => Box::new(exporter),
                    Err(e) => {
                        ::tracing::error!(
                            "failed to init exporter ({exporter}): {e}",
                            exporter = stringify!($exporter),
                        );
                        ::std::process::exit(1)
                    }
                }
            )*
        }
    };
}

init_exporter_impls! [
    // messenger with feature "exporter-messenger",
    storage with feature "exporter-storage",
];

#[async_trait]
pub trait Exporter<Req, Res>
where
    Self: Send + Sync,
{
    async fn export(&self, message: &PipeMessage<Req, ()>) -> Result<()>;
}

#[async_trait]
impl<Req, Res, T> Exporter<Req, Res> for &T
where
    Req: Sync,
    Res: Sync,
    T: ?Sized + Exporter<Req, Res>,
{
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn export(&self, message: &PipeMessage<Req, ()>) -> Result<()> {
        <T as Exporter<Req, Res>>::export(*self, message).await
    }
}

#[async_trait]
impl<Req, Res, T> Exporter<Req, Res> for Box<T>
where
    Req: Sync,
    Res: Sync,
    T: ?Sized + Exporter<Req, Res>,
{
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn export(&self, message: &PipeMessage<Req, ()>) -> Result<()> {
        <T as Exporter<Req, Res>>::export(self, message).await
    }
}

#[async_trait]
impl<Req, Res, T> Exporter<Req, Res> for Arc<T>
where
    Req: Sync,
    Res: Sync,
    T: ?Sized + Exporter<Req, Res>,
{
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn export(&self, message: &PipeMessage<Req, ()>) -> Result<()> {
        <T as Exporter<Req, Res>>::export(self, message).await
    }
}
