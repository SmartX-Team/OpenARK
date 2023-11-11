use anyhow::Result;
use async_trait::async_trait;
use dash_api::{
    model::ModelCrd, model_storage_binding::ModelStorageBindingStorageSpec,
    storage::object::ModelStorageObjectSpec,
};
use dash_provider::storage::{KubernetesStorageClient, ObjectStorageClient};
use dash_provider_api::data::Capacity;
use tracing::{instrument, Level};

#[async_trait]
impl super::GetCapacity for ModelStorageObjectSpec {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn get_capacity<'namespace, 'kind>(
        &self,
        kubernetes_storage: KubernetesStorageClient<'namespace, 'kind>,
        model: &ModelCrd,
        storage_name: String,
    ) -> Result<Option<Capacity>> {
        let KubernetesStorageClient { namespace, kube } = kubernetes_storage;
        let storage = ModelStorageBindingStorageSpec {
            source: None,
            source_binding_name: None,
            target: self,
            target_name: &storage_name,
        };

        let client = ObjectStorageClient::try_new(kube, namespace, storage).await?;
        let session = client.get_session(kube, namespace, model);
        session.get_capacity().await
    }
}
