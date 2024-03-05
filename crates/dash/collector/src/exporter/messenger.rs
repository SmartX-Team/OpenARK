use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use dash_pipe_provider::{messengers::Publisher, PipeMessage};
use tracing::{instrument, Level};

macro_rules! init_exporter {
    [ $( $signal:ident with feature $signal_feature:expr , )* ] => {
        pub struct Exporters {
            $(
                pub $signal: Arc<dyn super::Exporter>,
            )*
        }

        #[async_trait]
        impl super::Exporters for Exporters {
            #[instrument(level = Level::INFO, skip_all, err(Display))]
            async fn try_default() -> Result<Self> {
                use clap::Parser;
                use dash_pipe_provider::messengers::{init_messenger, MessengerArgs};
                use serde_json::Value;

                let args = StorageS3Args::parse();

                let messenger = init_messenger::<Value>(&args).await?;

                Ok(Self {
                    $(
                        $signal: Arc::new(messenger.publish(super::topics::$signal()?).await?),
                    )*
                })
            }

            $(
                fn $signal(&self) -> Arc<dyn super::Exporter> {
                    self.$signal.clone()
                }
            )*
        }
    };
}

init_exporter! [
    logs with feature "logs",
    metrics with feature "metrics",
    trace with feature "trace",
];

#[async_trait]
impl super::Exporter for Arc<dyn Publisher> {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn export(&self, message: &PipeMessage) -> Result<()> {
        self.send_one(message.try_into()?).await
    }
}
