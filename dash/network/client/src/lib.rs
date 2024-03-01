use anyhow::Result;
use clap::Parser;
use dash_api::{
    model::ModelCrd, model_claim::ModelClaimBindingPolicy,
    model_storage_binding::ModelStorageBindingCrd, storage::ModelStorageKind,
};
use dash_network_api::model;
use dash_pipe_provider::{
    messengers::{init_messenger, Messenger, MessengerArgs},
    PipeMessage, RemoteFunction, StatelessRemoteFunction,
};
use dash_provider::storage::KubernetesStorageClient;
use kube::ResourceExt;
use tracing::{info, instrument, Level};

pub struct NetworkClient {
    messenger: Box<dyn Messenger<model::Response>>,
}

impl NetworkClient {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn try_default() -> Result<Self> {
        let args = MessengerArgs::try_parse()?;
        let messenger = init_messenger(&args).await?;

        Ok(Self { messenger })
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
        let request = PipeMessage::<_, ()>::new(model::Request {
            model: Some(model.clone()),
            policy,
            storage,
        });

        let function =
            StatelessRemoteFunction::try_new(&self.messenger, model::model_in()?).await?;

        match function
            .call_one(request)
            .await
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
        let fallback = ::dash_network_fallback::NetworkClient::new(kubernetes_storage);
        fallback
            .optimize_model_storage_binding(field_manager, model, storage)
            .await
    }
}
