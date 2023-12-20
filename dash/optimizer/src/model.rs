use anyhow::Result;
use async_trait::async_trait;
use dash_api::{
    model_storage_binding::{
        ModelStorageBindingStorageKind, ModelStorageBindingStorageKindOwnedSpec,
    },
    storage::ModelStorageKind,
};
use dash_collector_world::ctx::{Timeout, WorldContext};
use dash_optimizer_api::model;
use dash_pipe_provider::{PipeMessage, RemoteFunction};
use futures::FutureExt;
use kube::ResourceExt;
use tracing::{info, instrument, Level};

use crate::pipe::init_pipe;

#[derive(Clone)]
pub struct Service {
    ctx: WorldContext,
}

impl Service {
    pub fn new(ctx: WorldContext) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl ::dash_collector_world::service::Service for Service {
    async fn loop_forever(self) -> Result<()> {
        info!("creating service: model optimizer");

        let pipe = init_pipe(self, model::model_in()?, model::model_out()?)?;
        pipe.loop_forever_async().await
    }
}

#[async_trait]
impl RemoteFunction for Service {
    type Input = model::Request;
    type Output = model::Response;

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn call_one(
        &self,
        input: PipeMessage<<Self as RemoteFunction>::Input, ()>,
    ) -> Result<PipeMessage<<Self as RemoteFunction>::Output, ()>> {
        let model::Request {
            model,
            policy,
            storage,
        } = &input.value;

        let fail = || Ok(PipeMessage::with_request(&input, vec![], None));

        let model = match model {
            Some(model) => model,
            None => return fail(),
        };
        let name = model.name_any();
        let namespace = match model.namespace() {
            Some(namespace) => namespace,
            None => return fail(),
        };
        let storage = match storage {
            Some(storage) => storage,
            None => return fail(),
        };

        match storage {
            ModelStorageKind::ObjectStorage => match self
                .ctx
                .get(&namespace, &name, Timeout::Unlimited)
                .then(|option| async {
                    match option {
                        Some(namespace) => namespace
                            .read()
                            .await
                            .solve_next_model_storage_binding(&name, *policy),
                        None => None,
                    }
                })
                .await
            {
                Some(target) => {
                    let value = ModelStorageBindingStorageKind::Owned(
                        ModelStorageBindingStorageKindOwnedSpec { target },
                    );
                    Ok(PipeMessage::with_request(&input, vec![], Some(value)))
                }
                None => Ok(PipeMessage::with_request(&input, vec![], None)),
            },
            // does not supported yet
            ModelStorageKind::Database | ModelStorageKind::Kubernetes => fail(),
        }
    }
}
