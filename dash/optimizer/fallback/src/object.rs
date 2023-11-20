use anyhow::Result;
use async_trait::async_trait;
use dash_api::{
    model::ModelCrd, model_storage_binding::ModelStorageBindingStorageSpec,
    storage::object::ModelStorageObjectSpec,
};
use dash_provider::storage::{ObjectStorageClient, ObjectStorageRef};
use dash_provider_api::data::Capacity;
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

        let client = ObjectStorageClient::try_new(kube, namespace, storage).await?;
        let session = client.get_session(kube, namespace, model);
        session.get_capacity().await
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn get_capacity_global<'namespace, 'kube>(
        &self,
        kube: &'kube Client,
        namespace: &'namespace str,
        storage_name: String,
    ) -> Result<Option<Capacity>> {
        let storage =
            ObjectStorageRef::load_storage_provider(kube, namespace, &storage_name, self).await?;
        storage.get_capacity_global().await
    }
}
