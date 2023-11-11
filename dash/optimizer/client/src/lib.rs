use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use dash_api::{
    model::ModelCrd, model_claim::ModelClaimBindingPolicy,
    model_storage_binding::ModelStorageBindingCrd, storage::ModelStorageKind,
};
use dash_optimizer_api::{
    optimize::{Request, Response},
    topics,
};
use dash_pipe_provider::{
    messengers::{init_messenger, MessengerArgs, Publisher},
    PipeMessage,
};
use dash_provider::storage::KubernetesStorageClient;
use kube::ResourceExt;
use tracing::{info, instrument, Level};

pub struct OptimizerClient {
    publisher: Arc<dyn Publisher>,
}

impl OptimizerClient {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn try_default() -> Result<Self> {
        let args = MessengerArgs::try_parse()?;
        let messenger = init_messenger::<()>(&args).await?;

        Ok(Self {
            publisher: messenger.publish(topics::optimize_model_in()?).await?,
        })
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn optimize_model_storage_binding(
        &self,
        field_manager: &str,
        kubernetes_storage: KubernetesStorageClient<'_, '_>,
        model: &ModelCrd,
        policy: ModelClaimBindingPolicy,
        storage: Option<ModelStorageKind>,
    ) -> Result<Option<ModelStorageBindingCrd>> {
        let request = PipeMessage::<_, ()>::new(
            vec![],
            Request {
                model: Some(model.clone()),
                policy: Some(policy),
                storage,
            },
        );
        match self
            .publisher
            .request_one((&request).try_into()?)
            .await
            .and_then(PipeMessage::<Response, ()>::try_from)
            .map(|message| message.value)
        {
            Ok(Some(storage_binding)) => kubernetes_storage
                .create_model_storage_binding(field_manager, model.name_any(), storage_binding)
                .await
                .map(Some),
            Ok(None) => {
                self.optimize_model_storage_binding_fallback(
                    field_manager,
                    kubernetes_storage,
                    model,
                    storage,
                )
                .await
            }
            Err(error) => {
                info!("failed to optimize model storage binding: {error}");
                self.optimize_model_storage_binding_fallback(
                    field_manager,
                    kubernetes_storage,
                    model,
                    storage,
                )
                .await
            }
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn optimize_model_storage_binding_fallback(
        &self,
        field_manager: &str,
        kubernetes_storage: KubernetesStorageClient<'_, '_>,
        model: &ModelCrd,
        storage: Option<ModelStorageKind>,
    ) -> Result<Option<ModelStorageBindingCrd>> {
        let fallback = ::dash_optimizer_fallback::OptimizerClient::new(kubernetes_storage);
        fallback
            .optimize_model_storage_binding(field_manager, model, storage)
            .await
    }
}
