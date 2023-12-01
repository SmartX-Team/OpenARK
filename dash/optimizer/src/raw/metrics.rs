use anyhow::Result;
use async_trait::async_trait;
use dash_optimizer_api::raw;
use dash_pipe_provider::{PipeArgs, PipeMessage, RemoteFunction};
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use tracing::{info, instrument, Level};

use crate::ctx::OptimizerContext;

#[derive(Clone)]
pub struct Reader {
    ctx: OptimizerContext,
}

#[async_trait]
impl crate::ctx::OptimizerService for Reader {
    fn new(ctx: &OptimizerContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    async fn loop_forever(self) -> Result<()> {
        info!("creating messenger: raw metrics reader");

        let pipe = PipeArgs::with_function(self)?
            .with_ignore_sigint(true)
            .with_model_in(Some(raw::metrics::model()?))
            .with_model_out(None);
        pipe.loop_forever_async().await
    }
}

#[async_trait]
impl RemoteFunction for Reader {
    type Input = ExportMetricsServiceRequest;
    type Output = ();

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn call_one(
        &self,
        input: PipeMessage<<Self as RemoteFunction>::Input, ()>,
    ) -> Result<PipeMessage<<Self as RemoteFunction>::Output, ()>> {
        let make_response = || Ok(PipeMessage::with_request(&input, vec![], ()));

        // skip if no metrics are given
        if input
            .value
            .resource_metrics
            .iter()
            .all(|metric| metric.scope_metrics.is_empty())
        {
            return make_response();
        }

        // TODO: to be implemented
        make_response()
    }
}
