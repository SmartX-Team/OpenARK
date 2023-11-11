use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use dash_pipe_provider::PipeMessage;
use tracing::{instrument, Level};

macro_rules! init_exporters {
    [ $( $signal:ident with feature $signal_feature:expr , )* ] => {
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
                fn $signal(&self) -> Arc<dyn Exporter>;
            )*
        }

        mod topics {
            $(
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

init_exporters! [
    logs with feature "logs",
    metrics with feature "metrics",
    trace with feature "trace",
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
    messenger with feature "exporter-messenger",
];

#[async_trait]
pub trait Exporter
where
    Self: Send + Sync,
{
    async fn export(&self, message: &PipeMessage) -> Result<()>;
}

#[async_trait]
impl<T> Exporter for &T
where
    T: ?Sized + Exporter,
{
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn export(&self, message: &PipeMessage) -> Result<()> {
        <T as Exporter>::export(*self, message).await
    }
}

#[async_trait]
impl<T> Exporter for Box<T>
where
    T: ?Sized + Exporter,
{
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn export(&self, message: &PipeMessage) -> Result<()> {
        <T as Exporter>::export(self, message).await
    }
}

#[async_trait]
impl<T> Exporter for Arc<T>
where
    T: ?Sized + Exporter,
{
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn export(&self, message: &PipeMessage) -> Result<()> {
        <T as Exporter>::export(self, message).await
    }
}
