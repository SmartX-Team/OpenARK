use anyhow::Result;
use async_trait::async_trait;
use dash_api::{
    model::ModelCrd, model_storage_binding::ModelStorageBindingStorageSpec,
    storage::object::ModelStorageObjectSpec,
};
use dash_provider::storage::{ObjectStorageClient, ObjectStorageSession};
use dash_provider_api::data::Capacity;
use futures::TryFutureExt;
use kube::Client;
use tracing::{instrument, Level};

#[async_trait]
impl super::GetCapacity for ModelStorageObjectSpec {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn get_capacity<'namespace, 'kube>(
        &self,
        kube: &'kube Client,
        namespace: &'namespace str,
        model: &ModelCrd,
        storage_name: String,
    ) -> Result<Option<Capacity>> {
        let storage = ModelStorageBindingStorageSpec {
            source: None,
            source_binding_name: None,
            target: self,
            target_name: &storage_name,
        };

        let client = ObjectStorageClient::try_new(kube, namespace, None, storage).await?;
        let session = client.get_session(kube, namespace, model);
        session.get_capacity().map_ok(Some).await
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn get_capacity_global<'namespace, 'kube>(
        &self,
        kube: &'kube Client,
        namespace: &'namespace str,
        storage_name: String,
    ) -> Result<Option<Capacity>> {
        let storage =
            ObjectStorageSession::load_storage_provider(kube, namespace, &storage_name, None, self)
                .await?;
        storage.get_capacity_global().map_ok(Some).await
    }
}
