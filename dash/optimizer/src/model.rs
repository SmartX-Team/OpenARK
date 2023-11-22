use anyhow::Result;
use async_trait::async_trait;
use dash_api::model_storage_binding::{
    ModelStorageBindingStorageKind, ModelStorageBindingStorageKindOwnedSpec,
};
use dash_optimizer_api::optimize;
use dash_pipe_provider::{PipeArgs, PipeMessage, PipeMessages, RemoteFunction};
use futures::{stream::FuturesOrdered, TryStreamExt};
use kube::ResourceExt;
use tracing::{info, instrument, Level};

use crate::ctx::OptimizerContext;

#[derive(Clone)]
pub struct Optimizer {
    ctx: OptimizerContext,
}

#[async_trait]
impl crate::ctx::Optimizer for Optimizer {
    fn new(ctx: &OptimizerContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    async fn loop_forever(self) -> Result<()> {
        info!("creating messenger: model optimizer");

        let pipe = PipeArgs::with_function(self)?
            .with_ignore_sigint(true)
            .with_model_in(optimize::model::model_in()?)
            .with_model_out(optimize::model::model_out()?);
        pipe.loop_forever_async().await
    }
}

#[async_trait]
impl RemoteFunction for Optimizer {
    type Input = optimize::model::Request;
    type Output = optimize::model::Response;

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn call(
        &self,
        inputs: PipeMessages<<Self as RemoteFunction>::Input, ()>,
    ) -> Result<PipeMessages<<Self as RemoteFunction>::Output, ()>> {
        inputs
            .into_vec()
            .into_iter()
            .map(|input| {
                let function = self.clone();
                async move { function.call_one(input).await }
            })
            .collect::<FuturesOrdered<_>>()
            .try_collect()
            .await
            .map(|outputs| PipeMessages::Batch(outputs))
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn call_one(
        &self,
        input: PipeMessage<<Self as RemoteFunction>::Input, ()>,
    ) -> Result<PipeMessage<<Self as RemoteFunction>::Output, ()>> {
        let optimize::model::Request {
            model,
            policy,
            storage,
        } = &input.value;

        let model = match model {
            Some(model) => model,
            None => return Ok(PipeMessage::with_request(&input, vec![], None)),
        };
        let name = model.name_any();
        let namespace = match model.namespace() {
            Some(namespace) => namespace,
            None => return Ok(PipeMessage::with_request(&input, vec![], None)),
        };
        let storage = match storage {
            Some(storage) => storage,
            None => return Ok(PipeMessage::with_request(&input, vec![], None)),
        };

        match self
            .ctx
            .solve_next_model_storage_binding(&namespace, &name, *policy)
            .await
        {
            Some(target) => {
                let value = ModelStorageBindingStorageKind::Owned(
                    ModelStorageBindingStorageKindOwnedSpec { target },
                );
                Ok(PipeMessage::with_request(&input, vec![], Some(value)))
            }
            None => Ok(PipeMessage::with_request(&input, vec![], None)),
        }
    }
}
