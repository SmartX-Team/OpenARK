use anyhow::Result;
use async_trait::async_trait;
use dash_collector_api::metadata::ObjectMetadata;
use dash_collector_world::ctx::{Timeout, WorldContext};
use dash_optimizer_api::storage;
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
        info!("creating service: storage optimizer");

        let pipe = init_pipe(self, storage::model_in()?, storage::model_out()?)?;
        pipe.loop_forever_async().await
    }
}

#[async_trait]
impl RemoteFunction for Service {
    type Input = storage::Request<'static>;
    type Output = storage::Response;

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn call_one(
        &self,
        input: PipeMessage<<Self as RemoteFunction>::Input, ()>,
    ) -> Result<PipeMessage<<Self as RemoteFunction>::Output, ()>> {
        let storage::Request {
            policy,
            storage: ObjectMetadata { name, namespace },
        } = &input.value;

        match self
            .ctx
            .get(namespace, name, Timeout::Unlimited)
            .then(|option| async {
                match option {
                    Some(namespace) => namespace.read().await.solve_next_storage(name, *policy),
                    None => None,
                }
            })
            .await
        {
            Some(target) => {
                let value = target.name_any().clone();
                Ok(PipeMessage::with_request(&input, vec![], Some(value)))
            }
            None => Ok(PipeMessage::with_request(&input, vec![], None)),
        }
    }
}
