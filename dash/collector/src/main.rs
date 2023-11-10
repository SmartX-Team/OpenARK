#[cfg(feature = "exporter")]
mod exporter;

use std::future::Future;

use anyhow::Result;
use tracing::error;

macro_rules! init_protocols {
    [ $( $protocol:ident with feature $protocol_feature:expr , )* ] => {
        $(
            #[cfg(feature = $protocol_feature)]
            mod $protocol;
        )*

        #[tokio::main]
        async fn main() {
            ::ark_core::tracer::init_once();

            #[cfg(feature = "exporter")]
            let exporters = $crate::exporter::init_exporters().await;

            ::tokio::join!(
                $(
                    init_loop(
                        stringify!($protocol),
                        self::$protocol::init_server(
                            #[cfg(feature = "exporter")]
                            exporters,
                        ),
                    ),
                )*
            );
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
