#[cfg(feature = "exporter")]
mod exporter;

use std::future::Future;

use anyhow::Result;
use opentelemetry::global;
use tracing::{error, info};

macro_rules! init_protocols {
    [ $( $protocol:ident with feature $protocol_feature:expr , )* ] => {
        $(
            #[cfg(feature = $protocol_feature)]
            mod $protocol;
        )*

        #[tokio::main]
        async fn main() {
            ::ark_core::tracer::init_once();

            let signal = ::ark_core::signal::FunctionSignal::default();
            if let Err(error) = signal.trap_on_sigint() {
                error!("{error}");
                return;
            }

            #[cfg(feature = "exporter")]
            let exporters = $crate::exporter::init_exporters().await;

            let handlers = vec![
                $(
                    ::tokio::spawn(init_loop(
                        stringify!($protocol),
                        self::$protocol::init_server(
                            #[cfg(feature = "exporter")]
                            exporters.clone(),
                        ),
                    )),
                )*
            ];

            info!("Ready");
            signal.wait_to_terminate().await;

            info!("Terminating...");
            for handler in handlers {
                handler.abort();
            }
            if let Err(error) = exporters.terminate().await {
                error!("failed to terminate exporters: {error}");
            }

            info!("Terminated.");
            global::shutdown_tracer_provider();
        }
    };
}

init_protocols![
    grpc with feature "grpc",
];

async fn init_loop(protocol: &str, f: impl Future<Output = Result<()>>) {
    match f.await {
        Ok(()) => (),
        Err(error) => {
            error!("failed to init {protocol}: {error}");
        }
    }
}
