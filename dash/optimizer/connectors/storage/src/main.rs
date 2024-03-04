use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use clap::Parser;
use dash_api::storage::ModelStorageCrd;
use dash_network_api::model;
use dash_optimizer_connector_storage_client::GetCapacity;
use dash_pipe_provider::{
    connector::Connector, storage::StorageIO, FunctionContext, PipeArgs, PipeMessage, PipeMessages,
};
use dash_provider_api::data::Capacity;
use derivative::Derivative;
use futures::{stream::FuturesUnordered, StreamExt};
use kube::{api::ListParams, Api, Client, ResourceExt};
use serde::{Deserialize, Serialize};
use tracing::error;

fn main() {
    PipeArgs::<Connector<Function>>::from_env()
        .with_model_in(None)
        .with_model_out(model::connector().ok())
        .loop_forever()
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, Parser)]
pub struct FunctionArgs {}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Function {
    api: Api<ModelStorageCrd>,
    #[derivative(Debug = "ignore")]
    kube: Client,
    lp: ListParams,
}

#[async_trait]
impl ::dash_pipe_provider::FunctionBuilder for Function {
    type Args = FunctionArgs;

    async fn try_new(
        FunctionArgs {}: &<Self as ::dash_pipe_provider::FunctionBuilder>::Args,
        _ctx: &mut FunctionContext,
        _storage: &Arc<StorageIO>,
    ) -> Result<Self> {
        let kube = Client::try_default().await?;

        Ok(Self {
            api: Api::all(kube.clone()),
            kube,
            lp: ListParams::default(),
        })
    }
}

#[async_trait]
impl ::dash_pipe_provider::Function for Function {
    type Input = ();
    type Output = Capacity;

    async fn tick(
        &mut self,
        _inputs: PipeMessages<<Self as ::dash_pipe_provider::Function>::Input>,
    ) -> Result<PipeMessages<<Self as ::dash_pipe_provider::Function>::Output>> {
        let kube = &self.kube;
        let items = self.api.list(&self.lp).await?.items;

        Ok(PipeMessages::Batch(
            items
                .into_iter()
                .filter_map(|item| Some((item.namespace()?, item)))
                .map(|(namespace, item)| async move {
                    item.get_capacity_global(kube, &namespace, item.name_any())
                        .await
                })
                .collect::<FuturesUnordered<_>>()
                .filter_map(|result| async move {
                    match result {
                        Ok(capacity) => capacity.map(PipeMessage::new),
                        Err(error) => {
                            error!("{error}");
                            None
                        }
                    }
                })
                .collect::<Vec<_>>()
                .await,
        ))
    }
}
