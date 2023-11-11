use anyhow::Result;
use async_trait::async_trait;
use dash_api::{model::ModelCrd, storage::kubernetes::ModelStorageKubernetesSpec};
use dash_provider::storage::KubernetesStorageClient;
use dash_provider_api::data::Capacity;
use tracing::{instrument, warn, Level};

#[async_trait]
impl super::GetCapacity for ModelStorageKubernetesSpec {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn get_capacity<'namespace, 'kind>(
        &self,
        _kubernetes_storage: KubernetesStorageClient<'namespace, 'kind>,
        _model: &ModelCrd,
        _storage_name: String,
    ) -> Result<Option<Capacity>> {
        warn!("unsupported storage type for fallback optimizer: Kubernetes");
        Ok(None)
    }
}
